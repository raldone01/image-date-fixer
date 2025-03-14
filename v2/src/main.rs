#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(missing_docs)]
mod date_extractors;
mod exiftool;

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use chumsky::error::Cheap;
use clap::{Arg, ArgAction, Command, command, value_parser};
use date_extractors::get_date_for_file;
use exiftool::{exif_tool_writable_file_extensions, get_exif_date, has_exiftool, set_exif_date};
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
  process::{self, exit},
  str::FromStr,
  sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  time::{Duration, SystemTime},
};
use tracing::{Level, debug, error, info, trace, warn};
use tracing_subscriber::{self, EnvFilter};
use walkdir::WalkDir;

fn set_modified_time(file_path: &Path, date: &NaiveDateTime, process_state: &ProcessState) -> bool {
  if process_state.dry_run {
    info!(
      "\"{}\": Would set modified time to {}",
      file_path.display(),
      date.format("%Y-%m-%d %H:%M:%S")
    );
    return true;
  }

  let file = std::fs::File::open(file_path);
  let file = match file {
    Ok(file) => file,
    Err(e) => {
      error!("\"{}\": Failed to open file: {}", file_path.display(), e);
      return false;
    },
  };

  let date_time = DateTime::<Utc>::from_naive_utc_and_offset(*date, Utc);
  file.set_modified(date_time.into()).is_ok()
}

