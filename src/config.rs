use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
}
