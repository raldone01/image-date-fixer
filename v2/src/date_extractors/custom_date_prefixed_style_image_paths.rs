use super::{ChumError, DateConfidence, get_date_for_file};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
use nom::IResult;
use regex::Regex;
use std::{path::Path, str::FromStr, sync::LazyLock};

/// Extracts the date and optional time from filenames prefixed with a YYYY, YYYYMM, YYYYMMDD, YYYY-MM, or YYYY-MM-DD format.
/// Optionally, a time in one of the following formats may follow:
/// * in HHMMSS format (e.g., `-211056`)
/// * 2019-07-14 20_25_57
///
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/2020 10 10 211056.png
///   * /storage/emulated/0/DCIM/Camera/2020_10_10 211056.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056 a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056-a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056+a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056[a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056~a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 211056_a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10-211056 a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10_211056 a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10_20_25_57 a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10 20_25_57 a.png
///   * /storage/emulated/0/DCIM/Camera/20201010_20_25_57 a.png
///   * /storage/emulated/0/DCIM/Camera/20201010_202557 a.png
///   * /storage/emulated/0/DCIM/Camera/20201010_20_25_57-a.png
///   * /storage/emulated/0/DCIM/Camera/20201010-20-25-57 a.png
///   * /storage/emulated/0/DCIM/Camera/2020-10-10.png
///   * /storage/emulated/0/DCIM/Camera/2020-10.png
///   * /storage/emulated/0/DCIM/Camera/2020 a.png (this requires a postfix otherwise it is not specific enough)
/// Unsupported:
///   * /storage/emulated/0/DCIM/Camera/2563.jpg
///   * /storage/emulated/0/DCIM/Camera/2543a.jpg
pub fn get_date_from_custom_date_prefixed_filepath_regex(
  file_path: &Path,
  _file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  let file_name_no_ext = file_path.file_stem()?.to_str()?;

  let re = LazyLock::new(|| {
    Regex::new(r"^(\d{4})([-_\s])?(\d{2})?([-_\s])?(\d{2})?([-_\s])?(\d{2})?([-_\s])?(\d{2})?([-_\s])?(\d{2})?([-_\s\[+.])?").unwrap()
  });
  let captures = re.captures(file_name_no_ext)?;

  let year = captures.get(1)?.as_str().parse::<i32>().ok()?;
  let mut confidence = DateConfidence::Year;
  let month = captures.get(3).map_or(1, |m| {
    confidence = DateConfidence::Month;
    m.as_str().parse::<u32>().ok().unwrap_or(1)
  });
  let day = captures.get(5).map_or(1, |d| {
    confidence = DateConfidence::Day;
    d.as_str().parse::<u32>().ok().unwrap_or(1)
  });
  let hour = captures.get(7).map_or(0, |h| {
    confidence = DateConfidence::Hour;
    h.as_str().parse::<u32>().ok().unwrap_or(0)
  });
  let minute = captures.get(9).map_or(0, |m| {
    confidence = DateConfidence::Minute;
    m.as_str().parse::<u32>().ok().unwrap_or(0)
  });
  let second = captures.get(11).map_or(0, |s| {
    confidence = DateConfidence::Second;
    s.as_str().parse::<u32>().ok().unwrap_or(0)
  });

  let separator_capture_groups = [2_usize, 4, 6, 8, 10, 12];
  let mut consecutive_capture_groups = 0;
  for matches in captures.iter().skip(1) {
    if matches.is_some() {
      consecutive_capture_groups += 1;
    } else {
      break;
    }
  }
  let ends_with_separator_capture_group: bool =
    separator_capture_groups.contains(&consecutive_capture_groups);

  if confidence == DateConfidence::Year && !ends_with_separator_capture_group {
    return None;
  }

  Some((
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
        file_path: "/storage/emulated/0/DCIM/Camera/2563.jpg",
        result: None,
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2563a.jpg",
        result: None,
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020 10 10 211056.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020_10_10 211056.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056-a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056+a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056[a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056~a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 211056_a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10-211056 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10_211056 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 21:10:56", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10_20_25_57 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10 20_25_57 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_20_25_57 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_202557 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010_20_25_57-a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20201010-20-25-57 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 20:25:57", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10-10.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-10 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Day,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020-10.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-10-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Month,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/2020 a.png",
        result: Some((
          NaiveDateTime::parse_from_str("2020-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Year,
        )),
      },
      TestCase {
        file_path: "/storage/emulated/0/DCIM/Camera/20241108_094517_Mull.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("2024-11-08 09:45:17", "%Y-%m-%d %H:%M:%S").unwrap(),
          DateConfidence::Second,
        )),
      },
    ]
  });

  #[test]
  fn custom_date_prefixed_filepath_regex() {
    test_test_cases(
      TESTS_CUSTOM_DATE_PREFIXED_FILEPATH.as_slice(),
      get_date_from_custom_date_prefixed_filepath_regex,
    );
  }
}
