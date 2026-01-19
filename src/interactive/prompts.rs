use inquire::{Confirm, CustomType, Select, Text};
use std::path::PathBuf;

use super::display::SelectionEntry;

/// Error type for prompt operations
#[derive(Debug)]
pub enum PromptError {
    Cancelled,
    Interrupted,
    Other(String),
}

impl From<inquire::InquireError> for PromptError {
    fn from(err: inquire::InquireError) -> Self {
        match err {
            inquire::InquireError::OperationCanceled => PromptError::Cancelled,
            inquire::InquireError::OperationInterrupted => PromptError::Interrupted,
            _ => PromptError::Other(err.to_string()),
        }
    }
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::Cancelled => write!(f, "Operation cancelled"),
            PromptError::Interrupted => write!(f, "Operation interrupted"),
            PromptError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

/// Prompt for the goal description
pub fn prompt_goal(default: Option<&str>) -> Result<String, PromptError> {
    let mut prompt = Text::new("What's your goal?")
        .with_help_message("Describe the context you need (e.g., 'understand the auth flow')");

    if let Some(d) = default {
        prompt = prompt.with_default(d);
    }

    prompt.prompt().map_err(Into::into)
}

/// Prompt for the token budget
pub fn prompt_budget(default: usize) -> Result<usize, PromptError> {
    CustomType::<usize>::new("Token budget (0 = unlimited):")
        .with_default(default)
        .with_help_message("Soft limit for context size")
        .prompt()
        .map_err(Into::into)
}

/// Prompt for confirmation
pub fn prompt_confirm(message: &str, default: bool) -> Result<bool, PromptError> {
    Confirm::new(message)
        .with_default(default)
        .prompt()
        .map_err(Into::into)
}

/// Prompt to select a file from the selection list
pub fn prompt_file_index(
    message: &str,
    entries: &[SelectionEntry],
) -> Result<Option<usize>, PromptError> {
    if entries.is_empty() {
        return Ok(None);
    }

    let options: Vec<String> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{}) {}", i + 1, e.path.display()))
        .collect();

    match Select::new(message, options)
        .with_page_size(10)
        .with_help_message("↑↓ to navigate, Enter to select, Esc to cancel")
        .prompt_skippable()
    {
        Ok(Some(selection)) => {
            // Parse index from "1) path/to/file"
            let idx = selection
                .split(')')
                .next()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .map(|i| i - 1);
            Ok(idx)
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Prompt to add a file using fuzzy search
pub fn prompt_fuzzy_file(candidates: Vec<String>) -> Result<Option<PathBuf>, PromptError> {
    if candidates.is_empty() {
        return Ok(None);
    }

    match Select::new("Add file (type to filter):", candidates)
        .with_page_size(15)
        .with_help_message("Type to search, ↑↓ to navigate, Enter to select, Esc to cancel")
        .prompt_skippable()
    {
        Ok(Some(selection)) => Ok(Some(PathBuf::from(selection))),
        Ok(None) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Prompt for line ranges
pub fn prompt_line_ranges(current: Option<&str>) -> Result<String, PromptError> {
    let mut prompt =
        Text::new("Line ranges:").with_help_message("Enter 'full' or ranges like '1-50,100-150'");

    if let Some(c) = current {
        prompt = prompt.with_default(c);
    }

    prompt.prompt().map_err(Into::into)
}

/// Prompt for output destination
pub fn prompt_output_destination() -> Result<Option<PathBuf>, PromptError> {
    let input = Text::new("Save to file?")
        .with_help_message("Enter path or press Enter for stdout")
        .with_default("")
        .prompt()?;

    if input.trim().is_empty() {
        Ok(None) // stdout
    } else {
        Ok(Some(PathBuf::from(input.trim())))
    }
}
