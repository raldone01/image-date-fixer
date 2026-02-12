use std::{path::Path, sync::LazyLock};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;

use super::{ConfidentNaiveDateTime, DateConfidence};

const GERMAN_MONTHS_NO_ACCENTS: [&str; 12] = [
  "jaenner",
  "februar",
  "maerz",
  "april",
  "mai",
  "juni",
  "juli",
  "august",
  "september",
  "oktober",
  "november",
  "dezember",
];

const GERMAN_MONTHS_WITH_ACCENTS: [&str; 12] = [
  "jänner",
  "februar",
  "märz",
  "april",
  "mai",
  "juni",
  "juli",
  "august",
  "september",
  "oktober",
  "november",
  "dezember",
];

const GERMAN_PREFIXES_NO_ACCENTS: [&str; 12] = [
  "jan", "feb", "mar", "apr", "mai", "jun", "jul", "aug", "sep", "okt", "nov", "dez",
];

const GERMAN_PREFIXES_WITH_ACCENTS: [&str; 12] = [
  "jan", "feb", "mär", "apr", "mai", "jun", "jul", "aug", "sep", "okt", "nov", "dez",
];

const ENGLISH_MONTHS: [&str; 12] = [
  "january",
  "february",
  "march",
  "april",
  "may",
  "june",
  "july",
  "august",
  "september",
  "october",
  "november",
  "december",
];

const ENGLISH_PREFIXES: [&str; 12] = [
  "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Helper function that converts a string representing a month (either numeric or alphabetic)
/// into a numeric month (1-12). Alphabetic comparisons are done case-insensitively
/// and allow the first 3 letters as an abbreviation.
fn parse_month_from_str(month_str: &str) -> Option<u32> {
  // If the string is numeric, try parsing it directly.
  let numeric_month = month_str.parse::<u32>().ok();
  if numeric_month.is_some() {
    return numeric_month;
  }

  let month_str = month_str.to_lowercase();

  // Check against English month names.
  for (i, &real_month_str) in [
    GERMAN_MONTHS_WITH_ACCENTS,
    GERMAN_MONTHS_NO_ACCENTS,
    GERMAN_PREFIXES_WITH_ACCENTS,
    GERMAN_PREFIXES_NO_ACCENTS,
    ENGLISH_MONTHS,
    ENGLISH_PREFIXES,
  ]
  .iter()
  .flat_map(|list| list.iter().enumerate())
  {
    let is_month_match = month_str == real_month_str;
    if is_month_match {
      return u32::try_from(i + 1).ok();
    }
  }

  None
}

/// Extracts the date and optional time from filenames prefixed with a date. The date can be in
/// numeric format (YYYY, YYYYMM, YYYYMMDD, etc.) or the month may be a string (e.g., "2020-Mar-10").
/// The regex is built to allow alphabetic months (case insensitive, and the first 3 letters are enough).
///
/// Example file paths:
///   * /.../2024-03-23_21.45.17_mull.jpg
///   * /.../2020-Mar-10 21:10:56.png
///   * /.../2020-oct-10.png
///
/// Unsupported:
///   * /.../2563.jpg
///
/// Note: When only a year is provided, a trailing separator (or extra characters) is required to
/// indicate that the date is meant to be specific.
pub fn get_date_from_custom_date_prefixed_filepath_regex(
  file_path: &Path,
  _file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  static RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
      r"^(?P<year>\d{4})?(?P<w1>[-_\s])?(?P<month>(?:\d{2}|[A-Za-z]{3,9}))?(?P<w2>[-_\s])?(?P<day>\d{2})?(?P<w3>[-_\s])?(?P<hour>\d{2})?(?P<w4>[-_\s:.])?(?P<minute>\d{2})?(?P<w5>[-_\s:.])?(?P<second>\d{2})?"
    ).unwrap()
  });

  let file_name_no_ext = file_path.file_stem()?.to_str()?;
  let captures = RE.captures(file_name_no_ext)?;

  // Parse year (required).
  let year_str = captures.name("year")?.as_str();
  let year = year_str.parse::<i32>().ok()?;
  let mut confidence = DateConfidence::Year;

  // Parse month: if provided, it may be numeric or an alphabetic month.
  let maybe_month = captures
    .name("month")
    .and_then(|month_match| parse_month_from_str(month_match.as_str()));
  let month = if let Some(month) = maybe_month {
    confidence = DateConfidence::Month;
    month
  } else {
    1
  };

  // Parse day if available.
  let maybe_day = maybe_month
    .and_then(|_| captures.name("day"))
    .and_then(|day_match| day_match.as_str().parse::<u32>().ok());
  let day = if let Some(day) = maybe_day {
    confidence = DateConfidence::Day;
    day
  } else {
    1
  };

  // Parse hour if available.
  let maybe_hour = maybe_day
    .and_then(|_| captures.name("hour"))
    .and_then(|hour_match| hour_match.as_str().parse::<u32>().ok());
  let hour = if let Some(hour) = maybe_hour {
    confidence = DateConfidence::Hour;
    hour
  } else {
    0
  };

  // Parse minute if available.
  let maybe_minute = maybe_hour
    .and_then(|_| captures.name("minute"))
    .and_then(|minute_match| minute_match.as_str().parse::<u32>().ok());
  let minute = if let Some(minute) = maybe_minute {
    confidence = DateConfidence::Minute;
    minute
  } else {
    0
  };

  // Parse second if available.
  let maybe_second = maybe_minute
    .and_then(|_| captures.name("second"))
    .and_then(|second_match| second_match.as_str().parse::<u32>().ok());
  let second = if let Some(second) = maybe_second {
    confidence = DateConfidence::Second;
    second
  } else {
    0
  };

  let captured_any_whitespace = captures
    .name("w1")
    .or_else(|| captures.name("w2"))
    .or_else(|| captures.name("w3"))
    .or_else(|| captures.name("w4"))
    .or_else(|| captures.name("w5"))
    .is_some();

  // If we only captured a year and the file name consists solely of the year (i.e. "2020"),
  // then the date is not specific enough.
  if confidence == DateConfidence::Year && !captured_any_whitespace {
    return None;
  }

  // Build the NaiveDateTime using the parsed values.
  Some(ConfidentNaiveDateTime::new(
    NaiveDateTime::new(
      NaiveDate::from_ymd_opt(year, month, day)?,
      NaiveTime::from_hms_opt(hour, minute, second)?,
    ),
    confidence,
  ))
}

