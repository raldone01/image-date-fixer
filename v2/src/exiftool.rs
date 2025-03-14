use std::{collections::BTreeSet, path::Path, process, sync::LazyLock};

use chrono::NaiveDateTime;
use tracing::{error, info};

pub fn has_exiftool() -> bool {
  let output = process::Command::new("exiftool")
    .arg("-ver")
    .output()
    .expect("Failed to run exiftool");

  output.status.success()
}

pub fn get_exif_date(file: &Path) -> Option<NaiveDateTime> {
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
      "\"{}\": Failed to get EXIF date. exiftool output: {}",
      file.display(),
      String::from_utf8(output.stderr).unwrap()
    );
    return None;
  }

  let date_str = String::from_utf8(output.stdout).unwrap();
  NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M:%S").ok()
}

pub fn set_exif_date(file: &Path, date: &NaiveDateTime, dry_run: bool) -> bool {
  if dry_run {
    info!(
      "\"{}\": Would set EXIF date to {}",
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
      "\"{}\": Failed to set EXIF date to {}. exiftool output: {}",
      file.display(),
      date_str,
      String::from_utf8(output.stderr).unwrap()
    );
    return false;
  }

  true
}

pub fn exif_tool_writable_file_extensions() -> &'static BTreeSet<String> {
  static SUPPORTED_EXTENSIONS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    // run exiftool to get the list of writable file extensions
    let output = process::Command::new("exiftool")
      .arg("-listwf")
      .output()
      .expect("Failed to run exiftool");

    if !output.status.success() {
      error!(
        "Failed to get list of writable file extensions. exiftool output: {}",
        String::from_utf8(output.stderr).unwrap()
      );
      process::exit(1);
    }

    let output_str = String::from_utf8(output.stdout).unwrap();
    let mut extensions = BTreeSet::new();
    for line in output_str.lines() {
      if line.starts_with("Writable file extensions:") {
        continue;
      }
      for extension in line.split_whitespace() {
        extensions.insert(extension.to_string());
      }
    }
    extensions
  });
  &SUPPORTED_EXTENSIONS
}
