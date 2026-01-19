use crate::cli::Args;
use crate::config::{AggConfig, GlobalConfig};
use crate::context::{
    estimate_tokens, format_line_ranges, read_selected_content, select_goal_context, LineRange,
    SelectedFile,
};
use crate::llm::LlmConfig;
use ignore::gitignore::Gitignore;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_BUDGET: usize = 8000;
const LLM_CANDIDATES: &[&str] = &["claude", "codex", "copilot", "opencode", "gemini"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionSource {
    Llm,
    Manual,
}

#[derive(Clone, Debug)]
struct SelectionEntry {
    file: SelectedFile,
    source: SelectionSource,
}

pub fn run_interactive(
    args: &mut Args,
    writer: &mut Box<dyn Write>,
    root: &Path,
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> io::Result<bool> {
    let mut goal = prompt_goal(args.goal.as_deref())?;
    let (llm, llm_cmd) = select_llm(args.llm.clone(), args.llm_cmd.clone())?;
    let mut budget = prompt_budget(args.budget.unwrap_or(DEFAULT_BUDGET))?;

    let save_defaults = defaults_changed(args, budget, &llm, &llm_cmd);
    if save_defaults && prompt_confirm("Save defaults to .aggconfig?", false)? {
        save_local_defaults(args, budget, llm.clone(), llm_cmd.clone())?;
        if prompt_confirm("Export defaults to global config too?", false)? {
            save_global_defaults(args, budget, llm.clone(), llm_cmd.clone())?;
        }
    }

    let llm_config = LlmConfig {
        provider: llm.clone(),
        command: llm_cmd.clone(),
        model: args.llm_model.clone(),
        debug: args.llm_debug,
        debug_log: args.llm_debug_log.clone(),
    };

    let mut removed_paths = HashSet::new();
    let mut entries = run_selection(args, root, gitignore, aggignore, &goal, budget, &llm_config)?;

    if entries.is_empty() {
        eprintln!("No files selected; exiting interactive mode");
        return Ok(false);
    }

    loop {
        print_summary(&entries, args.include_binary, budget)?;
        print_commands();
        let input = prompt_raw(" > ")?;
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("done") {
            let selection = entries
                .iter()
                .map(|entry| entry.file.clone())
                .collect::<Vec<_>>();
            crate::context::write_selected_context(
                writer,
                &goal,
                budget,
                &llm_config,
                args.include_binary,
                &selection,
            )?;
            return Ok(true);
        }

        let mut parts = trimmed.split_whitespace();
        let command = parts.next().unwrap_or("").to_lowercase();
        match command.as_str() {
            "help" | "h" => show_help(),
            "preview" | "p" => continue,
            "remove" | "r" => {
                if let Some(index) = parse_index(parts.next()) {
                    if index == 0 || index > entries.len() {
                        println!("Invalid index");
                    } else {
                        let entry = entries.remove(index - 1);
                        removed_paths.insert(entry.file.path);
                    }
                } else {
                    println!("Usage: remove <index>");
                }
            }
            "add" | "a" => {
                let path_input = match parts.next() {
                    Some(value) => value.to_string(),
                    None => prompt_raw("Path to add: ")?,
                };
                if let Some(path) = resolve_user_path(root, path_input.trim()) {
                    let ranges_input = parts.collect::<Vec<_>>().join(" ");
                    let ranges = if ranges_input.trim().is_empty() {
                        Vec::new()
                    } else {
                        parse_line_ranges(&ranges_input)
                    };
                    removed_paths.remove(&path);
                    entries.push(SelectionEntry {
                        file: SelectedFile {
                            path,
                            line_ranges: ranges,
                        },
                        source: SelectionSource::Manual,
                    });
                } else {
                    println!("Path not found");
                }
            }
            "range" | "e" => {
                let index = parse_index(parts.next());
                let mut range_input = parts.next().unwrap_or("").to_string();
                if let Some(index) = index {
                    if index == 0 || index > entries.len() {
                        println!("Invalid index");
                        continue;
                    }
                    if range_input.is_empty() {
                        range_input = prompt_raw("Ranges (start-end,... or full): ")?;
                    }
                    let entry = &mut entries[index - 1];
                    let ranges = parse_line_ranges(&range_input);
                    entry.file.line_ranges = ranges;
                    entry.source = SelectionSource::Manual;
                } else {
                    println!("Usage: range <index> <start-end,...>");
                }
            }
            "goal" | "g" => {
                let new_goal = prompt_goal(Some(&goal))?;
                let keep_manual = prompt_confirm("Keep manual changes?", false)?;
                entries = rerun_selection(
                    args,
                    root,
                    gitignore,
                    aggignore,
                    &new_goal,
                    budget,
                    &llm_config,
                    &entries,
                    &mut removed_paths,
                    keep_manual,
                )?;
                goal = new_goal;
            }
            "budget" | "b" => {
                let new_budget = prompt_budget(budget)?;
                let keep_manual = prompt_confirm("Keep manual changes?", false)?;
                entries = rerun_selection(
                    args,
                    root,
                    gitignore,
                    aggignore,
                    &goal,
                    new_budget,
                    &llm_config,
                    &entries,
                    &mut removed_paths,
                    keep_manual,
                )?;
                budget = new_budget;
            }
            "rerun" => {
                let keep_manual = prompt_confirm("Keep manual changes?", false)?;
                entries = rerun_selection(
                    args,
                    root,
                    gitignore,
                    aggignore,
                    &goal,
                    budget,
                    &llm_config,
                    &entries,
                    &mut removed_paths,
                    keep_manual,
                )?;
            }
            _ => println!("Unknown command. Type 'help' for options."),
        }
    }
}

fn prompt_goal(default: Option<&str>) -> io::Result<String> {
    loop {
        let value = prompt_with_default("Goal", default)?;
        if !value.trim().is_empty() {
            return Ok(value);
        }
        println!("Goal cannot be empty.");
    }
}

fn select_llm(
    current_llm: Option<String>,
    current_cmd: Option<String>,
) -> io::Result<(Option<String>, Option<String>)> {
    if current_llm.is_some() || current_cmd.is_some() {
        return Ok((current_llm, current_cmd));
    }

    let detected = detect_llms();
    if detected.is_empty() {
        let command = prompt_raw("Enter LLM command: ")?;
        return Ok((None, Some(command.trim().to_string())));
    }

    println!("Select LLM provider:");
    for (index, name) in detected.iter().enumerate() {
        println!("  {}) {}", index + 1, name);
    }
    println!("  {}) Other", detected.len() + 1);

    loop {
        let choice = prompt_raw("Choice: ")?;
        if let Ok(index) = choice.trim().parse::<usize>() {
            if index >= 1 && index <= detected.len() {
                return Ok((Some(detected[index - 1].clone()), None));
            }
            if index == detected.len() + 1 {
                let command = prompt_raw("Enter LLM command: ")?;
                return Ok((None, Some(command.trim().to_string())));
            }
        }
        println!("Invalid selection.");
    }
}

fn prompt_budget(default: usize) -> io::Result<usize> {
    loop {
        let label = format!("Token budget (0 = unlimited) [{}]", default);
        let input = prompt_raw(&format!("{}: ", label))?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(default);
        }
        if let Ok(value) = trimmed.parse::<usize>() {
            return Ok(value);
        }
        println!("Enter a number.");
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
    println!("Saved defaults to {}", path.display());
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
            eprintln!("Warning: {}", e);
            GlobalConfig::default()
        }
    };
    config.llm = llm;
    config.llm_cmd = llm_cmd;
    config.llm_model = args.llm_model.clone();
    config.budget = Some(budget);
    config.save()?;
    if let Some(path) = GlobalConfig::path() {
        println!("Saved defaults to {}", path.display());
    }
    Ok(())
}

