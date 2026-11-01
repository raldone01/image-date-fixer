use core::fmt::{self, Write as _};
use std::{
  error::Error,
  io::{self, IsTerminal},
};

use tracing::{Event, Level, Subscriber, field::Visit, level_filters::LevelFilter};
use tracing_subscriber::{
  EnvFilter,
  fmt::{
    FmtContext, FormatEvent, FormatFields,
    format::Writer,
    time::{FormatTime, SystemTime},
  },
  registry::LookupSpan,
};

struct CustomFormatter<T> {
  timer: T,
}

impl<T: FormatTime> CustomFormatter<T> {
  fn new(timer: T) -> Self {
    Self { timer }
  }
}

impl<S, N, T> FormatEvent<S, N> for CustomFormatter<T>
where
  S: Subscriber + for<'a> LookupSpan<'a>,
  N: for<'a> FormatFields<'a> + 'static,
  T: FormatTime,
{
  fn format_event(
    &self,
    _ctx: &FmtContext<'_, S, N>,
    mut writer: Writer<'_>,
    event: &Event<'_>,
  ) -> fmt::Result {
    let has_color = writer.has_ansi_escapes();
    // Create a visitor to extract specific fields
    let mut visitor = PathVisitor::default();
    event.record(&mut visitor);

    // Render Timestamp
    self.timer.format_time(&mut writer)?;
    write!(writer, "  ")?;

    // Render Level
    let level = *event.metadata().level();
    let (color, level_str) = match level {
      Level::TRACE => ("\x1b[35m", "TRACE"), // Purple
      Level::DEBUG => ("\x1b[34m", "DEBUG"), // Blue
      Level::INFO => ("\x1b[32m", "INFO "),  // Green (padded for alignment)
      Level::WARN => ("\x1b[33m", "WARN "),  // Yellow (padded for alignment)
      Level::ERROR => ("\x1b[31m", "ERROR"), // Red
    };
    if has_color {
      write!(writer, "{color}{level_str}\x1b[0m ")?;
    } else {
      write!(writer, "{level_str} ")?;
    }

    // Render Target
    write!(writer, "{}: ", event.metadata().target())?;

    // Print the file_path prefix if it exists
    if let Some(path) = visitor.file_path {
      write!(writer, "\"{path}\": ")?;
    }

    // Print the main log message
    if let Some(msg) = visitor.message {
      write!(writer, "{msg}")?;
    }

    // Print any other fields
    if !visitor.others.is_empty() {
      write!(writer, "{}", visitor.others)?;
    }

    writeln!(writer)
  }
}

#[derive(Default)]
struct PathVisitor {
  file_path: Option<String>,
  message: Option<String>,
  others: String,
}

impl Visit for PathVisitor {
  fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
    let name = field.name();
    if name == "file_path" {
      self.file_path = Some(format!("{value:?}"));
    } else if name == "message" {
      self.message = Some(format!("{value:?}"));
    } else {
      // Capture remaining structured fields
      let _ = write!(self.others, " {name}={value:?}");
    }
  }
}

#[derive(Debug)]
pub struct ParseColorModeError;

impl fmt::Display for ParseColorModeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.pad(
      "error parsing color mode: expected one of \"auto\", \"always\", \
      or \"never\"",
    )
  }
}

impl Error for ParseColorModeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorMode {
  #[default]
  Auto,
  Always,
  Never,
}

impl core::str::FromStr for ColorMode {
  type Err = ParseColorModeError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      s if s.eq_ignore_ascii_case("auto") => Ok(Self::Auto),
      s if s.eq_ignore_ascii_case("always") => Ok(Self::Always),
      s if s.eq_ignore_ascii_case("never") => Ok(Self::Never),
      _ => Err(ParseColorModeError),
    }
  }
}

pub fn setup_logging(log_level: Option<Level>, color_mode: ColorMode) {
  // RUST_LOG if set, otherwise INFO
  let mut filter = EnvFilter::builder()
    .with_default_directive(LevelFilter::INFO.into())
    .from_env_lossy();

  // An explicit level replaces the global (target-less) level, whether it came
  // from RUST_LOG or the default. Per-target directives from RUST_LOG still apply.
  if let Some(level) = log_level {
    filter = filter.add_directive(level.into());
  }

  let no_color = std::env::var("NO_COLOR")
    .map(|value| !value.is_empty())
    .unwrap_or(false);

  let with_ansi = match color_mode {
    ColorMode::Always => true,
    ColorMode::Never => false,
    ColorMode::Auto => !no_color && io::stdout().is_terminal(),
  };

  tracing_subscriber::fmt()
    .with_writer(io::stdout)
    .with_ansi(with_ansi)
    .event_format(CustomFormatter::new(SystemTime))
    .with_env_filter(filter)
    .init();
}
