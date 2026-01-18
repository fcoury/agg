use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, FromArgMatches};
use cli::Args;
use config::AggConfig;
use context::GoalContextArgs;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use llm::LlmConfig;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

mod cli;
mod config;
mod context;
mod llm;

fn main() -> io::Result<()> {
    let matches = Args::command().get_matches();
    let cli_args = match Args::from_arg_matches(&matches) {
        Ok(args) => args,
        Err(err) => err.exit(),
    };
    let mut args = if let Some(config) = AggConfig::load() {
        merge_args(config, cli_args, &matches)
    } else {
        cli_args
    };
    if args.llm_debug_log.is_some() {
        args.llm_debug = true;
    }

    if args.debug {
        eprintln!("Debug mode enabled");
        eprintln!("Arguments: {:?}", args);
        return Ok(());
    }

    let mut writer: Box<dyn Write> = match args.output {
        Some(ref path) => Box::new(BufWriter::new(File::create(path).unwrap())),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    let gitignore = load_ignore_file(&root, ".gitignore");
    let aggignore = load_ignore_file(&root, ".aggignore");

    let llm_config = LlmConfig {
        provider: args.llm.clone(),
        command: args.llm_cmd.clone(),
        model: args.llm_model.clone(),
        debug: args.llm_debug,
        debug_log: args.llm_debug_log.clone(),
    };

    if let Some(goal) = args.goal.as_deref() {
        if llm_config.provider.is_none() && llm_config.command.is_none() {
            eprintln!("Goal-driven mode requires --llm or --llm-cmd");
        } else {
            let wrote = context::write_goal_context(GoalContextArgs {
                root: &root,
                writer: &mut writer,
                allowed_extensions: &args.allowed_extensions,
                include_binary: args.include_binary,
                exclude_dirs: &args.exclude_dirs,
                gitignore: &gitignore,
                aggignore: &aggignore,
                goal,
                budget: args.budget,
                llm_config: &llm_config,
            })?;
            if wrote {
                return Ok(());
            }
        }
    }

    visit_dirs(
        &root,
        &mut writer,
        &args.allowed_extensions,
        args.include_binary,
        &args.exclude_dirs,
        &gitignore,
        &aggignore,
    )?;

    Ok(())
}

fn merge_args(config: AggConfig, cli: Args, matches: &ArgMatches) -> Args {
    let from_cli = |name: &str| matches.value_source(name) == Some(ValueSource::CommandLine);
    Args {
        include_binary: if from_cli("include_binary") {
            cli.include_binary
        } else {
            config.include_binary
        },
        path: if from_cli("path") {
            cli.path
        } else {
            config.path
        },
        output: if from_cli("output") {
            cli.output
        } else {
            config.output
        },
        exclude_dirs: if from_cli("exclude_dirs") {
            cli.exclude_dirs
        } else {
            config.exclude_dirs
        },
        debug: cli.debug,
        goal: if from_cli("goal") {
            cli.goal
        } else {
            config.goal
        },
        budget: if from_cli("budget") {
            cli.budget
        } else {
            config.budget
        },
        llm: if from_cli("llm") { cli.llm } else { config.llm },
        llm_cmd: if from_cli("llm_cmd") {
            cli.llm_cmd
        } else {
            config.llm_cmd
        },
        llm_model: if from_cli("llm_model") {
            cli.llm_model
        } else {
            config.llm_model
        },
        llm_debug: if from_cli("llm_debug") {
            cli.llm_debug
        } else {
            config.llm_debug
        },
        llm_debug_log: if from_cli("llm_debug_log") {
            cli.llm_debug_log
        } else {
            config.llm_debug_log
        },
        allowed_extensions: if from_cli("allowed_extensions") {
            cli.allowed_extensions
        } else {
            config.allowed_extensions
        },
    }
}

fn load_ignore_file(root: &Path, filename: &str) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let ignore_path = root.join(filename);
    eprintln!("Reading {} from {:?}", filename, ignore_path);
    if ignore_path.exists() {
        match builder.add(ignore_path) {
            None => (),
            Some(err) => eprintln!("Error adding {}: {}", filename, err),
        }
    }
    builder.build().unwrap_or_else(|err| {
        eprintln!("Error building {}: {}", filename, err);
        Gitignore::empty()
    })
}

fn visit_dirs(
    dir: &PathBuf,
    writer: &mut Box<dyn Write>,
    allowed_extensions: &[String],
    include_binary: bool,
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Check if the directory should be excluded
            let current_dir = path.file_name().unwrap().to_str().unwrap();
            if exclude_dirs.contains(&current_dir.to_string()) {
                continue;
            }

            // Check both gitignore and aggignore patterns
            if gitignore.matched(&path, path.is_dir()).is_ignore()
                || aggignore.matched(&path, path.is_dir()).is_ignore()
            {
                continue; // Skip ignored files/directories
            }

            if path.is_dir() {
                visit_dirs(
                    &path,
                    writer,
                    allowed_extensions,
                    include_binary,
                    exclude_dirs,
                    gitignore,
                    aggignore,
                )?;
            } else if should_process_file(&path, allowed_extensions) {
                match process_file(&path, writer, include_binary) {
                    Ok(_) => (),
                    Err(e) => eprintln!("Error processing file {:?}: {}", path, e),
                }
            }
        }
    }
    Ok(())
}

fn should_process_file(file_path: &Path, allowed_extensions: &[String]) -> bool {
    if allowed_extensions.is_empty() {
        return true; // Process all files if no extensions are specified
    }
    if let Some(extension) = file_path.extension() {
        let ext = extension.to_str().unwrap_or("").to_lowercase();
        allowed_extensions.contains(&ext)
    } else {
        false
    }
}

fn process_file(
    file_path: &PathBuf,
    writer: &mut Box<dyn Write>,
    include_binary: bool,
) -> io::Result<()> {
    let mut file = File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    // Check if the file is UTF-8 encoded
    if let Ok(contents) = String::from_utf8(buffer.clone()) {
        write_file_contents(file_path, writer, &contents)
    } else if include_binary {
        // If not UTF-8 and binary files are allowed, encode as base64
        #[allow(deprecated)]
        let base64 = base64::encode(&buffer);
        write_file_contents(
            file_path,
            writer,
            &format!("[Binary data encoded as base64]:\n{}", base64),
        )
    } else {
        // eprintln!("Skipping non-UTF8 file: {:?}", file_path);
        Ok(())
    }
}

fn write_file_contents(
    file_path: &Path,
    writer: &mut Box<dyn Write>,
    contents: &str,
) -> io::Result<()> {
    let start_marker = format!("<<<START_FILE:{}>>\n", file_path.display());
    let end_marker = format!("<<<END_FILE:{}>>\n", file_path.display());
    writer.write_all(start_marker.as_bytes())?;
    writer.write_all(contents.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.write_all(end_marker.as_bytes())?;
    Ok(())
}
