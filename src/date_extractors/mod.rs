//type ChumError = chumsky::error::Simple<char>;
type ChumError = chumsky::error::Cheap;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub enum DateConfidence {
  None,
  Decade,
  Year,
  Month,
  Day,
  Hour,
  Minute,
  Second,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct ConfidentNaiveDateTime {
  pub date: NaiveDateTime,
  pub confidence: DateConfidence,
}
impl ConfidentNaiveDateTime {
  pub fn new(date: NaiveDateTime, confidence: DateConfidence) -> Self {
    Self { date, confidence }
  }
}
impl core::fmt::Display for ConfidentNaiveDateTime {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(
      f,
      "{} (confidence: {:?})",
      self.date.format("%Y-%m-%d %H:%M:%S"),
      self.confidence
    )
  }
}

mod android_style_image_paths;
pub use android_style_image_paths::*;

mod whatsapp_style_image_paths;
pub use whatsapp_style_image_paths::*;

mod uuid_timestamp_prefixed_image_paths;
pub use uuid_timestamp_prefixed_image_paths::*;

mod screenshot_prefixed_style_image_paths;
pub use screenshot_prefixed_style_image_paths::*;

mod custom_date_prefixed_style_image_paths;
pub use custom_date_prefixed_style_image_paths::*;

mod unix_time_stamp_prefixed_style_image_paths;
pub use unix_time_stamp_prefixed_style_image_paths::*;

use chrono::NaiveDateTime;
use std::path::Path;

/// Prints the reports from the vector of errors
pub fn print_chumsky_errors<'a>(errors: impl Iterator<Item = &'a ChumError>, source: &str) {
  use ariadne::{Label, Report, ReportKind, Source};

  errors.for_each(|e| {
    Report::build(ReportKind::Error, e.span().into_range())
      .with_label(Label::new(e.span().into_range()).with_message(e.to_string()))
      .finish()
      .print(Source::from(source))
      .unwrap();
  });
}

pub fn get_date_for_file(
  file_path: &Path,
  file_name: &str,
  current_time: NaiveDateTime,
) -> Option<ConfidentNaiveDateTime> {
  // the uuid handler MUST come first!
  let handler_functions = vec![
    get_date_from_screenshot_prefixed_filepath_regex,
    get_date_from_uuid_prefixed_filepath_regex,
    get_date_from_unix_timestamp_prefixed_filepath_regex,
    get_date_from_android_filepath_nom,
    get_date_from_whatsapp_filepath_regex,
    get_date_from_custom_date_prefixed_filepath_regex,
  ];

  for handler in handler_functions {
    let ret = handler(file_path, file_name);
    if let Some(ret) = ret {
      // check if the date is in the future
      if ret.date > current_time {
        // skip the handler if it returns an invalid date
        continue;
      }
      return Some(ret);
    }
  }
  None
}

#[cfg(test)]
mod test {
  use super::{
    android_style_image_paths::test::TESTS_ANDROID_FILEPATH,
    custom_date_prefixed_style_image_paths::test::TESTS_CUSTOM_DATE_PREFIXED_FILEPATH,
    screenshot_prefixed_style_image_paths::test::TESTS_SCREENSHOT_PREFIXED_FILEPATH,
    unix_time_stamp_prefixed_style_image_paths::test::TESTS_UNIX_TIMESTAMP_PREFIXED_FILEPATH,
    uuid_timestamp_prefixed_image_paths::test::TESTS_UUID_TIMESTAMP_PREFIXED_FILEPATH,
    whatsapp_style_image_paths::test::TESTS_WHATSAPP_FILEPATH, *,
  };
  use std::sync::LazyLock;

  #[derive(PartialEq, Eq, Debug, Clone)]
  pub struct TestCase {
    pub file_path: &'static str,
    pub expected_result: Option<ConfidentNaiveDateTime>,
  }

  pub fn test_test_cases<'a>(
    test_cases: impl Iterator<Item = &'a TestCase>,
    parser: fn(&Path, &str) -> Option<ConfidentNaiveDateTime>,
  ) {
    for test_case in test_cases {
      let file_path = Path::new(test_case.file_path);
      let file_name = file_path.file_name().unwrap().to_str().unwrap();
      let result = parser(file_path, file_name);
      assert_eq!(
        test_case.expected_result, result,
        "`expected == got`: Failed for {}",
        test_case.file_path
      );
    }
  }

  #[test]
  fn confidence_compare() {
    assert!(DateConfidence::Decade < DateConfidence::Year);
    assert!(DateConfidence::Year < DateConfidence::Month);
    assert!(DateConfidence::Month < DateConfidence::Day);
    assert!(DateConfidence::Day < DateConfidence::Hour);
    assert!(DateConfidence::Hour < DateConfidence::Minute);
    assert!(DateConfidence::Minute < DateConfidence::Second);
  }

  fn get_all_test_data() -> impl Iterator<Item = &'static TestCase> {
    static ALL_TEST_CASES: LazyLock<Vec<TestCase>> = LazyLock::new(|| {
      [
        TESTS_ANDROID_FILEPATH.as_slice(),
        TESTS_WHATSAPP_FILEPATH.as_slice(),
        TESTS_UUID_TIMESTAMP_PREFIXED_FILEPATH.as_slice(),
        TESTS_SCREENSHOT_PREFIXED_FILEPATH.as_slice(),
        TESTS_CUSTOM_DATE_PREFIXED_FILEPATH.as_slice(),
        TESTS_UNIX_TIMESTAMP_PREFIXED_FILEPATH.as_slice(),
      ]
      .concat()
    });
    ALL_TEST_CASES.as_slice().iter()
  }

  #[test]
  fn all_test_cases() {
    let test_cases = get_all_test_data();
    test_test_cases(test_cases, |file_path, file_name| {
      get_date_for_file(file_path, file_name, NaiveDateTime::MAX)
    });
  }
}
