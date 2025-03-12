use super::{ChumError, DateConfidence};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use nom::IResult;
use regex::Regex;
use std::{path::Path, str::FromStr};

pub fn get_date_from_android_filepath_nom(
  _file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  parse_android_nom(file_name)
    .ok()
    .map(|(_, datetime)| (datetime, DateConfidence::Second))
}

fn parse_android_nom(filename: &str) -> IResult<&str, NaiveDateTime> {
  use nom::{
    IResult, Parser,
    bytes::complete::{tag, take},
    character::complete::char,
    combinator::{map_opt, map_res},
    error::Error,
    multi::many0,
  };

  let (input, _) = tag("IMG_")(filename)?;
  map_opt(
    (
      parse_num::<4>,
      parse_num::<2>,
      parse_num::<2>,
      char('_'),
      parse_num::<2>,
      parse_num::<2>,
      parse_num::<2>,
    ),
    |(year, month, day, _, hour, minute, second)| {
      Some(NaiveDateTime::new(
        NaiveDate::from_ymd_opt(year.try_into().ok()?, month, day)?,
        NaiveTime::from_hms_opt(hour, minute, second)?,
      ))
    },
  )
  .parse(input)
}

fn parse_num<const N: usize>(num: &str) -> IResult<&str, u32> {
  use nom::{
    IResult, Parser,
    bytes::complete::{tag, take},
    character::complete::char,
    combinator::{map_opt, map_res},
    error::Error,
    multi::many0,
  };

  map_res(take(N), u32::from_str).parse(num)
}

/// /storage/emulated/0/DCIM/Camera/IMG_20190818_130841POSTFIX.jpg
pub fn get_date_from_android_filepath_regex(
  _file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
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
pub fn int_n<C: chumsky::text::Character, E: chumsky::Error<C>>(
  radix: u32,
  length: usize,
) -> impl chumsky::Parser<C, C::Collection, Error = E> + Copy + Clone {
  use chumsky::prelude::*;

  filter(move |c: &C| c.is_digit(radix))
    .repeated()
    .exactly(length)
    .collect()
}

/// /storage/emulated/0/DCIM/Camera/IMG_20190818_130841POSTFIX.jpg
pub fn get_date_from_android_filepath_chumsky(
  _file_path: &Path,
  file_name: &str,
) -> Option<(NaiveDateTime, DateConfidence)> {
  use chumsky::prelude::*;

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

#[cfg(test)]
mod test {
  use super::*;
  use lazy_static::lazy_static;

  struct TestCase {
    file_path: &'static str,
    result: Option<(NaiveDateTime, DateConfidence)>,
  }

  lazy_static! {
    static ref TESTS_ANDROID_FILEPATH: [TestCase; 3] = [
      TestCase {
        file_path: "/home/user/Pictures/IMG_20190818_130841.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/home/user/Pictures/IMG_20190818_130841POSTFIX.jpg",
        result: Some((
          NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/home/user/Pictures/IMG_20191318_130841POSTFIX.jpg",
        result: None,
      },
    ];
  }

  fn test_test_cases(
    test_cases: &[TestCase],
    parser: fn(&Path, &str) -> Option<(NaiveDateTime, DateConfidence)>,
  ) {
    for test_case in test_cases {
      let file_path = Path::new(test_case.file_path);
      let file_name = file_path.file_name().unwrap().to_str().unwrap();
      let result = parser(file_path, file_name);
      assert_eq!(result, test_case.result);
    }
  }

  #[test]
  fn android_filepath_nom() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.as_slice(),
      get_date_from_android_filepath_nom,
    );
  }

  #[test]
  fn android_filepath_regex() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.as_slice(),
      get_date_from_android_filepath_regex,
    );
  }

  #[test]
  fn android_filepath_chumsky() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.as_slice(),
      get_date_from_android_filepath_chumsky,
    );
  }
}
