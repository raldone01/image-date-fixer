#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(missing_docs)]
use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chumsky::{
  error::Cheap,
  prelude::*,
  text::{Character, digits, ident},
};
use regex::Regex;
use std::path::Path;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber;

//type ChumError = Simple<char>;
type ChumError = Cheap<char>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DateConfidence {
  Decade,
  Year,
  Month,
  Day,
  Hour,
  Minute,
  Second,
}

/// /storage/emulated/0/DCIM/Camera/IMG_20190818_130841POSTFIX.jpg
fn get_date_from_android_filepath_regex(
  file_path: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  let file_name = Path::new(file_path).file_name().and_then(|f| f.to_str())?;

  let re = Regex::new(r"IMG_(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})").unwrap();
  let captures = re.captures(file_name)?;

  let year: u32 = captures.get(1)?.as_str().parse().ok()?;
  let month: u32 = captures.get(2)?.as_str().parse().ok()?;
  let day: u32 = captures.get(3)?.as_str().parse().ok()?;
  let hour: u32 = captures.get(4)?.as_str().parse().ok()?;
  let minute: u32 = captures.get(5)?.as_str().parse().ok()?;
  let second: u32 = captures.get(6)?.as_str().parse().ok()?;

  let datetime = NaiveDateTime::new(
    NaiveDate::from_ymd_opt(year.try_into().ok()?, month, day)?,
    NaiveTime::from_hms_opt(hour, minute, second)?,
  );
  Some((datetime, DateConfidence::Second))
}

#[must_use]
pub fn int_n<C: Character, E: chumsky::Error<C>>(
  radix: u32,
  length: usize,
) -> impl Parser<C, C::Collection, Error = E> + Copy + Clone {
  filter(move |c: &C| c.is_digit(radix))
    .repeated()
    .exactly(length)
    .collect()
}

/// /storage/emulated/0/DCIM/Camera/IMG_20190818_130841POSTFIX.jpg
fn get_date_from_android_filepath_chumsky(
  file_path: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  let file_name = Path::new(file_path).file_name().and_then(|f| f.to_str())?;

  let map_int = |s: String| s.parse::<u32>().unwrap();

  let prefix_parser = just::<_, _, ChumError>("IMG_").ignored();
  let date_parser = int_n(10, 4)
    .map(map_int)
    .chain(int_n(10, 2).map(map_int))
    .chain(int_n(10, 2).map(map_int));
  let time_parser = int_n(10, 2)
    .map(map_int)
    .chain(int_n(10, 2).map(map_int))
    .chain(int_n(10, 2).map(map_int));
  let date_part_parser = date_parser.then_ignore(just("_")).chain(time_parser);
  let android_parser = prefix_parser.then(date_part_parser);
  let result = android_parser.parse(file_name);

  //result.as_ref().inspect_err(|e| print_errors(e, file_name));
  //dbg!(result.as_ref());

  let ((), number_vec) = result.ok()?;
  let year: u32 = number_vec[0];
  let month = number_vec[1];
  let day = number_vec[2];
  let hour = number_vec[3];
  let minute = number_vec[4];
  let second = number_vec[5];
  let datetime = NaiveDateTime::new(
    NaiveDate::from_ymd_opt(year.try_into().ok()?, month, day)?,
    NaiveTime::from_hms_opt(hour, minute, second)?,
  );
  Some((datetime, DateConfidence::Second))
}

/// Prints the reports from the vector of errors
fn print_errors(errors: &Vec<ChumError>, source: &str) {
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

  #[test]
  fn android_filepath_regex() {
    let file_path = "/storage/emulated/0/DCIM/Camera/IMG_20190818_130841.jpg";
    let (datetime, confidence) = get_date_from_android_filepath_regex(file_path).unwrap();
    assert_eq!(
      datetime,
      NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap()
    );
    assert_eq!(confidence, DateConfidence::Second);
  }

  #[test]
  fn android_filepath_chumsky() {
    let file_path = "/storage/emulated/0/DCIM/Camera/IMG_20190818_130841.jpg";
    let (datetime, confidence) = get_date_from_android_filepath_chumsky(file_path).unwrap();
    assert_eq!(
      datetime,
      NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap()
    );
    assert_eq!(confidence, DateConfidence::Second);
  }
}

fn main() {
  // Set up a global subscriber (logs to stdout with default settings)
  tracing_subscriber::fmt::init();

  trace!("This is a trace message");
  debug!("This is a debug message");
  info!("This is an info message");
  warn!("This is a warning");
  error!("This is an error!");
}
