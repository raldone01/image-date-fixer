use super::DateConfidence;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use std::{path::Path, sync::LazyLock};

/// Extracts the date from WhatsApp-style filenames (e.g., IMG-20250127-WA0006.jpg).
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/IMG-20250127-WA0006<POSTFIX>.jpg
pub fn get_date_from_whatsapp_filepath_regex(
  _file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  let re = LazyLock::new(|| Regex::new(r"IMG-(\d{4})(\d{2})(\d{2})-WA\d+").unwrap());
  let captures = re.captures(file_name)?;

  let year: u32 = captures.get(1)?.as_str().parse().ok()?;
  let month: u32 = captures.get(2)?.as_str().parse().ok()?;
  let day: u32 = captures.get(3)?.as_str().parse().ok()?;

  let datetime = NaiveDateTime::new(
    NaiveDate::from_ymd_opt(year.try_into().ok()?, month, day)?,
    NaiveTime::from_hms_opt(0, 0, 0)?,
  );
  Some((datetime, DateConfidence::Day))
}

#[cfg(test)]
pub mod test {
  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_WHATSAPP_FILEPATH: LazyLock<Vec<TestCase>> = LazyLock::new(|| {
    vec![
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/IMG-20250127-WA0006.jpg",
        result: Some((
          NaiveDate::parse_from_str("20250127", "%Y%m%d")
            .unwrap()
            .into(),
          DateConfidence::Day,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/IMG-20250127-WA0006POSTFIX.jpg",
        result: Some((
          NaiveDate::parse_from_str("20250127", "%Y%m%d")
            .unwrap()
            .into(),
          DateConfidence::Day,
        )),
      },
    ]
  });

  #[test]
  fn whatsapp_filepath_regex() {
    test_test_cases(
      TESTS_WHATSAPP_FILEPATH.as_slice(),
      get_date_from_whatsapp_filepath_regex,
    );
  }
}
