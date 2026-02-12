use core::fmt::{self, Write as _};
use std::io;

use tracing::{Event, Level, Subscriber, field::Visit};
use tracing_subscriber::{
  EnvFilter,
  fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
  registry::LookupSpan,
};

struct CustomFormatter;

impl<S, N> FormatEvent<S, N> for CustomFormatter
where
  S: Subscriber + for<'a> LookupSpan<'a>,
  N: for<'a> FormatFields<'a> + 'static,
{
  fn format_event(
    &self,
    _ctx: &FmtContext<'_, S, N>,
    mut writer: Writer<'_>,
    event: &Event<'_>,
  ) -> fmt::Result {
    // Create a visitor to extract specific fields
    let mut visitor = PathVisitor::default();
    event.record(&mut visitor);

    // Render Timestamp
    let now = chrono::Utc::now();
    write!(writer, "{}  ", now.format("%Y-%m-%dT%H:%M:%S.%fZ"))?;

    // Render Level with Colors
    let level = *event.metadata().level();
    let (color_start, level_str) = match level {
      Level::TRACE => ("\x1b[35m", "TRACE"), // Purple
      Level::DEBUG => ("\x1b[34m", "DEBUG"), // Blue
      Level::INFO => ("\x1b[32m", "INFO "),  // Green (padded for alignment)
      Level::WARN => ("\x1b[33m", "WARN "),  // Yellow
      Level::ERROR => ("\x1b[31m", "ERROR"), // Red
    };
    // Reset color
    write!(writer, "{color_start}{level_str} \x1b[0m")?;

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

pub fn setup_logging(log_level: Option<Level>) {
  let logging_builder = tracing_subscriber::fmt::fmt()
    .with_writer(io::stdout)
    .event_format(CustomFormatter);
  if let Some(level) = log_level {
    logging_builder.with_max_level(level).init();
  } else {
    logging_builder
      .with_env_filter(EnvFilter::from_default_env())
      .init();
  }
}
