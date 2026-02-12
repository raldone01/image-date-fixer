use std::{path::Path, sync::LazyLock};

use chrono::DateTime;
use regex::Regex;

use super::{ConfidentNaiveDateTime, DateConfidence};

/// Extracts the date from unix timestamp prefixed image file paths.
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/1624280370243_postfix.jpg
pub fn get_date_from_unix_timestamp_prefixed_filepath_regex(
  _file_path: &Path,
  file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{13})").unwrap());
  let captures = RE.captures(file_name)?;

  let timestamp = captures.get(1)?.as_str().parse::<i64>().ok()?;
  let datetime = DateTime::from_timestamp(timestamp / 1000, 0)?;
  Some(ConfidentNaiveDateTime::new(
    datetime.naive_utc(),
    DateConfidence::Second,
  ))
}

#[cfg(test)]
pub mod test {
  use chrono::NaiveDateTime;

  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_UNIX_TIMESTAMP_PREFIXED_FILEPATH: LazyLock<Vec<TestCase>> =
    LazyLock::new(|| {
      vec![
        TestCase {
          file_path: "/home/user/Pictures/1624280370243.jpg",
          expected_result: Some(ConfidentNaiveDateTime::new(
            NaiveDateTime::parse_from_str("20210621125930", "%Y%m%d%H%M%S").unwrap(),
            DateConfidence::Second,
          )),
        },
        TestCase {
          file_path: "/home/user/Pictures/1624280370243_postfix.jpg",
          expected_result: Some(ConfidentNaiveDateTime::new(
            NaiveDateTime::parse_from_str("20210621125930", "%Y%m%d%H%M%S").unwrap(),
            DateConfidence::Second,
          )),
        },
      ]
    });

  #[test]
  fn unix_timestamp_prefixed_filepath_regex() {
    test_test_cases(
      TESTS_UNIX_TIMESTAMP_PREFIXED_FILEPATH.iter(),
      get_date_from_unix_timestamp_prefixed_filepath_regex,
    );
  }
}
