#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(missing_docs)]

mod date_extractors;
mod exiftool;

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use chumsky::error::Cheap;
use clap::{Arg, ArgAction, Command, command, value_parser};
use date_extractors::{ConfidentNaiveDateTime, DateConfidence, get_date_for_file};
use exiftool::{exif_tool_writable_file_extensions, get_exif_date, has_exiftool, set_exif_date};
use jwalk::WalkDir;
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
  fmt::{Display, Write as _},
  io::{self, Write as _},
  path::{Path, PathBuf},
  process::{self, exit},
  str::FromStr,
  sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  time::{Duration, SystemTime},
};
use tracing::{Level, debug, error, event, info, trace, warn};
use tracing_subscriber::{self, EnvFilter};

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

fn pretty_duration(duration: Duration) -> String {
  let mut duration = duration;
  let mut result = String::new();
  if duration.as_secs() >= 86400 {
    let days = duration.as_secs() / 86400;
    write!(result, "{days}d ").unwrap();
    duration -= Duration::from_secs(days * 86400);
  }
  if duration.as_secs() >= 3600 {
    let hours = duration.as_secs() / 3600;
    write!(result, "{hours}h ").unwrap();
    duration -= Duration::from_secs(hours * 3600);
  }
  if duration.as_secs() >= 60 {
    let minutes = duration.as_secs() / 60;
    write!(result, "{minutes}m ").unwrap();
    duration -= Duration::from_secs(minutes * 60);
  }
  let seconds = duration.as_secs();
  write!(result, "{seconds}s").unwrap();
  result
}

struct ProcessState {
  excluded_files: BTreeSet<PathBuf>,
  skip_hidden_files: bool,
  exit_flag: AtomicBool,
  start_time: NaiveDateTime,
  dry_run: bool,
  modified_times_future_threshold: NaiveDateTime,
  exif_dates_future_threshold: NaiveDateTime,
  ignore_minor_exif_errors: bool,

  stat_folders_processed: AtomicUsize,
  stat_folders_skipped: AtomicUsize,
  stat_files_processed: AtomicUsize,
  stat_files_skipped: AtomicUsize,
  stat_files_errors: AtomicUsize,
  stat_exif_updated: AtomicUsize,
  stat_exif_overwritten: AtomicUsize,
  stat_modified_time_updated: AtomicUsize,
}

impl ProcessState {
  fn new(
    excluded_files: BTreeSet<PathBuf>,
    skip_hidden_files: bool,
    dry_run: bool,
    modified_times_future_threshold: NaiveDateTime,
    exif_dates_future_threshold: NaiveDateTime,
    ignore_minor_exif_errors: bool,
  ) -> Self {
    Self {
      excluded_files: excluded_files,
      skip_hidden_files,
      exit_flag: AtomicBool::new(true),
      start_time: Local::now().naive_utc(),
      dry_run,
      modified_times_future_threshold,
      exif_dates_future_threshold,
      ignore_minor_exif_errors,

      stat_folders_processed: AtomicUsize::new(0),
      stat_folders_skipped: AtomicUsize::new(0),
      stat_files_processed: AtomicUsize::new(0),
      stat_files_skipped: AtomicUsize::new(0),
      stat_files_errors: AtomicUsize::new(0),
      stat_exif_updated: AtomicUsize::new(0),
      stat_exif_overwritten: AtomicUsize::new(0),
      stat_modified_time_updated: AtomicUsize::new(0),
    }
  }

  fn pretty_print_stats(&self) -> Result<(), io::Error> {
    let folders_processed = self.stat_folders_processed.load(Ordering::Relaxed);
    let folders_skipped = self.stat_folders_skipped.load(Ordering::Relaxed);
    let files_processed = self.stat_files_processed.load(Ordering::Relaxed);
    let files_skipped = self.stat_files_skipped.load(Ordering::Relaxed);
    let files_errors = self.stat_files_errors.load(Ordering::Relaxed);
    let exif_updated = self.stat_exif_updated.load(Ordering::Relaxed);
    let exif_overwritten = self.stat_exif_overwritten.load(Ordering::Relaxed);
    let modified_time_updated = self.stat_modified_time_updated.load(Ordering::Relaxed);

    // Acquire a lock on standard output for buffered writing
    let mut stdout = io::stdout().lock();

    writeln!(&mut stdout, "Statistics:")?;
    writeln!(&mut stdout, "  Folders processed: {folders_processed}")?;
    writeln!(&mut stdout, "  Folders skipped: {folders_skipped}")?;
    writeln!(&mut stdout, "  Files processed: {files_processed}")?;
    writeln!(&mut stdout, "  Files skipped: {files_skipped}")?;
    writeln!(&mut stdout, "  Files with errors: {files_errors}")?;
    writeln!(&mut stdout, "  EXIF dates updated: {exif_updated}")?;
    writeln!(&mut stdout, "  EXIF dates overwritten: {exif_overwritten}")?;
    writeln!(
      &mut stdout,
      "  Modified times updated: {modified_time_updated}"
    )?;

    let std_duration = (Local::now().naive_utc() - self.start_time).to_std();
    if let Ok(std_duration) = std_duration {
      writeln!(
        &mut stdout,
        "  Time taken: {}",
        pretty_duration(std_duration)
      )?;
    }

    Ok(())
  }
}

