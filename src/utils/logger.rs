use std::io::Write;

use colored::Colorize;
use terminal_size::{Width, terminal_size};

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

  fn cols() -> usize {
    terminal_size()
      .map(|(Width(w), _)| w as usize)
      .unwrap_or(80)
  }

  fn print_line(&mut self, colored_prefix: String, message: &str) {
    if self.last_progress {
      println!();
    }
    println!("{} {}", colored_prefix, message);
    self.last_progress = false;
  }

  pub fn info(&mut self, message: impl Into<String>) {
    let prefix = format!("[{}]", self.name).blue().to_string();
    self.print_line(prefix, &message.into());
  }

  pub fn warn(&mut self, message: impl Into<String>) {
    let prefix = format!("[{}]", self.name).yellow().to_string();
    self.print_line(prefix, &message.into());
  }

  pub fn error(&mut self, message: impl Into<String>) {
    let prefix = format!("[{}]", self.name).red().to_string();
    self.print_line(prefix, &message.into());
  }

  pub fn success(&mut self, message: impl Into<String>) {
    let prefix = format!("[{}]", self.name).bright_green().to_string();
    self.print_line(prefix, &message.into());
  }

  pub fn progress(&mut self, message: impl Into<String>, progress: f64) {
    let cols = Self::cols();
    let name_plain = format!("[{}]", self.name);
    let prefix_plain = format!("{} {}", name_plain, message.into());

    let mut content: Vec<char> = prefix_plain.chars().collect();
    if content.len() >= cols {
      content.truncate(cols);
    } else {
      content.resize(cols, ' ');
    }

    let p = progress.clamp(0.0, 1.0);
    let filled_length = ((p * cols as f64).round() as usize).min(cols);

    let name_len = name_plain.chars().count();
    let name_filled_overlap = filled_length.min(name_len);
    let name_empty_overlap = name_len.saturating_sub(name_filled_overlap);

    let filled_chars = &content[..filled_length];
    let empty_chars = &content[filled_length..];

    let mut output = String::new();

    if filled_length > 0 {
      if name_filled_overlap > 0 {
        let name_part: String = filled_chars[..name_filled_overlap].iter().collect();
        let rest: String = filled_chars[name_filled_overlap..].iter().collect();
        output.push_str(&name_part.magenta().on_white().to_string());
        if !rest.is_empty() {
          output.push_str(&rest.on_white().to_string());
        }
      } else {
        let filled: String = filled_chars.iter().collect();
        output.push_str(&filled.on_white().to_string());
      }
    }

    if !empty_chars.is_empty() {
      if name_empty_overlap > 0 {
        let name_part: String = empty_chars[..name_empty_overlap].iter().collect();
        let rest: String = empty_chars[name_empty_overlap..].iter().collect();
        output.push_str(&name_part.magenta().to_string());
        output.push_str(&rest);
      } else {
        let empty: String = empty_chars.iter().collect();
        output.push_str(&empty);
      }
    }

    print!("\r{}", output);
    let _ = std::io::stdout().flush();
    self.last_progress = true;
  }

  pub fn had_progress_line(&self) -> bool {
    self.last_progress
  }
}
