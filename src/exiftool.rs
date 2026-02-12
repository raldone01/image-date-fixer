use alloc::{borrow::Cow, collections::BTreeSet};
use core::cell::RefCell;
use std::{
  io::{BufRead, BufReader, Write},
  path::Path,
  process::{Child, Command, Stdio},
  sync::OnceLock,
};

use anyhow::Context as _;
use chrono::NaiveDateTime;
use tracing::info;

use crate::{errors::ErrorWithFilePath, tie_command_to_self::tie_command_to_self};

thread_local! {
  static EXIFTOOL: RefCell<RespawningExifToolWorker> = const { RefCell::new(RespawningExifToolWorker::new()) };
}

struct ExifToolWorker {
  process: Child,
  reader: BufReader<std::process::ChildStdout>,
}

impl ExifToolWorker {
  fn new() -> anyhow::Result<Self> {
    let mut command = Command::new("exiftool");
    command
      .arg("-stay_open")
      .arg("True")
      .arg("-@")
      .arg("-")
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null());

    tie_command_to_self(&mut command);

    let mut process = command.spawn().context("Failed to spawn exiftool")?;

    let stdout = process
      .stdout
      .take()
      .context("Failed to capture stdout of exiftool")?;
    Ok(Self {
      process,
      reader: BufReader::new(stdout),
    })
  }

  fn execute(&mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> anyhow::Result<String> {
    let stdin = self
      .process
      .stdin
      .as_mut()
      .context("Failed to capture stdin")?;

    for arg in args {
      writeln!(stdin, "{}", arg.as_ref()).context("Failed to write args to exiftool")?;
    }
    writeln!(stdin, "-execute").context("Failed to execute command")?;

    let mut output = String::new();
    let mut line = String::new();

    loop {
      line.clear();
      let bytes_read = self
        .reader
        .read_line(&mut line)
        .context("Failed to read from exiftool")?;
      if bytes_read == 0 {
        return Err(anyhow::anyhow!(
          "ExifTool process exited unexpectedly (EOF)"
        ));
      }

      if line.trim() == "{ready}" {
        break;
      }
      output.push_str(&line);
    }
    Ok(output.trim().to_string())
  }

  /// Check if the exiftool process is still running.
  #[must_use]
  fn is_running(&mut self) -> bool {
    match self.process.try_wait() {
      Ok(Some(_)) => false,
      Ok(None) => true,
      Err(_) => false,
    }
  }
}

impl Drop for ExifToolWorker {
  fn drop(&mut self) {
    if let Some(mut stdin) = self.process.stdin.take() {
      let _ = writeln!(stdin, "-stay_open\nFalse");
    }
    let _ = self.process.wait();
  }
}

struct RespawningExifToolWorker {
  cached_worker: Option<ExifToolWorker>,
}

impl RespawningExifToolWorker {
  #[must_use]
  const fn new() -> Self {
    Self {
      cached_worker: None,
    }
  }

