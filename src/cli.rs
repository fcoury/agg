use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::config::AggConfig;

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
            allowed_extensions: config.allowed_extensions,
        }
    }
}
