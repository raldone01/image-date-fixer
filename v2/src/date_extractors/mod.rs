//type ChumError = chumsky::error::Simple<char>;
type ChumError = chumsky::error::Cheap<char>;

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

mod android_style_image_paths;
use std::path::Path;

pub use android_style_image_paths::*;
use chrono::NaiveDateTime;

/// Prints the reports from the vector of errors
pub fn print_chumsky_errors(errors: &[ChumError], source: &str) {
  use ariadne::{Label, Report, ReportKind, Source};

  errors
    .iter()
    .map(|e| {
      Report::build(ReportKind::Error, e.span())
        .with_label(Label::new(e.span()).with_message("OOF"))
        .finish()
        .print(Source::from(source))
        .unwrap();
    })
    .for_each(drop);
}

pub fn get_date_for_file(
  file_path: &Path,
  file_name: &str,
  current_time: NaiveDateTime,
) -> Option<(NaiveDateTime, DateConfidence)> {
  // the uuid handler MUST come first!
  let handler_functions = vec![get_date_from_android_filepath_nom];

  for handler in handler_functions {
    let ret = handler(file_path, file_name);
    if let Some((date, confidence)) = ret {
      // check if the date is in the future
      if date > current_time {
        // skip the handler if it returns an invalid date
        continue;
      }
      return Some((date, confidence));
    }
  }
  None
}

#[cfg(test)]
mod test {
  use super::{android_style_image_paths::test::TESTS_ANDROID_FILEPATH, *};
  use rand::seq::SliceRandom;
  use std::sync::LazyLock;

  #[derive(Debug, Clone, Copy)]
  pub struct TestCase {
    pub file_path: &'static str,
    pub result: Option<(NaiveDateTime, DateConfidence)>,
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

  fn get_all_test_data() -> Vec<TestCase> {
    static ALL_TEST_CASES: LazyLock<Vec<TestCase>> =
      LazyLock::new(|| vec![TESTS_ANDROID_FILEPATH.as_slice()].concat());
    // shuffle the test cases
    let mut rng = rand::rng();
    let mut test_cases = ALL_TEST_CASES.clone();
    test_cases.shuffle(&mut rng);
    test_cases
  }

  #[test]
  fn all_test_cases() {
    let test_cases = get_all_test_data();
    for test_case in test_cases {
      let file_path = Path::new(test_case.file_path);
      let file_name = file_path.file_name().unwrap().to_str().unwrap();
      let result = get_date_for_file(file_path, file_name, NaiveDateTime::MAX);
      assert_eq!(result, test_case.result);
    }
  }
}