  fn execute(&mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> anyhow::Result<String> {
    let mut exif_tool_worker = match self.cached_worker.take() {
      Some(worker) => worker,
      None => ExifToolWorker::new()?,
    };
    let execute_result = exif_tool_worker.execute(args);
    // Only put the worker back into the cache if the error was not due to the process exiting unexpectedly
    if exif_tool_worker.is_running() {
      self.cached_worker = Some(exif_tool_worker);
    }
    execute_result
  }
}

#[must_use]
pub fn has_exiftool() -> bool {
  Command::new("exiftool")
    .arg("-ver")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

pub fn get_exif_date(
  file_path: &Path,
  ignore_minor_exif_errors: bool,
) -> Result<Option<NaiveDateTime>, ErrorWithFilePath> {
  EXIFTOOL.with_borrow_mut(|et| {
    let mut args = Vec::new();
    if ignore_minor_exif_errors {
      args.push(Cow::Borrowed("-m"));
    }
    args.push(Cow::Borrowed("-DateTimeOriginal"));
    args.push(Cow::Borrowed("-d"));
    args.push(Cow::Borrowed("%Y-%m-%d %H:%M:%S"));
    args.push(Cow::Borrowed("-s3"));
    args.push(file_path.to_string_lossy());

    let date_str = et
      .execute(args)
      .context("Failed to execute exiftool to get EXIF date")
      .map_err(ErrorWithFilePath::from_source(file_path))?;

    if date_str.is_empty() {
      return Ok(None);
    }

    let date_str = date_str.trim();
    Ok(Some(
      NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
        .context("Failed to parse the EXIF date returned by exiftool")
        .map_err(ErrorWithFilePath::from_source(file_path))?,
    ))
  })
}

pub fn set_exif_date(
  file_path: &Path,
  date: &NaiveDateTime,
  dry_run: bool,
  ignore_minor_exif_errors: bool,
) -> Result<(), ErrorWithFilePath> {
  if dry_run {
    info!(
      file_path = %file_path.display(),
      "Would set EXIF date to {}",
      date.format("%Y-%m-%d %H:%M:%S"),
    );
    return Ok(());
  }

  let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();

  EXIFTOOL.with_borrow_mut(|et| {
    let mut args = Vec::new();
    if ignore_minor_exif_errors {
      args.push(Cow::Borrowed("-m"));
    }
    args.push(Cow::Borrowed("-overwrite_original"));
    args.push(Cow::Owned(format!("-DateTimeOriginal={date_str}")));
    args.push(file_path.to_string_lossy());

    let output = et
      .execute(args)
      .context("Failed to execute exiftool to set EXIF date")
      .map_err(ErrorWithFilePath::from_source(file_path))?;

    if output.contains("1 image files updated") {
      Ok(())
    } else {
      Err(ErrorWithFilePath::new(
        file_path,
        anyhow::anyhow!("Failed to set EXIF date to {date_str}. exiftool output: {output}"),
      ))
    }
  })
}

pub fn repair_exif_errors(file_path: &Path, dry_run: bool) -> Result<(), ErrorWithFilePath> {
  if dry_run {
    info!(
      file_path = %file_path.display(),
      "Would attempt to repair EXIF errors",
    );
    return Ok(());
  }

  EXIFTOOL.with_borrow_mut(|et| {
    let args = [
      Cow::Borrowed("-m"),
      Cow::Borrowed("-overwrite_original"),
      Cow::Borrowed("-exif:all="),
      Cow::Borrowed("-tagsfromfile"),
      Cow::Borrowed("@"),
      Cow::Borrowed("-all:all"),
      Cow::Borrowed("-unsafe"),
      Cow::Borrowed("-icc_profile"),
      file_path.to_string_lossy(),
    ];

    let output = et
      .execute(args)
      .context("Failed to execute exiftool to repair EXIF errors")
      .map_err(ErrorWithFilePath::from_source(file_path))?;

    if output.contains("1 image files updated") {
      info!(
        file_path = %file_path.display(),
        "Successfully repaired EXIF errors",
      );
      Ok(())
    } else {
      Err(ErrorWithFilePath::new(
        file_path,
        anyhow::anyhow!("Failed to repair EXIF errors. exiftool output: {output}"),
      ))
    }
  })
}

pub fn wrap_with_exiftool_repair<R>(
  file_path: &Path,
  repair_exif_errors: bool,
  dry_run: bool,
  closure: impl Fn() -> Result<R, ErrorWithFilePath>,
) -> Result<R, ErrorWithFilePath> {
  let closure_result = closure();

  if !repair_exif_errors {
    return closure_result;
  }

  match closure_result {
    Ok(result) => Ok(result),
    Err(mut e) => {
      e = e.context("Operation failed, and repair_exif_errors is enabled, attempting to repair EXIF errors and retry...");
      e.log_error();
      self::repair_exif_errors(file_path, dry_run)?;
      closure().map_err(|e| e.context("Operation still failed after repairing EXIF errors"))
    },
  }
}

fn exif_tool_writable_file_extensions_internal() -> anyhow::Result<BTreeSet<String>> {
  // run exiftool to get the list of writable file extensions
  EXIFTOOL.with_borrow_mut(|et| {
    let output_str = et.execute(["-listwf"])?;
    let mut extensions = BTreeSet::new();
    for line_str in output_str.lines() {
      if line_str.starts_with("Writable file extensions:") {
        continue;
      }
      for extension in line_str.split_whitespace() {
        extensions.insert(extension.to_string());
      }
    }
    // We remove file types that are supported by exiftool but make no sense for our program
    extensions.remove("PDF"); // PDF files don't support the DateTimeOriginal
    extensions.remove("PSC");
    Ok(extensions)
  })
}

pub fn exif_tool_writable_file_extensions() -> anyhow::Result<&'static BTreeSet<String>> {
  static WRITABLE_EXTENSIONS: OnceLock<BTreeSet<String>> = OnceLock::new();

  // TODO: use get_or_init once it is stabilized: https://github.com/rust-lang/rust/issues/109737
  //WRITABLE_EXTENSIONS.get_or_try_init(exif_tool_writable_file_extensions_internal)

  if let Some(value) = WRITABLE_EXTENSIONS.get() {
    return Ok(value);
  }

  let value = exif_tool_writable_file_extensions_internal()?;
  let _ = WRITABLE_EXTENSIONS.set(value);

  Ok(WRITABLE_EXTENSIONS.get().unwrap())
}
