//type ChumError = chumsky::error::Simple<char>;
type ChumError = chumsky::error::Cheap<char>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
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
pub use android_style_image_paths::*;

/// Prints the reports from the vector of errors
fn print_errors(errors: &Vec<ChumError>, source: &str) {
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

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn confidence_compare() {
    assert!(DateConfidence::Decade < DateConfidence::Year);
    assert!(DateConfidence::Year < DateConfidence::Month);
    assert!(DateConfidence::Month < DateConfidence::Day);
    assert!(DateConfidence::Day < DateConfidence::Hour);
    assert!(DateConfidence::Hour < DateConfidence::Minute);
    assert!(DateConfidence::Minute < DateConfidence::Second);
  }
}
