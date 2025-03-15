use super::{ChumError, DateConfidence, get_date_for_file};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
use nom::IResult;
use regex::Regex;
use std::{path::Path, str::FromStr, sync::LazyLock};

/// Extracts the date from screenshot prefixed image file paths.
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/Screenshot 2020-09-15 191156.png
///   * /storage/emulated/0/DCIM/Camera/Screenshot_20240720_020223_Jerboa.png
///   * /storage/emulated/0/DCIM/Camera/Screenshot 2023-08-22 121704_windows_big_ad.png
///   * /storage/emulated/0/DCIM/Camera/Screenshot_20241108_094517_Mull.jpg
///   * /storage/emulated/0/DCIM/Camera/screenshot_20241108_094517_Mull.jpg
///   * /storage/emulated/0/DCIM/Camera/screenshot-20241108_094517_Mull.jpg
/// Unsupported:
///   * /storage/emulated/0/DCIM/Camera/Screenshot_312.png
pub fn get_date_from_screenshot_prefixed_filepath_regex(
  file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(screenshot[-_\s])").unwrap());
  let captures = RE.captures(file_name)?;

  let prefix = captures.get(1)?.as_str();
  let unprefixed_file_name = file_name.strip_prefix(prefix).unwrap();
  let unprefixed_file_path = file_path.with_file_name(unprefixed_file_name);

  get_date_for_file(
    &unprefixed_file_path,
    unprefixed_file_name,
    NaiveDateTime::MAX,
  )
}

#[cfg(test)]
pub mod test {
  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_SCREENSHOT_PREFIXED_FILEPATH: LazyLock<Vec<TestCase>> = LazyLock::new(|| {
    vec![
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/Screenshot_312.png",
        result: None,
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/Screenshot 2020-09-15 191156.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-09-15 19:11:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/Screenshot_20240720_020223_Jerboa.png",
        result: Some((
          NaiveDateTime::parse_from_str("2024-07-20 02:02:23", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/Screenshot 2023-08-22 121704_windows_big_ad.png",
        result: Some((
          NaiveDateTime::parse_from_str("2023-08-22 12:17:04", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/Screenshot_20241108_094517_Mull.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("2024-11-08 09:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/screenshot_20241108_094517_Mull.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("2024-11-08 09:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/screenshot-20241108_094517_Mull.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("2024-11-08 09:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
    ]
  });

  #[test]
  fn screenshot_prefixed_filepath_regex() {
    test_test_cases(
      TESTS_SCREENSHOT_PREFIXED_FILEPATH.as_slice(),
      get_date_from_screenshot_prefixed_filepath_regex,
    );
  }
}