fn run_selection(
    args: &Args,
    root: &Path,
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    goal: &str,
    budget: usize,
    llm_config: &LlmConfig,
) -> io::Result<Vec<SelectionEntry>> {
    let mut sink: Box<dyn Write> = Box::new(io::sink());
    let selection = select_goal_context(crate::context::GoalContextArgs {
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
    })?
    .unwrap_or_default();

    Ok(selection
        .into_iter()
        .map(|file| SelectionEntry {
            file,
            source: SelectionSource::Llm,
        })
        .collect())
}

fn rerun_selection(
    args: &Args,
    root: &Path,
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    goal: &str,
    budget: usize,
    llm_config: &LlmConfig,
    current: &[SelectionEntry],
    removed_paths: &mut HashSet<PathBuf>,
    keep_manual: bool,
) -> io::Result<Vec<SelectionEntry>> {
    let mut fresh = run_selection(args, root, gitignore, aggignore, goal, budget, llm_config)?;
    if !keep_manual {
        removed_paths.clear();
        return Ok(fresh);
    }

    let mut map: HashMap<PathBuf, SelectionEntry> = fresh
        .drain(..)
        .map(|entry| (entry.file.path.clone(), entry))
        .collect();

    for entry in current
        .iter()
        .filter(|entry| entry.source == SelectionSource::Manual)
    {
        let path = entry.file.path.clone();
        map.entry(path.clone())
            .and_modify(|existing| {
                existing.file.line_ranges =
                    merge_ranges(&entry.file.line_ranges, &existing.file.line_ranges);
                existing.source = SelectionSource::Manual;
            })
            .or_insert_with(|| entry.clone());
    }

    for path in removed_paths.iter() {
        map.remove(path);
    }

    Ok(map.into_values().collect())
}

