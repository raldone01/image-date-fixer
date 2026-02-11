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
use tracing::{error, info};

thread_local! {
  static EXIFTOOL: RefCell<Option<ExifToolWorker>> = const { RefCell::new(None) };
}

struct ExifToolWorker {
  process: Child,
  reader: BufReader<std::process::ChildStdout>,
}

impl ExifToolWorker {
  fn new() -> anyhow::Result<Self> {
    let mut process = Command::new("exiftool")
      .arg("-stay_open")
      .arg("True")
      .arg("-@")
      .arg("-")
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()
      .context("Failed to spawn exiftool")?;

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
      writeln!(stdin, "{}", arg.as_ref()).context("Failed to write to exiftool")?;
    }
    writeln!(stdin, "-execute").context("Failed to execute command")?;

    let mut output = String::new();
    let mut line = String::new();
    loop {
      line.clear();
      if self.reader.read_line(&mut line).unwrap_or(0) == 0 {
        break;
      }
      if line.trim() == "{ready}" {
        break;
      }
      output.push_str(&line);
    }
    Ok(output.trim().to_string())
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

fn get_exif_tool(et: &mut Option<ExifToolWorker>) -> anyhow::Result<&mut ExifToolWorker> {
  if et.is_none() {
    *et = Some(ExifToolWorker::new()?);
  }
  Ok(et.as_mut().unwrap())
}

pub fn get_exif_date(
  file: &Path,
  ignore_minor_exif_errors: bool,
) -> anyhow::Result<Option<NaiveDateTime>> {
  EXIFTOOL.with_borrow_mut(|et| {
    let et = get_exif_tool(et)?;

    let mut args = Vec::new();
    if ignore_minor_exif_errors {
      args.push(Cow::Borrowed("-m"));
    }
    args.push(Cow::Borrowed("-DateTimeOriginal"));
    args.push(Cow::Borrowed("-d"));
    args.push(Cow::Borrowed("%Y-%m-%d %H:%M:%S"));
    args.push(Cow::Borrowed("-s3"));
    args.push(file.to_string_lossy());

    let date_str = et.execute(args)?;

    if date_str.is_empty() {
      return Ok(None);
    }

    let date_str = date_str.trim();
    Ok(NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S").ok())
  })
}

pub fn set_exif_date(
  file: &Path,
  date: &NaiveDateTime,
  dry_run: bool,
  ignore_minor_exif_errors: bool,
) -> anyhow::Result<bool> {
  if dry_run {
    info!(
      "\"{}\": Would set EXIF date to {}",
      file.display(),
      date.format("%Y-%m-%d %H:%M:%S")
    );
    return Ok(true);
  }

  let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();

  EXIFTOOL.with_borrow_mut(|et| {
    let et = get_exif_tool(et)?;

    let mut args = Vec::new();
    if ignore_minor_exif_errors {
      args.push(Cow::Borrowed("-m"));
    }
    args.push(Cow::Borrowed("-overwrite_original"));
    args.push(Cow::Owned(format!("-DateTimeOriginal={date_str}")));
    args.push(file.to_string_lossy());

    let output = et.execute(args)?;

    Ok(if output.contains("1 image files updated") {
      true
    } else {
      error!(
        "\"{}\": Failed to set EXIF date to {}. exiftool output: {}",
        file.display(),
        date_str,
        output
      );
      false
    })
  })
}

fn exif_tool_writable_file_extensions_internal() -> anyhow::Result<BTreeSet<String>> {
  // run exiftool to get the list of writable file extensions
  EXIFTOOL.with_borrow_mut(|et| {
    let et = get_exif_tool(et)?;
    let output_str = et.execute(vec!["-listwf".to_string()])?;
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
