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

struct CommandOutput {
  stdout: String,
  stderr: String,
}

struct ExifToolWorker {
  process: Child,
  stdout_reader: BufReader<std::process::ChildStdout>,
  stderr_reader: BufReader<std::process::ChildStderr>,
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
      .stderr(Stdio::piped());

    tie_command_to_self(&mut command);

    let mut process = command.spawn().context("Failed to spawn exiftool")?;

    let stdout = process
      .stdout
      .take()
      .context("Failed to capture stdout of exiftool")?;

    let stderr = process
      .stderr
      .take()
      .context("Failed to capture stderr of exiftool")?;

    Ok(Self {
      process,
      stdout_reader: BufReader::new(stdout),
      stderr_reader: BufReader::new(stderr),
    })
  }

  fn execute(&mut self, args: &[impl AsRef<str>]) -> anyhow::Result<CommandOutput> {
    let stdin = self
      .process
      .stdin
      .as_mut()
      .context("Failed to capture stdin")?;

    tracing::trace!(
      "exiftool {}",
      args
        .iter()
        .map(|s| format!("\"{}\"", s.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
    );

    for arg in args {
      writeln!(stdin, "{}", arg.as_ref()).context("Failed to write args to exiftool")?;
    }

    // Instruct exiftool to echo a sentinel to stderr so we know when to stop reading.
    // {{ready}} escapes to {ready} in the format string.
    writeln!(stdin, "-echo4\n{{ready}}").context("Failed to write stderr sentinel")?;
    writeln!(stdin, "-execute").context("Failed to execute command")?;

    // Read Stdout
    let mut stdout_string = String::new();
    let mut line = String::new();
    let mut unexpected_eof_in_stdout = false;
    loop {
      line.clear();
      let bytes_read = self.stdout_reader.read_line(&mut line).with_context(|| {
        format!("Failed to read from exiftool stdout. Partial stdout:\n{stdout_string}")
      })?;

      if bytes_read == 0 {
        unexpected_eof_in_stdout = true;
        break;
      }

      if line.trim() == "{ready}" {
        break;
      }
      stdout_string.push_str(&line);
    }

    // Read Stderr
    let mut stderr_string = String::new();
    loop {
      line.clear();
      let bytes_read = self.stderr_reader.read_line(&mut line).with_context(|| {
        format!("Failed to read from exiftool stderr. Partial stderr:\n{stderr_string}\n{}stdout:\n{stdout_string}", if unexpected_eof_in_stdout { "Unexpected EOF while reading exiftool stdout. " } else { "" })
      })?;

      // If we hit EOF here but stdout finished successfully,
      // it usually means the pipe closed or process is shutting down.
      if bytes_read == 0 {
        break;
      }

      if line.trim() == "{ready}" {
        break;
      }
      stderr_string.push_str(&line);
    }

    if unexpected_eof_in_stdout {
      anyhow::bail!(
        "Unexpected EOF while reading exiftool stdout. Stderr:\n{stderr_string}\nPartial stdout:\n{stdout_string}"
      );
    }

    Ok(CommandOutput {
      stdout: stdout_string,
      stderr: stderr_string,
    })
  }

  /// Check if the exiftool process is still running.
  #[must_use]
  fn is_running(&mut self) -> bool {
    match self.process.try_wait() {
      Ok(None) => true,
      Err(_) | Ok(Some(_)) => false,
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

  fn execute(&mut self, args: &[impl AsRef<str>]) -> anyhow::Result<CommandOutput> {
    let mut exiftool_worker = match self.cached_worker.take() {
      Some(worker) => worker,
      None => ExifToolWorker::new()?,
    };
    let execute_result = exiftool_worker.execute(args);
    // Only put the worker back into the cache if the error was not due to the process exiting unexpectedly
    if exiftool_worker.is_running() {
      self.cached_worker = Some(exiftool_worker);
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
    .is_ok_and(|s| s.success())
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
    if file_path.to_str().is_none() {
      tracing::warn!(
        file_path = %file_path.display(),
        "File path is not valid UTF-8, exiftool may not be able to process it",
      );
    }
    args.push(file_path.to_string_lossy());

    let exiftool_output = et
      .execute(&args)
      .context("Failed to execute exiftool to get EXIF date")
      .map_err(ErrorWithFilePath::from_source(file_path))?;
    let exiftool_stdout = exiftool_output.stdout.trim();
    let exiftool_stderr = exiftool_output.stderr.trim();

    if exiftool_stdout.is_empty() {
      // If there is no DateTimeOriginal tag, exiftool returns an empty string.
      return Ok(None);
    }

    // On success the exiftool output is the date_str
    Ok(Some(
      NaiveDateTime::parse_from_str(exiftool_stdout, "%Y-%m-%d %H:%M:%S")
        .with_context(|| {
          format!(
            "Failed to parse the EXIF date. exiftool stderr:\n{exiftool_stderr}\nstdout:\n{exiftool_stdout}"
          )
        })
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

    let exiftool_output = et
      .execute(&args)
      .context("Failed to execute exiftool to set EXIF date")
      .map_err(ErrorWithFilePath::from_source(file_path))?;
    let exiftool_stdout = exiftool_output.stdout.trim();
    let exiftool_stderr = exiftool_output.stderr.trim();

    if exiftool_stdout.contains("1 image files updated") {
      Ok(())
    } else {
      Err(ErrorWithFilePath::new(
        file_path,
        anyhow::anyhow!(
          "Failed to set EXIF date to {date_str}. exiftool stderr:\n{exiftool_stderr}\nstdout:\n{exiftool_stdout}"
        ),
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

    let exiftool_output = et
      .execute(&args)
      .context("Failed to execute exiftool to repair EXIF errors")
      .map_err(ErrorWithFilePath::from_source(file_path))?;
    let exiftool_stdout = exiftool_output.stdout.trim();
    let exiftool_stderr = exiftool_output.stderr.trim();

    if exiftool_stdout.contains("1 image files updated") {
      info!(
        file_path = %file_path.display(),
        "Successfully repaired EXIF errors",
      );
      Ok(())
    } else {
      Err(ErrorWithFilePath::new(
        file_path,
        anyhow::anyhow!(
          "Failed to repair EXIF errors. exiftool stderr:\n{exiftool_stderr}\nstdout:\n{exiftool_stdout}"
        ),
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

fn exiftool_writable_file_extensions_internal() -> anyhow::Result<BTreeSet<String>> {
  // run exiftool to get the list of writable file extensions
  EXIFTOOL.with_borrow_mut(|et| {
    let exiftool_output = et.execute(&["-listwf"])?;
    let exiftool_stdout = exiftool_output.stdout.trim();

    let mut extensions = BTreeSet::new();
    for line_str in exiftool_stdout.lines() {
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

pub fn exiftool_writable_file_extensions() -> anyhow::Result<&'static BTreeSet<String>> {
  static WRITABLE_EXTENSIONS: OnceLock<BTreeSet<String>> = OnceLock::new();

  // TODO: use get_or_init once it is stabilized: https://github.com/rust-lang/rust/issues/109737
  //WRITABLE_EXTENSIONS.get_or_try_init(exiftool_writable_file_extensions_internal)

  if let Some(value) = WRITABLE_EXTENSIONS.get() {
    return Ok(value);
  }

  let value = exiftool_writable_file_extensions_internal()?;
  let _ = WRITABLE_EXTENSIONS.set(value);

  Ok(WRITABLE_EXTENSIONS.get().unwrap())
}