fn process_dir_recursive(root_dir: &Path, process_state: &Arc<ProcessState>) {
  if !process_state.exit_flag.load(Ordering::Relaxed) {
    return;
  }

  if process_state
    .excluded_files
    .contains(&root_dir.to_path_buf())
  {
    process_state
      .stat_folders_skipped
      .fetch_add(1, Ordering::Relaxed);
    return;
  }

  process_state
    .stat_folders_processed
    .fetch_add(1, Ordering::Relaxed);

  info!("\"{}\": Processing top level directory", root_dir.display());

  let entries = {
    let process_state = process_state.clone();
    WalkDir::new(root_dir)
      .skip_hidden(process_state.skip_hidden_files)
      .process_read_dir(move |_depth, _path, _read_dir_state, children| {
        // Filter out excluded directories
        for child in children.iter_mut().flatten() {
          if process_state.excluded_files.contains(&child.path()) {
            child.read_children_path = None;
            process_state
              .stat_folders_skipped
              .fetch_add(1, Ordering::Relaxed);
          }
        }
      })
      .into_iter()
  };

  let _ = entries.par_bridge().try_for_each(|entry_result| {
    let entry = match entry_result {
      Ok(entry) => entry,
      Err(e) => {
        error!(
          "\"{}\": Failed to read entry: {}",
          e.path().map_or_else(
            || "Unknown path".to_string(),
            |path_option| path_option.display().to_string(),
          ),
          e
        );
        process_state
          .stat_files_errors
          .fetch_add(1, Ordering::Relaxed);
        return Ok(());
      },
    };

    if !process_state.exit_flag.load(Ordering::Relaxed) {
      return Err(());
    }

    let path = entry.path();
    if process_state.excluded_files.contains(&path) {
      process_state
        .stat_files_skipped
        .fetch_add(1, Ordering::Relaxed);
      return Ok(());
    }

    if entry.file_type().is_dir() {
      process_state
        .stat_folders_processed
        .fetch_add(1, Ordering::Relaxed);
      trace!("\"{}\": Processing directory", path.display());
    } else if entry.file_type().is_file() {
      process_file(&path, &process_state);
    } else {
      process_state
        .stat_files_skipped
        .fetch_add(1, Ordering::Relaxed);
      warn!("\"{}\": Skipping non-file entry", path.display());
    }

    Ok(())
  });
}

const OLD_MODIFIED_TIME_THRESHOLD: NaiveDateTime = NaiveDateTime::new(
  NaiveDate::from_ymd_opt(1970, 1, 2).unwrap(),
  NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
);

fn get_confidence_of_naive(naive: &NaiveDateTime) -> DateConfidence {
  if *naive == OLD_MODIFIED_TIME_THRESHOLD {
    return DateConfidence::None;
  }
  if naive.second() != 0 {
    return DateConfidence::Second;
  }
  if naive.minute() != 0 {
    return DateConfidence::Minute;
  }
  if naive.hour() != 0 {
    return DateConfidence::Hour;
  }
  if naive.day() != 1 {
    return DateConfidence::Day;
  }
  if naive.month() != 1 {
    return DateConfidence::Month;
  }
  if naive.year() % 10 != 0 {
    return DateConfidence::Year;
  }
  DateConfidence::Decade
}

macro_rules! dyn_event {
    ($lvl:ident, $($arg:tt)+) => {
        match $lvl {
            ::tracing::Level::TRACE => ::tracing::trace!($($arg)+),
            ::tracing::Level::DEBUG => ::tracing::debug!($($arg)+),
            ::tracing::Level::INFO => ::tracing::info!($($arg)+),
            ::tracing::Level::WARN => ::tracing::warn!($($arg)+),
            ::tracing::Level::ERROR => ::tracing::error!($($arg)+),
        }
    };
}

