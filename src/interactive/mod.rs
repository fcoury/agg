mod display;
mod file_select;
mod llm_select;
mod prompts;
mod spinner;

use console::{Key, Term};
use ignore::gitignore::Gitignore;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::cli::Args;
use crate::config::{AggConfig, GlobalConfig};
use crate::context::{
    format_line_ranges, select_goal_context, write_selected_context, GoalContextArgs, SelectedFile,
};
use crate::llm::LlmConfig;

use display::{
    print_commands_bar, print_error, print_file_list, print_file_preview, print_header, print_info,
    print_success, SelectionEntry, SelectionSource,
};
use file_select::{fuzzy_add_file, merge_selections, parse_line_ranges};
use llm_select::select_llm;
use prompts::{
    prompt_budget, prompt_confirm, prompt_file_index, prompt_goal, prompt_line_ranges,
    prompt_output_destination, PromptError,
};
use spinner::Spinner;

const DEFAULT_BUDGET: usize = 8000;

/// Main entry point for interactive mode
pub fn run_interactive(
    args: &mut Args,
    _writer: &mut Box<dyn Write>,
    root: &Path,
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> io::Result<bool> {
    // Show header
    print_header();

    // Step 1: Get the goal
    let mut goal = match prompt_goal(args.goal.as_deref()) {
        Ok(g) => g,
        Err(PromptError::Cancelled | PromptError::Interrupted) => {
            print_info("Cancelled");
            return Ok(false);
        }
        Err(e) => {
            print_error(&format!("Error: {}", e));
            return Ok(false);
        }
    };

    // Step 2: Select LLM
    let (llm, llm_cmd) = match select_llm(args.llm.clone(), args.llm_cmd.clone()) {
        Ok(choice) => choice,
        Err(PromptError::Cancelled | PromptError::Interrupted) => {
            print_info("Cancelled");
            return Ok(false);
        }
        Err(e) => {
            print_error(&format!("Error: {}", e));
            return Ok(false);
        }
    };

    // Step 3: Get budget
    let budget = match prompt_budget(args.budget.unwrap_or(DEFAULT_BUDGET)) {
        Ok(b) => b,
        Err(PromptError::Cancelled | PromptError::Interrupted) => {
            print_info("Cancelled");
            return Ok(false);
        }
        Err(e) => {
            print_error(&format!("Error: {}", e));
            return Ok(false);
        }
    };

    // Step 4: Offer to save defaults if changed
    let save_defaults = defaults_changed(args, budget, &llm, &llm_cmd);
    if save_defaults {
        if let Ok(true) = prompt_confirm("Save as defaults?", false) {
            save_local_defaults(args, budget, llm.clone(), llm_cmd.clone())?;
            if let Ok(true) = prompt_confirm("Also save to global config?", false) {
                save_global_defaults(args, budget, llm.clone(), llm_cmd.clone())?;
            }
        }
    }

    // Build LLM config
    let llm_config = LlmConfig {
        provider: llm.clone(),
        command: llm_cmd.clone(),
        model: args.llm_model.clone(),
        debug: args.llm_debug,
        debug_log: args.llm_debug_log.clone(),
    };

    // Step 5: Run initial LLM selection
    let mut entries =
        run_llm_selection(args, root, gitignore, aggignore, &goal, budget, &llm_config)?;
    let mut removed_paths: HashSet<PathBuf> = HashSet::new();

    if entries.is_empty() {
        print_error("No files selected by LLM");
        return Ok(false);
    }

    // Step 6: Interactive command loop
    let term = Term::stdout();

    loop {
        print_file_list(&entries, budget);
        print_commands_bar();

        let key = match term.read_key() {
            Ok(k) => k,
            Err(_) => continue,
        };

        match key {
            Key::Char('a') | Key::Char('A') => {
                // Add file
                let selected_paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
                match fuzzy_add_file(
                    root,
                    &args.allowed_extensions,
                    &args.exclude_dirs,
                    gitignore,
                    aggignore,
                    &selected_paths,
                ) {
                    Ok(Some(path)) => {
                        let full_path = root.join(&path);
                        removed_paths.remove(&path);
                        removed_paths.remove(&full_path);
                        entries.push(SelectionEntry::new(
                            full_path,
                            Vec::new(),
                            SelectionSource::Manual,
                        ));
                        print_success(&format!("Added {}", path.display()));
                    }
                    Ok(None) => {
                        print_info("No file selected");
                    }
                    Err(PromptError::Cancelled) => {}
                    Err(e) => print_error(&format!("Error: {}", e)),
                }
            }

            Key::Char('r') | Key::Char('R') => {
                // Remove file
                match prompt_file_index("Remove which file?", &entries) {
                    Ok(Some(idx)) if idx < entries.len() => {
                        let entry = entries.remove(idx);
                        removed_paths.insert(entry.path.clone());
                        print_success(&format!("Removed {}", entry.path.display()));
                    }
                    Ok(_) => print_info("No file selected"),
                    Err(PromptError::Cancelled) => {}
                    Err(e) => print_error(&format!("Error: {}", e)),
                }
            }

            Key::Char('e') | Key::Char('E') => {
                // Edit line ranges
                match prompt_file_index("Edit which file?", &entries) {
                    Ok(Some(idx)) if idx < entries.len() => {
                        let current = format_line_ranges(&entries[idx].line_ranges)
                            .unwrap_or_else(|| "full".to_string());

                        match prompt_line_ranges(Some(&current)) {
                            Ok(input) => {
                                let ranges = parse_line_ranges(&input);
                                entries[idx].line_ranges = ranges;
                                entries[idx].source = SelectionSource::Manual;
                                entries[idx].recalculate_tokens();
                                print_success("Updated line ranges");
                            }
                            Err(PromptError::Cancelled) => {}
                            Err(e) => print_error(&format!("Error: {}", e)),
                        }
                    }
                    Ok(_) => print_info("No file selected"),
                    Err(PromptError::Cancelled) => {}
                    Err(e) => print_error(&format!("Error: {}", e)),
                }
            }

            Key::Char('p') | Key::Char('P') => {
                // Preview file content
                match prompt_file_index("Preview which file?", &entries) {
                    Ok(Some(idx)) if idx < entries.len() => {
                        print_file_preview(&entries[idx].path, &entries[idx].line_ranges);
                    }
                    Ok(_) => print_info("No file selected"),
                    Err(PromptError::Cancelled) => {}
                    Err(e) => print_error(&format!("Error: {}", e)),
                }
            }

            Key::Char('g') | Key::Char('G') => {
                // Update goal and re-run
                match prompt_goal(Some(&goal)) {
                    Ok(new_goal) => {
                        let keep_manual =
                            prompt_confirm("Keep your manual changes?", true).unwrap_or(true);

                        let new_entries = run_llm_selection(
                            args,
                            root,
                            gitignore,
                            aggignore,
                            &new_goal,
                            budget,
                            &llm_config,
                        )?;

                        entries =
                            merge_selections(&entries, new_entries, &removed_paths, keep_manual);
                        goal = new_goal;

                        if !keep_manual {
                            removed_paths.clear();
                        }
                    }
                    Err(PromptError::Cancelled) => {}
                    Err(e) => print_error(&format!("Error: {}", e)),
                }
            }

            Key::Char('q') | Key::Char('Q') | Key::Escape => {
                print_info("Cancelled");
                return Ok(false);
            }

            Key::Enter => {
                // Accept and output
                break;
            }

            _ => {
                // Ignore other keys
            }
        }
    }

    // Step 7: Prompt for output destination
    let output_path = match prompt_output_destination() {
        Ok(path) => path,
        Err(PromptError::Cancelled | PromptError::Interrupted) => {
            print_info("Cancelled");
            return Ok(false);
        }
        Err(e) => {
            print_error(&format!("Error: {}", e));
            return Ok(false);
        }
    };

    // Step 8: Write output
    let mut output_writer: Box<dyn Write> = match &output_path {
        Some(path) => Box::new(BufWriter::new(File::create(path)?)),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let selection: Vec<SelectedFile> = entries
        .iter()
        .map(|e| SelectedFile {
            path: e.path.clone(),
            line_ranges: e.line_ranges.clone(),
        })
        .collect();

    write_selected_context(
        &mut output_writer,
        &goal,
        budget,
        &llm_config,
        args.include_binary,
        &selection,
    )?;

    let total_tokens: usize = entries.iter().map(|e| e.tokens).sum();
    let dest = output_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "stdout".to_string());

    println!();
    print_success(&format!("Context written to {}", dest));
    print_info(&format!(
        "{} files, ~{} tokens",
        entries.len(),
        total_tokens
    ));

    Ok(true)
}

/// Run LLM selection with spinner
fn run_llm_selection(
    args: &Args,
    root: &Path,
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    goal: &str,
    budget: usize,
    llm_config: &LlmConfig,
) -> io::Result<Vec<SelectionEntry>> {
    let spinner = Spinner::new("Querying LLM for relevant files...");

    let mut sink: Box<dyn Write> = Box::new(io::sink());
    let result = select_goal_context(GoalContextArgs {
        root,
        writer: &mut sink,
        allowed_extensions: &args.allowed_extensions,
        include_binary: args.include_binary,
        exclude_dirs: &args.exclude_dirs,
        gitignore,
        aggignore,
        goal,
        budget: Some(budget),
        llm_config,
    });

    match result {
        Ok(Some(files)) => {
            spinner.finish_success(&format!("Selected {} files", files.len()));
            Ok(files
                .into_iter()
                .map(|f| SelectionEntry::new(f.path, f.line_ranges, SelectionSource::Llm))
                .collect())
        }
        Ok(None) => {
            spinner.finish_error("No files selected");
            Ok(Vec::new())
        }
        Err(e) => {
            spinner.finish_error(&format!("Error: {}", e));
            Err(e)
        }
    }
}

fn defaults_changed(
    args: &Args,
    budget: usize,
    llm: &Option<String>,
    llm_cmd: &Option<String>,
) -> bool {
    args.budget.unwrap_or(DEFAULT_BUDGET) != budget
        || args.llm.as_ref() != llm.as_ref()
        || args.llm_cmd.as_ref() != llm_cmd.as_ref()
}

fn save_local_defaults(
    args: &Args,
    budget: usize,
    llm: Option<String>,
    llm_cmd: Option<String>,
) -> io::Result<()> {
    let mut config = AggConfig::load().unwrap_or_default();
    config.llm = llm;
    config.llm_cmd = llm_cmd;
    config.llm_model = args.llm_model.clone();
    config.budget = Some(budget);
    let path = AggConfig::local_path();
    config.save(&path)?;
    print_success(&format!("Saved to {}", path.display()));
    Ok(())
}

fn save_global_defaults(
    args: &Args,
    budget: usize,
    llm: Option<String>,
    llm_cmd: Option<String>,
) -> io::Result<()> {
    let mut config = match GlobalConfig::load() {
        Ok(existing) => existing,
        Err(e) => {
            print_error(&format!("Warning: {}", e));
            GlobalConfig::default()
        }
    };
    config.llm = llm;
    config.llm_cmd = llm_cmd;
    config.llm_model = args.llm_model.clone();
    config.budget = Some(budget);
    config.save()?;
    if let Some(path) = GlobalConfig::path() {
        print_success(&format!("Saved to {}", path.display()));
    }
    Ok(())
}