fn get_modified_time(file_path: &Path) -> Option<NaiveDateTime> {
  let metadata = std::fs::metadata(file_path);
  let metadata = match metadata {
    Ok(metadata) => metadata,
    Err(e) => {
      error!(
        "\"{}\": Failed to get metadata for file: {}",
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
        "\"{}\": Failed to get modified time for file: {}",
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
  modified_times_future_threshold: NaiveDateTime,
  exif_dates_future_threshold: NaiveDateTime,

  stat_folders_processed: AtomicUsize,
  stat_folders_skipped: AtomicUsize,
  stat_files_processed: AtomicUsize,
  stat_files_skipped: AtomicUsize,
  stat_files_errors: AtomicUsize,
  stat_exif_updated: AtomicUsize,
  stat_exif_overwritten: AtomicUsize,
  stat_modified_time_updated: AtomicUsize,
}

fn pretty_duration(duration: Duration) -> String {
  let mut duration = duration;
  let mut result = String::new();
  if duration.as_secs() >= 86400 {
    let days = duration.as_secs() / 86400;
    result.push_str(&format!("{}d ", days));
    duration -= Duration::from_secs(days * 86400);
  }
  if duration.as_secs() >= 3600 {
    let hours = duration.as_secs() / 3600;
    result.push_str(&format!("{}h ", hours));
    duration -= Duration::from_secs(hours * 3600);
  }
  if duration.as_secs() >= 60 {
    let minutes = duration.as_secs() / 60;
    result.push_str(&format!("{}m ", minutes));
    duration -= Duration::from_secs(minutes * 60);
  }
  result.push_str(&format!("{}s ", duration.as_secs()));
  result
}

impl ProcessState {
  fn pretty_print_stats(&self) {
    let folders_processed = self.stat_folders_processed.load(Ordering::Relaxed);
    let folders_skipped = self.stat_folders_skipped.load(Ordering::Relaxed);
    let files_processed = self.stat_files_processed.load(Ordering::Relaxed);
    let files_skipped = self.stat_files_skipped.load(Ordering::Relaxed);
    let files_errors = self.stat_files_errors.load(Ordering::Relaxed);
    let exif_updated = self.stat_exif_updated.load(Ordering::Relaxed);
    let exif_overwritten = self.stat_exif_overwritten.load(Ordering::Relaxed);
    let modified_time_updated = self.stat_modified_time_updated.load(Ordering::Relaxed);

    println!("Statistics:");
    println!("  Folders processed: {}", folders_processed);
    println!("  Folders skipped: {}", folders_skipped);
    println!("  Files processed: {}", files_processed);
    println!("  Files skipped: {}", files_skipped);
    println!("  Files with errors: {}", files_errors);
    println!("  EXIF dates updated: {}", exif_updated);
    println!("  EXIF dates overwritten: {}", exif_overwritten);
    println!("  Modified times updated: {}", modified_time_updated);
    let std_duration = (Local::now().naive_local() - self.start_time).to_std();
    if let Ok(std_duration) = std_duration {
      println!("  Time taken: {}", pretty_duration(std_duration));
    }
  }
}

fn process_dir(dir: &Path, process_state: &ProcessState) {
  if !process_state.exit_flag.load(Ordering::Relaxed) {
    return;
  }

  if process_state.excluded_files.contains(&dir.to_path_buf()) {
    process_state
      .stat_folders_skipped
      .fetch_add(1, Ordering::Relaxed);
    return;
  }

  process_state
    .stat_folders_processed
    .fetch_add(1, Ordering::Relaxed);

  info!("\"{}\": Processing directory", dir.display());

  let entries = WalkDir::new(dir)
    .into_iter()
    .filter_map(Result::ok)
    .collect::<Vec<_>>();

  entries.par_iter().for_each(|entry| {
    if !process_state.exit_flag.load(Ordering::Relaxed) {
      return;
    }

    let path = entry.path();
    if process_state.excluded_files.contains(&path.to_path_buf()) {
      process_state
        .stat_files_skipped
        .fetch_add(1, Ordering::Relaxed);
      return;
    }

    if path.is_dir() {
      process_dir(path, process_state);
    } else {
      process_file(path, process_state);
    }
  });
}

const OLD_MODIFIED_TIME_THRESHOLD: NaiveDateTime = NaiveDateTime::new(
  NaiveDate::from_ymd_opt(1970, 1, 2).unwrap(),
  NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
);

fn get_confidence_of_naive(naive: &NaiveDateTime) -> date_extractors::DateConfidence {
  if *naive == OLD_MODIFIED_TIME_THRESHOLD {
    return date_extractors::DateConfidence::None;
  }
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

fn process_file(file: &Path, process_state: &ProcessState) {
  info!("\"{}\": Processing file", file.display());

  let original_file_modified_time = get_modified_time(file);

  if let Some(original_file_modified_time) = original_file_modified_time {
    // check if the original modified time is in the future
    if original_file_modified_time > process_state.modified_times_future_threshold {
      info!(
        "\"{}\": File has a modified time in the future: {}. Setting it to the current time.",
        file.display(),
        original_file_modified_time.format("%Y-%m-%d %H:%M:%S")
      );
      if !set_modified_time(file, &process_state.start_time, process_state) {
        error!("\"{}\": Failed to set modified time", file.display());
        process_state
          .stat_files_errors
          .fetch_add(1, Ordering::Relaxed);
        return;
      }
    }
    // check if the original modified time is before 1970-01-02
    else if original_file_modified_time < OLD_MODIFIED_TIME_THRESHOLD {
      info!(
        "\"{}\": File has a modified time before 1970-01-02: {}. Setting it to 1970-01-02.",
        file.display(),
        original_file_modified_time.format("%Y-%m-%d %H:%M:%S")
      );
      if !set_modified_time(file, &OLD_MODIFIED_TIME_THRESHOLD, process_state) {
        error!("\"{}\": Failed to set modified time", file.display());
        process_state
          .stat_files_errors
          .fetch_add(1, Ordering::Relaxed);
        return;
      }
    }
  }

  let file_extension = file
    .extension()
    .and_then(|ext| ext.to_str())
    .map(str::to_ascii_uppercase);

  // check that the file extension is a valid image extension
  if file_extension.is_some_and(|ext| !exif_tool_writable_file_extensions().contains(&ext)) {
    info!(
      "\"{}\": File is not an image file. Skipping.",
      file.display()
    );
    process_state
      .stat_files_skipped
      .fetch_add(1, Ordering::Relaxed);
    return;
  }

  let current_time = Local::now().naive_local();

  // guess the date from the file path
  let file_name = file.file_name().unwrap().to_str().unwrap();
  let guessed_date = get_date_for_file(file, file_name, current_time).or_else(|| {
    let folder_path = file.parent().unwrap();
    let folder_name = folder_path.file_name().unwrap().to_str().unwrap();
    get_date_for_file(folder_path, folder_name, current_time)
  });

  // get the original exif date and its confidence
  let original_exif_date = get_exif_date(file);

  if let Some((date, confidence)) = guessed_date {
    debug!(
      "\"{}\": Guessed date: {} (confidence: {:?})",
      file.display(),
      date.format("%Y-%m-%d %H:%M:%S"),
      confidence
    );
  } else if original_exif_date.is_none() {
    warn!(
      "\"{}\": Could not guess date from file path",
      file.display()
    );
  }

  let mut original_exif_confidence = date_extractors::DateConfidence::None;
  if let Some(original_exif_date) = original_exif_date {
    original_exif_confidence = get_confidence_of_naive(&original_exif_date);
    debug!(
      "\"{}\": Original EXIF date: {} (confidence: {:?})",
      file.display(),
      original_exif_date.format("%Y-%m-%d %H:%M:%S"),
      original_exif_confidence
    );

    // fix future exif dates
    if original_exif_date > process_state.exif_dates_future_threshold {
      info!(
        "\"{}\": File has an EXIF date in the future: {}. Setting it to the current time.",
        file.display(),
        original_exif_date.format("%Y-%m-%d %H:%M:%S")
      );
      if !set_exif_date(file, &process_state.start_time, process_state.dry_run) {
        error!("\"{}\": Failed to set EXIF date", file.display());
        process_state
          .stat_files_errors
          .fetch_add(1, Ordering::Relaxed);
        return;
      }
    }
  }
  if let Some((date, confidence)) = guessed_date {
    if confidence > original_exif_confidence {
      if let Some(original_exif_date) = original_exif_date {
        info!(
          "\"{}\": Overwriting EXIF date {} (confidence: {:?}) with guessed date {} (confidence: {:?})",
          file.display(),
          original_exif_date.format("%Y-%m-%d %H:%M:%S"),
          original_exif_confidence,
          date.format("%Y-%m-%d %H:%M:%S"),
          confidence
        );
      } else {
        info!(
          "\"{}\": Setting EXIF date to guessed date {} (confidence: {:?})",
          file.display(),
          date.format("%Y-%m-%d %H:%M:%S"),
          confidence
        );
      }
      if !set_exif_date(file, &date, process_state.dry_run) {
        error!("\"{}\": Failed to set EXIF date", file.display());
        process_state
          .stat_files_errors
          .fetch_add(1, Ordering::Relaxed);
        return;
      }
      if original_exif_date.is_some() {
        process_state
          .stat_exif_overwritten
          .fetch_add(1, Ordering::Relaxed);
      } else {
        process_state
          .stat_exif_updated
          .fetch_add(1, Ordering::Relaxed);
      }
    }
  }
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
          Arg::new("fix-future-modified-times")
          .long("fix-future-modified-times")
          .help("Fix modified times that are this many days in the future")
          .value_parser(value_parser!(u64)),
        )
        .arg(
          Arg::new("fix-future-exif-dates")
          .long("fix-future-exif-dates")
          .help("Fix exif dates that are this many days in the future")
          .value_parser(value_parser!(u64)),
        )
        .arg(
          Arg::new("dry-run")
          .long("dry-run")
          .help("Perform a dry run")
          .action(ArgAction::SetTrue),
        )
        .arg(
          Arg::new("print-supported-file-extensions")
          .long("print-supported-file-extensions")
          .help("Print the list of supported file extensions")
          .action(ArgAction::SetTrue),
        )
        .arg(
          Arg::new("print-stats")
          .long("print-stats")
          .help("Print statistics")
          .action(ArgAction::SetTrue)
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
    .and_then(|level| Level::from_str(level).ok());
  let mut logging_builder = tracing_subscriber::fmt::fmt();
  if let Some(level) = log_level {
    logging_builder = logging_builder.with_max_level(level);
  }
  logging_builder
    .with_env_filter(EnvFilter::from_default_env())
    .init();

  if !has_exiftool() {
    error!("exiftool is not installed. Make sure it is installed and in your PATH.");
    exit(1);
  }

  let fix_future_modified_times_day_offset =
    matches.get_one::<u64>("fix-future-modified-times").copied();
  let modified_times_future_threshold = fix_future_modified_times_day_offset
    .and_then(|invalid_modified_times_days| {
      Local::now()
        .naive_local()
        .checked_add_days(chrono::Days::new(invalid_modified_times_days))
    })
    .unwrap_or(NaiveDateTime::MAX);

  let fix_future_exif_dates_day_offset = matches.get_one::<u64>("fix-future-exif-dates").copied();
  let exif_dates_future_threshold = fix_future_exif_dates_day_offset
    .and_then(|invalid_exif_dates_days| {
      Local::now()
        .naive_local()
        .checked_add_days(chrono::Days::new(invalid_exif_dates_days))
    })
    .unwrap_or(NaiveDateTime::MAX);

  let dry_run = matches.get_one::<bool>("dry-run").copied().unwrap_or(false);
  let print_supported_file_extensions = matches
    .get_one::<bool>("print-supported-file-extensions")
    .copied()
    .unwrap_or(false);
  let print_stats = matches
    .get_one::<bool>("print-stats")
    .copied()
    .unwrap_or(false);

  if print_supported_file_extensions {
    println!("Supported file extensions:");
    let items_per_line = 10;
    for (i, extension) in exif_tool_writable_file_extensions().iter().enumerate() {
      if i % items_per_line == 0 {
        print!("  ");
      }
      print!("{} ", extension);
      if (i + 1) % items_per_line == 0 {
        println!();
      }
    }
    println!();
  }

  let process_state = Arc::new(ProcessState {
    excluded_files: exclude_files.flatten().cloned().collect(),
    exit_flag: AtomicBool::new(true),
    start_time: Local::now().naive_local(),
    dry_run,
    modified_times_future_threshold,
    exif_dates_future_threshold,

    stat_folders_processed: AtomicUsize::new(0),
    stat_folders_skipped: AtomicUsize::new(0),
    stat_files_processed: AtomicUsize::new(0),
    stat_files_skipped: AtomicUsize::new(0),
    stat_files_errors: AtomicUsize::new(0),
    stat_exif_updated: AtomicUsize::new(0),
    stat_exif_overwritten: AtomicUsize::new(0),
    stat_modified_time_updated: AtomicUsize::new(0),
  });

  let ctrlc_process_state = process_state.clone();
  ctrlc::set_handler(move || {
    println!("\nReceived Ctrl+C! Exiting...");
    ctrlc_process_state
      .exit_flag
      .store(false, Ordering::Relaxed);
  })
  .expect("Error setting Ctrl+C handler");

  files.flatten().par_bridge().for_each(|file| {
    // check if the file is a directory
    if file.is_dir() {
      process_dir(file, &process_state);
    } else {
      process_file(file, &process_state);
    }
  });

  if print_stats {
    process_state.pretty_print_stats();
  }
}
