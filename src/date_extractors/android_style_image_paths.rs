use super::{ChumError, ConfidentNaiveDateTime, DateConfidence};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chumsky::{
  extra::ParserExtra,
  input::{SliceInput, StrInput},
  label::LabelError,
  text::{Char, TextExpected},
  util::MaybeRef,
};
use core::str::FromStr as _;
use nom::IResult;
use regex::Regex;
use std::{path::Path, sync::LazyLock};

/// Extracts the date from Android-style image file paths.
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/IMG_20190818_130841<POSTFIX>.jpg
pub fn get_date_from_android_filepath_nom(
  _file_path: &Path,
  file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  parse_android_nom(file_name)
    .ok()
    .map(|(_, datetime)| ConfidentNaiveDateTime::new(datetime, DateConfidence::Second))
}

fn parse_android_nom(filename: &str) -> IResult<&str, NaiveDateTime> {
  use nom::{Parser, bytes::complete::tag, character::complete::char, combinator::map_opt};

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
  use nom::{Parser, bytes::complete::take, combinator::map_res};

  map_res(take(N), u32::from_str).parse(num)
}

/// Extracts the date from Android-style image file paths.
/// Example file paths:
///   * /storage/emulated/0/DCIM/Camera/IMG_20190818_130841<POSTFIX>.jpg
#[allow(dead_code)]
pub fn get_date_from_android_filepath_regex(
  _file_path: &Path,
  file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"IMG_(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})").unwrap());
  let captures = RE.captures(file_name)?;

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
  Some(ConfidentNaiveDateTime::new(
    datetime,
    DateConfidence::Second,
  ))
}

#[allow(dead_code)]
#[must_use]
pub fn int_n<'src, I, E>(
  radix: u32,
  length: usize,
) -> impl chumsky::Parser<'src, I, <I as SliceInput<'src>>::Slice, E> + Copy
where
  I: StrInput<'src>,
  I::Token: Char + 'src,
  E: ParserExtra<'src, I>,
  E::Error: LabelError<'src, I, TextExpected<I>> + LabelError<'src, I, MaybeRef<'src, I::Token>>,
{
  use chumsky::prelude::*;

  any()
    .try_map(move |c: I::Token, span| {
      if c.is_digit(radix) {
        Ok(())
      } else {
        Err(LabelError::expected_found(
          [TextExpected::Digit(0, radix)],
          Some(MaybeRef::Val(c)),
          span,
        ))
      }
    })
    .repeated()
    .exactly(length)
    .to_slice()
}

/// /storage/emulated/0/DCIM/Camera/IMG_20190818_130841<POSTFIX>.jpg
#[allow(dead_code)]
pub fn get_date_from_android_filepath_chumsky(
  _file_path: &Path,
  file_name: &str,
) -> Option<ConfidentNaiveDateTime> {
  use chumsky::prelude::*;

  let prefix_parser = just::<_, _, extra::Err<ChumError>>("IMG_").ignored();

  let map_int = |s: &str| s.parse::<u32>().unwrap();

  let date_parser = int_n(10, 4)
    .map(map_int)
    .then(int_n(10, 2).map(map_int))
    .then(int_n(10, 2).map(map_int));

  let time_parser = int_n(10, 2)
    .map(map_int)
    .then(int_n(10, 2).map(map_int))
    .then(int_n(10, 2).map(map_int));
  let date_part_parser = date_parser.then_ignore(just("_")).then(time_parser);
  let android_parser = prefix_parser.then(date_part_parser).lazy();
  let result = android_parser.parse(file_name);

  //crate::date_extractors::print_chumsky_errors(result.errors(), file_name);
  //dbg!(result);

  let ((), (((year, month), day), ((hour, minute), second))) = result.into_result().ok()?;

  let datetime = NaiveDateTime::new(
    NaiveDate::from_ymd_opt(year.try_into().ok()?, month, day)?,
    NaiveTime::from_hms_opt(hour, minute, second)?,
  );
  Some(ConfidentNaiveDateTime::new(
    datetime,
    DateConfidence::Second,
  ))
}

#[cfg(test)]
pub mod test {
  use super::*;
  use crate::date_extractors::test::{TestCase, test_test_cases};
  use std::sync::LazyLock;

  pub static TESTS_ANDROID_FILEPATH: LazyLock<Vec<TestCase>> = LazyLock::new(|| {
    vec![
      TestCase {
        file_path: "/home/user/Pictures/IMG_20190818_130841.jpg",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/home/user/Pictures/IMG_20190818_130841POSTFIX.jpg",
        expected_result: Some(ConfidentNaiveDateTime::new(
          NaiveDateTime::parse_from_str("20190818130841", "%Y%m%d%H%M%S").unwrap(),
          DateConfidence::Second,
        )),
      },
      TestCase {
        file_path: "/home/user/Pictures/IMG_20191318_130841POSTFIX.jpg",
        expected_result: None,
      },
    ]
  });

  #[test]
  fn android_filepath_nom() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.iter(),
      get_date_from_android_filepath_nom,
    );
  }

  #[test]
  fn android_filepath_regex() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.iter(),
      get_date_from_android_filepath_regex,
    );
  }

  #[test]
  fn android_filepath_chumsky() {
    test_test_cases(
      TESTS_ANDROID_FILEPATH.iter(),
      get_date_from_android_filepath_chumsky,
    );
  }
}
