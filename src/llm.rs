use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub provider: Option<String>,
    pub command: Option<String>,
    pub model: Option<String>,
    pub debug: bool,
    pub debug_log: Option<PathBuf>,
}

impl LlmConfig {
    pub fn label(&self) -> String {
        self.command
            .clone()
            .or_else(|| self.provider.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn command_template(&self) -> io::Result<String> {
        self.command
            .clone()
            .or_else(|| self.provider.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LLM command not provided"))
    }
}

pub fn run_llm(prompt: &str, config: &LlmConfig) -> io::Result<String> {
    let template = config.command_template()?;

    // Always pass the prompt via stdin and environment variable to avoid shell injection.
    // The {{prompt}} placeholder is replaced with a safe reference to $AGG_PROMPT.
    let command_str = if template.contains("{{prompt}}") {
        // Replace {{prompt}} with a shell variable reference instead of inline content
        template.replace("{{prompt}}", "\"$AGG_PROMPT\"")
    } else {
        template.clone()
    };

    if config.debug {
        debug_log(config, &format!("LLM command template: {}\n", template))?;
        debug_log(config, &format!("LLM command: {}\n", command_str))?;
        debug_log(config, &format!("LLM prompt bytes: {}\n", prompt.len()))?;
        debug_log(config, &format!("LLM prompt:\n{}\n", prompt))?;
        if let Some(model) = &config.model {
            debug_log(config, &format!("LLM model: {}\n", model))?;
        }
    }

    let mut command = Command::new("sh");
    command.arg("-c").arg(&command_str);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // Always set the prompt as an environment variable (safe from shell injection)
    command.env("AGG_PROMPT", prompt);
    if let Some(model) = &config.model {
        command.env("AGG_LLM_MODEL", model);
    }
    // Also pipe stdin for commands that read from it
    command.stdin(Stdio::piped());

    let mut child = command.spawn()?;
    // Write prompt to stdin for commands that expect it there
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "LLM command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if config.debug {
        debug_log(config, &format!("LLM raw response:\n{}\n", stdout))?;
    }
    Ok(stdout)
}

fn debug_log(config: &LlmConfig, message: &str) -> io::Result<()> {
    if let Some(path) = &config.debug_log {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(message.as_bytes())
    } else {
        eprint!("{}", message);
        Ok(())
    }
}
