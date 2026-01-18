use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Global configuration stored in ~/.config/agg.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Default LLM provider or command name
    pub llm: Option<String>,
    /// Default custom command template for LLM
    pub llm_cmd: Option<String>,
    /// Default model name for LLM
    pub llm_model: Option<String>,
    /// Default token budget for goal-driven context
    pub budget: Option<usize>,
}

impl GlobalConfig {
    /// Returns the path to the global config file
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("agg.toml"))
    }

    /// Load global config from ~/.config/agg.toml
    pub fn load() -> Option<Self> {
        let config_path = Self::path()?;
        if !config_path.exists() {
            return Some(GlobalConfig::default());
        }
        match fs::read_to_string(&config_path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => Some(config),
                Err(e) => {
                    eprintln!("Error parsing global config: {}", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Error reading global config: {}", e);
                None
            }
        }
    }

    /// Save global config to ~/.config/agg.toml
    pub fn save(&self) -> io::Result<()> {
        let config_path = Self::path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine config directory",
            )
        })?;

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&config_path, contents)
    }

    /// Get a configuration value by key
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "llm" => self.llm.clone(),
            "llm_cmd" => self.llm_cmd.clone(),
            "llm_model" => self.llm_model.clone(),
            "budget" => self.budget.map(|b| b.to_string()),
            _ => None,
        }
    }

    /// Set a configuration value by key
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "llm" => {
                self.llm = Some(value.to_string());
                Ok(())
            }
            "llm_cmd" => {
                self.llm_cmd = Some(value.to_string());
                Ok(())
            }
            "llm_model" => {
                self.llm_model = Some(value.to_string());
                Ok(())
            }
            "budget" => {
                let budget = value
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid budget value: {}", value))?;
                self.budget = Some(budget);
                Ok(())
            }
            _ => Err(format!("Unknown configuration key: {}", key)),
        }
    }

    /// Unset a configuration value by key
    pub fn unset(&mut self, key: &str) -> Result<(), String> {
        match key {
            "llm" => {
                self.llm = None;
                Ok(())
            }
            "llm_cmd" => {
                self.llm_cmd = None;
                Ok(())
            }
            "llm_model" => {
                self.llm_model = None;
                Ok(())
            }
            "budget" => {
                self.budget = None;
                Ok(())
            }
            _ => Err(format!("Unknown configuration key: {}", key)),
        }
    }

    /// List all set configuration values
    pub fn list(&self) -> Vec<(String, String)> {
        let mut items = Vec::new();
        if let Some(ref llm) = self.llm {
            items.push(("llm".to_string(), llm.clone()));
        }
        if let Some(ref llm_cmd) = self.llm_cmd {
            items.push(("llm_cmd".to_string(), llm_cmd.clone()));
        }
        if let Some(ref llm_model) = self.llm_model {
            items.push(("llm_model".to_string(), llm_model.clone()));
        }
        if let Some(budget) = self.budget {
            items.push(("budget".to_string(), budget.to_string()));
        }
        items
    }
}

/// Project-local configuration stored in .aggconfig
#[derive(Debug, Serialize, Deserialize)]
pub struct AggConfig {
    #[serde(default)]
    pub include_binary: bool,
    pub path: Option<PathBuf>,
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
    pub goal: Option<String>,
    pub budget: Option<usize>,
    pub llm: Option<String>,
    pub llm_cmd: Option<String>,
    pub llm_model: Option<String>,
    #[serde(default)]
    pub llm_debug: bool,
    pub llm_debug_log: Option<PathBuf>,
}

impl AggConfig {
    pub fn load() -> Option<Self> {
        let config_path = PathBuf::from(".aggconfig");
        if config_path.exists() {
            match fs::read_to_string(config_path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => Some(config),
                    Err(e) => {
                        eprintln!("Error parsing .aggconfig: {}", e);
                        None
                    }
                },
                Err(e) => {
                    eprintln!("Error reading .aggconfig: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Apply global config defaults to local config (local takes precedence)
    pub fn with_global_defaults(mut self, global: &GlobalConfig) -> Self {
        if self.llm.is_none() {
            self.llm = global.llm.clone();
        }
        if self.llm_cmd.is_none() {
            self.llm_cmd = global.llm_cmd.clone();
        }
        if self.llm_model.is_none() {
            self.llm_model = global.llm_model.clone();
        }
        if self.budget.is_none() {
            self.budget = global.budget;
        }
        self
    }
}

impl Default for AggConfig {
    fn default() -> Self {
        AggConfig {
            include_binary: false,
            path: None,
            output: None,
            exclude_dirs: Vec::new(),
            allowed_extensions: Vec::new(),
            goal: None,
            budget: None,
            llm: None,
            llm_cmd: None,
            llm_model: None,
            llm_debug: false,
            llm_debug_log: None,
        }
    }
}