fn merge_ranges(manual: &[LineRange], llm: &[LineRange]) -> Vec<LineRange> {
    if manual.is_empty() {
        return Vec::new();
    }
    let mut merged = manual.to_vec();
    for range in llm {
        if !merged.iter().any(|existing| overlaps(existing, range)) {
            merged.push(*range);
        }
    }
    merged.sort_by_key(|range| range.start);
    merged
}

fn overlaps(a: &LineRange, b: &LineRange) -> bool {
    a.start <= b.end && b.start <= a.end
}

fn print_summary(
    entries: &[SelectionEntry],
    include_binary: bool,
    budget: usize,
) -> io::Result<()> {
    let mut total_tokens = 0usize;
    println!("\nSelection summary (approximate tokens)");
    for (index, entry) in entries.iter().enumerate() {
        let range_label =
            format_line_ranges(&entry.file.line_ranges).unwrap_or_else(|| "full".to_string());
        let tokens = match read_selected_content(
            &entry.file.path,
            &entry.file.line_ranges,
            include_binary,
        ) {
            Ok(content) => estimate_tokens(&content),
            Err(_) => 0,
        };
        total_tokens += tokens;
        println!(
            "  {}) {} ({}; ~{} tokens)",
            index + 1,
            entry.file.path.display(),
            range_label,
            tokens
        );
    }

    if budget == 0 {
        println!("Estimated total: ~{} tokens (unlimited)\n", total_tokens);
    } else {
        println!("Estimated total: ~{} / {} tokens\n", total_tokens, budget);
    }
    Ok(())
}

fn print_commands() {
    println!("Commands: remove <n>, add <path> [start-end,...], range <n> <start-end,...>, goal, budget, rerun, preview, done, help");
}

fn show_help() {
    println!("remove <n>      Remove file by index");
    println!("add <path>      Add file (optional ranges)");
    println!("range <n>       Set ranges for file index");
    println!("goal           Update goal and rerun selection");
    println!("budget         Update budget and rerun selection");
    println!("rerun          Rerun selection with current goal");
    println!("preview        Reprint summary only");
    println!("done           Output context and exit");
}

fn prompt_with_default(prompt: &str, default: Option<&str>) -> io::Result<String> {
    let label = match default {
        Some(value) => format!("{} [{}]: ", prompt, value),
        None => format!("{}: ", prompt),
    };
    let input = prompt_raw(&label)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_confirm(prompt: &str, default: bool) -> io::Result<bool> {
    let label = if default { "Y/n" } else { "y/N" };
    let input = prompt_raw(&format!("{} [{}]: ", prompt, label))?;
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        Ok(default)
    } else {
        Ok(matches!(trimmed.as_str(), "y" | "yes"))
    }
}

fn prompt_raw(prompt: &str) -> io::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn parse_index(value: Option<&str>) -> Option<usize> {
    value?.trim().parse::<usize>().ok()
}

fn parse_line_ranges(input: &str) -> Vec<LineRange> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("full") {
        return Vec::new();
    }
    trimmed
        .split(',')
        .filter_map(|part| {
            let mut iter = part.trim().split('-');
            let start = iter.next()?.trim().parse::<usize>().ok()?;
            let end = iter.next().map(str::trim).unwrap_or("");
            let end = if end.is_empty() {
                start
            } else {
                end.parse::<usize>().ok()?
            };
            let (start, end) = if start > end {
                (end, start)
            } else {
                (start, end)
            };
            Some(LineRange { start, end })
        })
        .collect()
}

fn resolve_user_path(root: &Path, input: &str) -> Option<PathBuf> {
    if input.is_empty() {
        return None;
    }
    let path = Path::new(input);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn detect_llms() -> Vec<String> {
    LLM_CANDIDATES
        .iter()
        .filter(|name| command_exists(name))
        .map(|name| name.to_string())
        .collect()
}

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
    }
    false
}