#[cfg(test)]
pub mod test {
  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_CUSTOM_DATE_PREFIXED_FILEPATH: LazyLock<Vec<TestCase>> = LazyLock::new(|| {
    vec![
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2024-03-23_21.45.17_mull.jpg",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2024-03-23 21:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      // Numeric-only examples
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2563.jpg",
        expected_result: None,
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2563a.jpg",
        expected_result: None,
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020 10 10 21:10:56.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020 10 10 211056.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020_10_10 211056.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056-a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056+a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056[a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056~a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056_a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10-211056 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10_211056 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10_20_25_57 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 20_25_57 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_20_25_57 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_202557 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_20_25_57-a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010-20-25-57 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Day,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Month,
        )),
      },
      // New examples using alphabetic month names.
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-Mar-10 21:10:56.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-03-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-oct-10.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Day,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-OCT-10.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-10-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Day,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2012_Tag06_Bayreuth_Markgräfliches Opernhaus (7).jpg",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2012-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Year,
        )),
      },
      // The test for a year-only filename
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020 a.png",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2020-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Year,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20241108_094517_Mull.jpg",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("2024-11-08 09:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
    ]
  });

  #[test]
  fn custom_date_prefixed_filepath_regex() {
    test_test_cases(
      TESTS_CUSTOM_DATE_PREFIXED_FILEPATH.iter(),
      get_date_from_custom_date_prefixed_filepath_regex,
    );
  }
}
