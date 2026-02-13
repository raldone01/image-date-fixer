use core::fmt::Display;
use std::path::PathBuf;

use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
#[error("Error processing file {}", file_path.display())]
pub struct ErrorWithFilePath {
  pub file_path: PathBuf,
  #[source]
  pub source: anyhow::Error,
}

impl ErrorWithFilePath {
  #[must_use]
  pub fn new(file_path: impl Into<PathBuf>, source: impl Into<anyhow::Error>) -> Self {
    Self {
      file_path: file_path.into(),
      source: source.into(),
    }
  }

  pub fn from_source<E: Into<anyhow::Error>>(
    file_path: impl Into<PathBuf>,
  ) -> impl FnOnce(E) -> Self {
    let file_path = file_path.into();
    move |source| Self {
      file_path,
      source: source.into(),
    }
  }

  pub fn log_error(&self) {
    error!(
      file_path = %self.file_path.display(),
      "{:#}", self.source
    );
  }

  #[cold]
  #[must_use]
  pub fn context<C>(self, context: C) -> Self
  where
    C: Display + Send + Sync + 'static,
  {
    Self {
      file_path: self.file_path,
      source: self.source.context(context),
    }
  }
}
