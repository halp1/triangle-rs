#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
  Info,
  Warning,
  Error,
  Success,
  Progress,
}

#[derive(Debug, Clone)]
pub struct Logger {
  name: String,
  last_progress: bool,
}

impl Logger {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      last_progress: false,
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  fn prefix(&self) -> String {
    format!("[{}]", self.name)
  }

  fn render_message(messages: &[String]) -> String {
    messages.join(" ")
  }

  pub fn format<T>(&mut self, level: LogLevel, messages: &[T]) -> String
  where
    T: ToString,
  {
    let body = Self::render_message(&messages.iter().map(ToString::to_string).collect::<Vec<_>>());

    match level {
      LogLevel::Progress => {
        self.last_progress = true;
        format!("{} {}", self.prefix(), body)
      }
      _ => {
        self.last_progress = false;
        format!("{} {}", self.prefix(), body)
      }
    }
  }

  pub fn info<T>(&mut self, messages: &[T]) -> String
  where
    T: ToString,
  {
    self.format(LogLevel::Info, messages)
  }

  pub fn warn<T>(&mut self, messages: &[T]) -> String
  where
    T: ToString,
  {
    self.format(LogLevel::Warning, messages)
  }

  pub fn error<T>(&mut self, messages: &[T]) -> String
  where
    T: ToString,
  {
    self.format(LogLevel::Error, messages)
  }

  pub fn success<T>(&mut self, messages: &[T]) -> String
  where
    T: ToString,
  {
    self.format(LogLevel::Success, messages)
  }

  pub fn progress(&mut self, message: impl AsRef<str>, progress: f64, width: usize) -> String {
    let width = width.max(1);
    let progress = progress.clamp(0.0, 1.0);
    let filled = ((width as f64) * progress).round() as usize;
    let base = format!("{} {}", self.prefix(), message.as_ref());
    let mut output = base.chars().take(width).collect::<String>();
    if output.len() < width {
      output.push_str(&" ".repeat(width - output.len()));
    }
    let (a, b) = output.split_at(filled.min(output.len()));
    self.last_progress = true;
    format!("{}{}", a, b)
  }

  pub fn had_progress_line(&self) -> bool {
    self.last_progress
  }
}
