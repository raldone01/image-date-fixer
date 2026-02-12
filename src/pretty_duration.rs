use core::{fmt::Write as _, time::Duration};

#[must_use]
pub fn pretty_duration(duration: Duration) -> String {
  const SECONDS_PER_MINUTE: u64 = 60;
  const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
  const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

  let mut result = String::with_capacity(32);
  let secs = duration.as_secs();

  if secs >= SECONDS_PER_DAY {
    let days = secs / SECONDS_PER_DAY;
    let hours = (secs % SECONDS_PER_DAY) / SECONDS_PER_HOUR;
    let minutes = (secs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;

    write!(result, "{days}d").unwrap();
    if hours > 0 {
      write!(result, " {hours}h").unwrap();
    }
    if minutes > 0 {
      write!(result, " {minutes}m").unwrap();
    }
  } else if secs >= SECONDS_PER_HOUR {
    let hours = secs / SECONDS_PER_HOUR;
    let minutes = (secs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let seconds = secs % SECONDS_PER_MINUTE;

    write!(result, "{hours}h").unwrap();
    if minutes > 0 {
      write!(result, " {minutes}m").unwrap();
    }
    if seconds > 0 {
      write!(result, " {seconds}s").unwrap();
    }
  } else if secs >= SECONDS_PER_MINUTE {
    let minutes = secs / SECONDS_PER_MINUTE;
    let seconds = secs % SECONDS_PER_MINUTE;

    write!(result, "{minutes}m").unwrap();
    if seconds > 0 {
      write!(result, " {seconds}s").unwrap();
    }
  } else if secs >= 1 {
    let millis = duration.subsec_millis();
    write!(result, "{secs}s").unwrap();
    if millis > 0 {
      write!(result, " {millis}ms").unwrap();
    }
  } else {
    // Sub-second handling
    let nanos = duration.subsec_nanos();

    if nanos == 0 {
      return "0ns".to_string();
    } else if nanos >= 1_000_000 {
      let millis = nanos / 1_000_000;
      let micros = (nanos % 1_000_000) / 1_000;
      write!(result, "{millis}ms").unwrap();
      if micros > 0 {
        write!(result, " {micros}µs").unwrap();
      }
    } else if nanos >= 1_000 {
      let micros = nanos / 1_000;
      let ns = nanos % 1_000;
      write!(result, "{micros}µs").unwrap();
      if ns > 0 {
        write!(result, " {ns}ns").unwrap();
      }
    } else {
      write!(result, "{nanos}ns").unwrap();
    }
  }

  result
}
