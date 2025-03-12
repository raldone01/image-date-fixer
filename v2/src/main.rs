#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(missing_docs)]
mod date_extractors;

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use chumsky::error::Cheap;
use clap::{Arg, ArgAction, Command, command, value_parser};
use ctrlc;
use nom::{
  IResult,
  bytes::complete::{tag, take},
  character::complete::char,
  combinator::{map_opt, map_res},
  error::Error,
  multi::many0,
};
use rayon::prelude::*;
use regex::Regex;
use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
  process,
  str::FromStr,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  time::{Duration, SystemTime},
};
use tracing::{Level, debug, error, info, trace, warn};
use tracing_subscriber::{self, EnvFilter};
use walkdir::WalkDir;

fn get_date_for_file(
  file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, date_extractors::DateConfidence)> {
  let handler_functions = vec![date_extractors::get_date_from_android_filepath_nom];

  for handler in handler_functions {
    let ret = handler(file_path, file_name);
    if ret.is_some() {
      return ret;
    }
  }
  return None;
}

fn has_exiftool() -> bool {
  let output = process::Command::new("exiftool")
    .arg("-ver")
    .output()
    .expect("Failed to run exiftool");

  output.status.success()
}

fn get_exif_date(file: &Path) -> Option<NaiveDateTime> {
  let output = process::Command::new("exiftool")
    .arg("-DateTimeOriginal")
    .arg("-d")
    .arg("%Y-%m-%d %H:%M:%S")
    .arg("-s3")
    .arg(file)
    .output()
    .expect("Failed to run exiftool");

  if !output.status.success() {
    error!(
      "Failed to get EXIF date for file {}: {}",
      file.display(),
      String::from_utf8(output.stderr).unwrap()
    );
    return None;
  }

  let date_str = String::from_utf8(output.stdout).unwrap();
  NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M:%S").ok()
}

fn set_exif_date(file: &Path, date: &NaiveDateTime, process_state: &ProcessState) -> bool {
  if process_state.dry_run {
    info!(
      "Would set EXIF date for file {} to {}",
      file.display(),
      date.format("%Y-%m-%d %H:%M:%S")
    );
    return true;
  }

  let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
  let output = process::Command::new("exiftool")
    .arg("-overwrite_original")
    .arg("-DateTimeOriginal=")
    .arg(&date_str)
    .arg(file)
    .output()
    .expect("Failed to run exiftool");

  if !output.status.success() {
    error!(
      "Failed to set EXIF date for file {}: {}",
      file.display(),
      String::from_utf8(output.stderr).unwrap()
    );
    return false;
  }

  true
}

fn set_modified_time(file_path: &Path, date: &NaiveDateTime, process_state: &ProcessState) -> bool {
  if process_state.dry_run {
    info!(
      "Would set modified time for file {} to {}",
      file_path.display(),
      date.format("%Y-%m-%d %H:%M:%S")
    );
    return true;
  }

  let file = std::fs::File::open(file_path);
  let file = match file {
    Ok(file) => file,
    Err(e) => {
      error!("Failed to open file {}: {}", file_path.display(), e);
      return false;
    },
  };

  let date_time = DateTime::<Utc>::from_naive_utc_and_offset(date.clone(), Utc);
  file.set_modified(date_time.into()).is_ok()
}

fn get_modified_time(file_path: &Path) -> Option<NaiveDateTime> {
  let metadata = std::fs::metadata(file_path);
  let metadata = match metadata {
    Ok(metadata) => metadata,
    Err(e) => {
      error!(
        "Failed to get metadata for file {}: {}",
        file_path.display(),
        e
      );
      return None;
    },
  };

  let modified_time = metadata.modified();
  let modified_time = match modified_time {
    Ok(modified_time) => modified_time,
    Err(e) => {
      error!(
        "Failed to get modified time for file {}: {}",
        file_path.display(),
        e
      );
      return None;
    },
  };

  let modified_date_time = DateTime::<Utc>::from(modified_time);
  Some(modified_date_time.naive_utc())
}

struct ProcessState {
  excluded_files: BTreeSet<PathBuf>,
  exit_flag: AtomicBool,
  start_time: NaiveDateTime,
  dry_run: bool,

  folders_processed: AtomicUsize,
  folders_skipped: AtomicUsize,
  files_processed: AtomicUsize,
  files_skipped: AtomicUsize,
  files_failed: AtomicUsize,
  files_already_processed: AtomicUsize,
  exif_updated: AtomicUsize,
  exif_overwritten: AtomicUsize,
  modified_time_updated: AtomicUsize,
}

