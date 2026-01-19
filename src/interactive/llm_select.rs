use inquire::{Select, Text};

use super::prompts::PromptError;

/// Known LLM CLI tools to detect
const LLM_CANDIDATES: &[&str] = &[
    "claude", "openai", "ollama", "llm", "codex", "copilot", "opencode", "gemini", "aichat", "mods",
];

/// Detect which LLM CLI tools are available on the system
pub fn detect_llms() -> Vec<String> {
    LLM_CANDIDATES
        .iter()
        .filter(|name| command_exists(name))
        .map(|name| name.to_string())
        .collect()
}

/// Check if a command exists in PATH
fn command_exists(command: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(value) => value,
        None => return false,
    };
    for dir in std::env::split_paths(&path) {
        let full = dir.join(command);
        if full.exists() {
            return true;
        }
        // Also check with common extensions on Windows
        #[cfg(windows)]
        {
            for ext in &[".exe", ".cmd", ".bat"] {
                let with_ext = dir.join(format!("{}{}", command, ext));
                if with_ext.exists() {
                    return true;
                }
            }
        }
    }
    false
}

/// Prompt user to select an LLM provider
pub fn select_llm(
    current_llm: Option<String>,
    current_cmd: Option<String>,
) -> Result<(Option<String>, Option<String>), PromptError> {
    // If already configured, use existing
    if current_llm.is_some() || current_cmd.is_some() {
        return Ok((current_llm, current_cmd));
    }

    let detected = detect_llms();

    if detected.is_empty() {
        // No LLMs detected, prompt for custom command
        let command = Text::new("No LLM tools detected. Enter LLM command:")
            .with_help_message("e.g., 'ollama run llama2' or 'my-custom-llm'")
            .prompt()?;
        return Ok((None, Some(command.trim().to_string())));
    }

    // Build options list
    let mut options: Vec<String> = detected
        .iter()
        .map(|name| format!("{} (detected)", name))
        .collect();
    options.push("Enter custom command...".to_string());

    let selection = Select::new("Select LLM provider:", options)
        .with_help_message("↑↓ to navigate, Enter to select")
        .prompt()?;

    if selection == "Enter custom command..." {
        let command = Text::new("Enter LLM command:")
            .with_help_message("e.g., 'ollama run llama2' or custom script")
            .prompt()?;
        Ok((None, Some(command.trim().to_string())))
    } else {
        // Extract the provider name (remove " (detected)" suffix)
        let provider = selection
            .split(" (detected)")
            .next()
            .unwrap_or(&selection)
            .to_string();
        Ok((Some(provider), None))
    }
}