fn process_file(file_path: &Path, process_state: &ProcessState) {
  trace!("\"{}\": Processing file", file_path.display());
  process_state
    .stat_files_processed
    .fetch_add(1, Ordering::Relaxed);

  let mut new_file_modified_time = None;
  let mut new_exif_date = None;

  let original_file_modified_time = get_modified_time(file_path);
  let mut original_exif_date = None;
  let mut guessed_date = None;

  if let Some(original_file_modified_time) = original_file_modified_time {
    // check if the original modified time is in the future
    if original_file_modified_time > process_state.modified_times_future_threshold {
      info!(
        "\"{}\": File has a modified time in the future: {}.",
        file_path.display(),
        original_file_modified_time.format("%Y-%m-%d %H:%M:%S")
      );
      new_file_modified_time = Some(process_state.start_time);
    }
    // check if the original modified time is before 1970-01-02
    else if original_file_modified_time < OLD_MODIFIED_TIME_THRESHOLD {
      info!(
        "\"{}\": File has a modified time before 1970-01-02: {}.",
        file_path.display(),
        original_file_modified_time.format("%Y-%m-%d %H:%M:%S")
      );
      new_file_modified_time = Some(OLD_MODIFIED_TIME_THRESHOLD);
    }
  }

  let file_extension = file_path
    .extension()
    .and_then(|ext| ext.to_str())
    .map(str::to_ascii_uppercase);

  // check that exif tool can work with this file type
  if file_extension.is_some_and(|ext| exif_tool_writable_file_extensions().contains(&ext)) {
    // guess the date from the file path
    let file_name = file_path.file_name().unwrap().to_str().unwrap();
    guessed_date =
      get_date_for_file(file_path, file_name, process_state.start_time).or_else(|| {
        let folder_path = file_path.parent().unwrap();
        let folder_name = folder_path.file_name().unwrap().to_str().unwrap();
        get_date_for_file(folder_path, folder_name, process_state.start_time)
      });

    if let Some(guessed_date) = guessed_date {
      trace!(
        "\"{}\": Guessed date from file name: {} (confidence: {:?})",
        file_path.display(),
        guessed_date.date.format("%Y-%m-%d %H:%M:%S"),
        guessed_date.confidence
      );
    }

    // get the original exif date and its confidence
    original_exif_date =
      get_exif_date(file_path, process_state.ignore_minor_exif_errors).map(|date| {
        let confidence = get_confidence_of_naive(&date);
        ConfidentNaiveDateTime::new(date, confidence)
      });

    if let Some(original_exif_date) = original_exif_date {
      trace!(
        "\"{}\": Original EXIF date: {} (confidence: {:?})",
        file_path.display(),
        original_exif_date.date.format("%Y-%m-%d %H:%M:%S"),
        original_exif_date.confidence
      );
    }
  }

  // fix future exif dates
  if let Some(original_exif_date) = original_exif_date {
    if original_exif_date.date > process_state.exif_dates_future_threshold {
      info!(
        "\"{}\": File has an EXIF date in the future: {}. Setting it to the current time.",
        file_path.display(),
        original_exif_date
      );
      new_exif_date = Some(ConfidentNaiveDateTime::new(
        process_state.start_time,
        DateConfidence::None,
      ));
    }
  }

  if let Some(original_exif_date) = original_exif_date {
    if let Some(guessed_date) = guessed_date {
      if guessed_date.confidence > original_exif_date.confidence
        && guessed_date.date != original_exif_date.date
      {
        new_exif_date = Some(guessed_date);
      }
    }
  } else {
    new_exif_date = new_exif_date.or(guessed_date);
  }

  if original_exif_date.is_none() && new_exif_date.is_none() && guessed_date.is_none() {
    let digit_count_in_file_name = file_path
      .file_name()
      .unwrap()
      .to_str()
      .unwrap()
      .chars()
      .filter(char::is_ascii_digit)
      .count();
    let log_level = if digit_count_in_file_name > 4 {
      Level::DEBUG
    } else {
      Level::TRACE
    };
    dyn_event!(
      log_level,
      "\"{}\": The file does not have an existing EXIF date, and no date could be guessed from the file name.",
      file_path.display()
    );
  }

  // overwrite or set the EXIF date
  if let Some(new_exif_date) = new_exif_date {
    if let Some(original_exif_date) = original_exif_date {
      info!(
        "\"{}\": Overwriting EXIF date {} (confidence: {:?}) with new EXIF date {} (confidence: {:?})",
        file_path.display(),
        original_exif_date.date.format("%Y-%m-%d %H:%M:%S"),
        original_exif_date.confidence,
        new_exif_date.date.format("%Y-%m-%d %H:%M:%S"),
        new_exif_date.confidence
      );
    } else {
      info!(
        "\"{}\": Setting EXIF date to new EXIF date {} (confidence: {:?})",
        file_path.display(),
        new_exif_date.date.format("%Y-%m-%d %H:%M:%S"),
        new_exif_date.confidence
      );
    }

    // write the new exif date
    if set_exif_date(
      file_path,
      &new_exif_date.date,
      process_state.dry_run,
      process_state.ignore_minor_exif_errors,
    ) {
      // update the statistics
      if original_exif_date.is_some() {
        process_state
          .stat_exif_overwritten
          .fetch_add(1, Ordering::Relaxed);
      } else {
        process_state
          .stat_exif_updated
          .fetch_add(1, Ordering::Relaxed);
      }
    } else {
      error!("\"{}\": Failed to set EXIF date", file_path.display());
      process_state
        .stat_files_errors
        .fetch_add(1, Ordering::Relaxed);
    }
  }

  // overwrite the modified time
  if let Some(new_file_modified_time) = new_file_modified_time {
    if set_modified_time(file_path, &new_file_modified_time, process_state) {
      process_state
        .stat_modified_time_updated
        .fetch_add(1, Ordering::Relaxed);
    } else {
      error!("\"{}\": Failed to set modified time", file_path.display());
      process_state
        .stat_files_errors
        .fetch_add(1, Ordering::Relaxed);
    }
  }
}

