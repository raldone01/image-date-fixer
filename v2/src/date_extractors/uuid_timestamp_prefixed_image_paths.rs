use super::{ChumError, ConfidentNaiveDateTime, DateConfidence};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
use nom::IResult;
use regex::Regex;
use std::{path::Path, str::FromStr, sync::LazyLock};

/// Extracts the date from uuid timestamp prefixed image file paths.
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/1606470461418-49b19a16-01a9-4a11-9789-e3005d827362postfix.jpg
pub fn get_date_from_uuid_prefixed_filepath_regex(
  _file_path: &Path,
  file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+)-([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})").unwrap()
  });
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
  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_UUID_TIMESTAMP_PREFIXED_FILEPATH: LazyLock<Vec<TestCase>> = LazyLock::new(
    || {
      vec![
        TestCase {
          file_path: "/home/user/Pictures/1606470461418-49b19a16-01a9-4a11-9789-e3005d827362.jpg",
          expected_result: Some(ConfidentNaiveDateTime::new(
            NaiveDateTime::parse_from_str("20201127094741", "%Y%m%d%H%M%S").unwrap(),
            DateConfidence::Second,
          )),
        },
        TestCase {
          file_path: "/home/user/Pictures/1606470461418-49b19a16-01a9-4a11-9789-e3005d827362postfix.jpg",
          expected_result: Some(ConfidentNaiveDateTime::new(
            NaiveDateTime::parse_from_str("20201127094741", "%Y%m%d%H%M%S").unwrap(),
            DateConfidence::Second,
          )),
        },
      ]
    },
  );

  #[test]
  fn uuid_prefixed_filepath_regex() {
    test_test_cases(
      TESTS_UUID_TIMESTAMP_PREFIXED_FILEPATH.iter(),
      get_date_from_uuid_prefixed_filepath_regex,
    );
  }
}