fn process_dir(
  dir: &Path,
  fix_future_dates: Option<i32>,
  dry_run: bool,
  process_state: &ProcessState,
) {
  if !process_state.exit_flag.load(Ordering::Relaxed) {
    return;
  }

  if process_state.excluded_files.contains(&dir.to_path_buf()) {
    process_state
      .folders_skipped
      .fetch_add(1, Ordering::Relaxed);
    return;
  }

  process_state
    .folders_processed
    .fetch_add(1, Ordering::Relaxed);

  info!("Processing directory: {}", dir.display());

  let entries = WalkDir::new(dir)
    .into_iter()
    .filter_map(|entry| entry.ok())
    .collect::<Vec<_>>();

  entries.par_iter().for_each(|entry| {
    if !process_state.exit_flag.load(Ordering::Relaxed) {
      return;
    }

    let path = entry.path();
    if process_state.excluded_files.contains(&path.to_path_buf()) {
      process_state.files_skipped.fetch_add(1, Ordering::Relaxed);
      return;
    }

    if path.is_dir() {
      process_dir(path, fix_future_dates, dry_run, process_state);
    } else {
      process_file(path, fix_future_dates, dry_run, process_state);
    }
  });
}

fn get_confidence_of_naive(naive: &NaiveDateTime) -> date_extractors::DateConfidence {
  if naive.second() != 0 {
    return date_extractors::DateConfidence::Second;
  }
  if naive.minute() != 0 {
    return date_extractors::DateConfidence::Minute;
  }
  if naive.hour() != 0 {
    return date_extractors::DateConfidence::Hour;
  }
  if naive.day() != 1 {
    return date_extractors::DateConfidence::Day;
  }
  if naive.month() != 1 {
    return date_extractors::DateConfidence::Month;
  }
  if naive.year() % 10 != 0 {
    return date_extractors::DateConfidence::Year;
  }
  date_extractors::DateConfidence::Decade
}

fn process_file(
  file: &Path,
  fix_future_dates: Option<i32>,
  dry_run: bool,
  process_state: &ProcessState,
) {
  todo!()
}

fn main() {
  let matches = command!()
        .about("Extracts possible timestamp information from filenames and sets EXIF and modified times accordingly.")
        .arg(
            Arg::new("file")
                .long("file")
                .help("Files or directories to process")
                .num_args(1..)
                .value_name("FILES")
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("exclude-files")
                .long("exclude-files")
                .help("Files or directories to exclude")
                .num_args(1..)
                .value_name("FILES")
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("log-level")
                .long("log-level")
                .help("Log level")
                .value_parser(["TRACE", "DEBUG", "INFO", "WARNING", "ERROR"]),
        )
        .arg(
            Arg::new("fix-future-dates")
                .long("fix-future-dates")
                .help("Fix dates that are this many days in the future")
                .value_parser(value_parser!(i32)),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Perform a dry run")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

  let files = matches
    .get_occurrences::<PathBuf>("file")
    .unwrap_or_default();
  let exclude_files = matches
    .get_occurrences::<PathBuf>("exclude-files")
    .unwrap_or_default();

  // set the correct log level
  let log_level = matches
    .get_one::<String>("log-level")
    .and_then(|level| Level::from_str(&level).ok());
  let mut logging_builder = tracing_subscriber::fmt::fmt();
  if let Some(level) = log_level {
    logging_builder = logging_builder.with_max_level(level);
  }
  logging_builder
    .with_env_filter(EnvFilter::from_default_env())
    .init();

  if !has_exiftool() {
    error!("exiftool is not installed. Please install it and try again.");
    process::exit(1);
  }

  let fix_future_dates = matches.get_one::<i32>("fix-future-dates").copied();

  let dry_run = matches.get_one::<bool>("dry-run").copied().unwrap_or(false);

  let process_state = Arc::new(ProcessState {
    excluded_files: exclude_files.flatten().cloned().collect(),
    exit_flag: AtomicBool::new(true),
    start_time: chrono::Local::now().naive_local(),
    dry_run,

    folders_processed: AtomicUsize::new(0),
    folders_skipped: AtomicUsize::new(0),
    files_processed: AtomicUsize::new(0),
    files_skipped: AtomicUsize::new(0),
    files_failed: AtomicUsize::new(0),
    files_already_processed: AtomicUsize::new(0),
    exif_updated: AtomicUsize::new(0),
    exif_overwritten: AtomicUsize::new(0),
    modified_time_updated: AtomicUsize::new(0),
  });

  let ctrlc_process_state = process_state.clone();
  ctrlc::set_handler(move || {
    println!("\nReceived Ctrl+C! Exiting...");
    ctrlc_process_state
      .exit_flag
      .store(false, Ordering::Relaxed);
  })
  .expect("Error setting Ctrl+C handler");

  files.flatten().par_bridge().for_each(move |file| {
    // check if the file is a directory
    if file.is_dir() {
      process_dir(&file, fix_future_dates, dry_run, &process_state);
    } else {
      process_file(&file, fix_future_dates, dry_run, &process_state);
    }
  });
}
