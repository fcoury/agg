use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::config::AggConfig;

#[derive(Parser, Debug)]
#[command(name = "agg")]
#[command(about = "Aggregate files for LLM context")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub args: Args,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage global configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Set a configuration value
    Set {
        /// Configuration key (llm, llm_cmd, llm_model, budget)
        key: String,
        /// Configuration value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key (llm, llm_cmd, llm_model, budget)
        key: String,
    },
    /// List all configuration values
    List,
    /// Remove a configuration value
    Unset {
        /// Configuration key to remove
        key: String,
    },
    /// Show the path to the global config file
    Path,
}

#[derive(Parser, Debug, Serialize, Deserialize)]
pub struct Args {
    /// Includes binary files as base64 encoded strings in the output
    #[arg(short = 'b', long)]
    pub include_binary: bool,

    /// Initial path to start searching for files, defaults to the current directory
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Output file to write the generated prompt contents to, defaults to stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// List of subdirectories to exclude from the output
    #[arg(short, long)]
    pub exclude_dirs: Vec<String>,

    /// Debug mode, prints the arguments and exits
    #[arg(short, long)]
    pub debug: bool,

    /// Run in interactive context builder mode
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Goal describing the context to extract
    #[arg(long)]
    pub goal: Option<String>,

    /// Soft token budget for goal-driven context
    #[arg(long)]
    pub budget: Option<usize>,

    /// LLM provider or command name to invoke
    #[arg(long)]
    pub llm: Option<String>,

    /// Custom command template to invoke the LLM
    #[arg(long)]
    pub llm_cmd: Option<String>,

    /// Optional model name passed to the LLM command
    #[arg(long)]
    pub llm_model: Option<String>,

    /// Print debug output for goal-driven LLM selection
    #[arg(long)]
    pub llm_debug: bool,

    /// Write LLM debug output to a log file
    #[arg(long)]
    pub llm_debug_log: Option<PathBuf>,

    /// List of file extensions to include in the output
    #[clap(last = true)]
    pub allowed_extensions: Vec<String>,
}

impl From<AggConfig> for Args {
    fn from(config: AggConfig) -> Self {
        Args {
            include_binary: config.include_binary,
            path: config.path,
            output: config.output,
            exclude_dirs: config.exclude_dirs,
            goal: config.goal,
            budget: config.budget,
            llm: config.llm,
            llm_cmd: config.llm_cmd,
            llm_model: config.llm_model,
            llm_debug: config.llm_debug,
            llm_debug_log: config.llm_debug_log,
            interactive: config.interactive,
            allowed_extensions: config.allowed_extensions,
            debug: false,
        }
    }
}