fn new_argparser() -> clap::Command {
  command!()
  .about("Extracts possible timestamp information from filenames and sets EXIF and modified times accordingly.")
  .arg(
    Arg::new("files")
    .long("files")
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
  .arg(
    Arg::new("skip-hidden-files")
    .long("skip-hidden-files")
    .help("Skip hidden files")
    .action(ArgAction::SetTrue),
  )
  .arg(
    Arg::new("ignore-minor-exif-errors")
    .long("ignore-minor-exif-errors")
    .help("Ignore minor EXIF errors")
    .action(ArgAction::SetTrue),
  )
}

fn main() -> Result<(), io::Error> {
  let matches = new_argparser().get_matches();

  let files = matches
    .get_occurrences::<PathBuf>("files")
    .unwrap_or_default();
  let excluded_files = matches
    .get_occurrences::<PathBuf>("exclude-files")
    .unwrap_or_default()
    .flatten()
    .cloned()
    .collect();

  // set the correct log level
  let log_level = matches
    .get_one::<String>("log-level")
    .and_then(|level| Level::from_str(level).ok());
  let logging_builder = tracing_subscriber::fmt::fmt().with_writer(io::stdout);
  if let Some(level) = log_level {
    logging_builder.with_max_level(level).init();
  } else {
    logging_builder
      .with_env_filter(EnvFilter::from_default_env())
      .init();
  }

  if !has_exiftool() {
    error!("exiftool is not installed. Make sure it is installed and in your PATH.");
    exit(1);
  }

  let fix_future_modified_times_day_offset =
    matches.get_one::<u64>("fix-future-modified-times").copied();
  let modified_times_future_threshold = fix_future_modified_times_day_offset
    .and_then(|invalid_modified_times_days| {
      Local::now()
        .naive_utc()
        .checked_add_days(chrono::Days::new(invalid_modified_times_days))
    })
    .unwrap_or(NaiveDateTime::MAX);

  let fix_future_exif_dates_day_offset = matches.get_one::<u64>("fix-future-exif-dates").copied();
  let exif_dates_future_threshold = fix_future_exif_dates_day_offset
    .and_then(|invalid_exif_dates_days| {
      Local::now()
        .naive_utc()
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
  let skip_hidden_files = matches
    .get_one::<bool>("skip-hidden-files")
    .copied()
    .unwrap_or(false);
  let ignore_minor_exif_errors = matches
    .get_one::<bool>("ignore-minor-exif-errors")
    .copied()
    .unwrap_or(false);

  if print_supported_file_extensions {
    // Acquire a lock on standard output for buffered writing
    let mut stdout = io::stdout().lock();

    writeln!(&mut stdout, "Supported file extensions:")?;
    let items_per_line = 10;
    for (i, extension) in exif_tool_writable_file_extensions().iter().enumerate() {
      if i % items_per_line == 0 {
        write!(&mut stdout, "  ")?;
      }
      write!(&mut stdout, "{extension} ")?;
      if (i + 1) % items_per_line == 0 {
        writeln!(&mut stdout)?;
      }
    }
    writeln!(&mut stdout, "\n")?;
  }

  let process_state = Arc::new(ProcessState::new(
    excluded_files,
    skip_hidden_files,
    dry_run,
    modified_times_future_threshold,
    exif_dates_future_threshold,
    ignore_minor_exif_errors,
  ));

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
      process_dir_recursive(file, &process_state);
    } else {
      process_file(file, &process_state);
    }
  });

  if print_stats {
    process_state.pretty_print_stats()?;
  }

  Ok(())
}
