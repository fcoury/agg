use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// A wrapper around indicatif's spinner for consistent styling
pub struct Spinner {
    bar: ProgressBar,
}

impl Spinner {
    /// Create a new spinner with the given message
    pub fn new(message: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        bar.set_message(message.to_string());
        bar.enable_steady_tick(Duration::from_millis(80));
        Self { bar }
    }

    /// Finish the spinner with a success message
    pub fn finish_success(self, message: &str) {
        self.bar.finish_with_message(format!("✓ {}", message));
    }

    /// Finish the spinner with an error message
    pub fn finish_error(self, message: &str) {
        self.bar.finish_with_message(format!("✗ {}", message));
    }

    /// Update the spinner message
    #[allow(dead_code)]
    pub fn set_message(&self, message: &str) {
        self.bar.set_message(message.to_string());
    }
}
