use ignore::gitignore::Gitignore;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::display::{SelectionEntry, SelectionSource};
use super::prompts::{prompt_fuzzy_file, PromptError};
use crate::context::LineRange;

/// Collect all candidate files respecting ignore patterns
pub fn collect_all_files(
    root: &Path,
    allowed_extensions: &[String],
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_inner(
        root,
        root,
        allowed_extensions,
        exclude_dirs,
        gitignore,
        aggignore,
        &mut files,
    );
    files.sort();
    files
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    allowed_extensions: &[String],
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    files: &mut Vec<PathBuf>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip excluded directories
        if path.is_dir() && exclude_dirs.contains(&name.to_string()) {
            continue;
        }

        // Skip ignored paths
        if gitignore.matched(&path, path.is_dir()).is_ignore()
            || aggignore.matched(&path, path.is_dir()).is_ignore()
        {
            continue;
        }

        if path.is_dir() {
            collect_files_inner(
                root,
                &path,
                allowed_extensions,
                exclude_dirs,
                gitignore,
                aggignore,
                files,
            );
        } else if should_process_file(&path, allowed_extensions) {
            // Store relative path
            if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.to_path_buf());
            } else {
                files.push(path);
            }
        }
    }
}

fn should_process_file(file_path: &Path, allowed_extensions: &[String]) -> bool {
    if allowed_extensions.is_empty() {
        return true;
    }
    if let Some(extension) = file_path.extension() {
        let ext = extension.to_str().unwrap_or("").to_lowercase();
        allowed_extensions.contains(&ext)
    } else {
        false
    }
}

/// Prompt to add a file using fuzzy search
pub fn fuzzy_add_file(
    root: &Path,
    allowed_extensions: &[String],
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    already_selected: &[PathBuf],
) -> Result<Option<PathBuf>, PromptError> {
    let all_files = collect_all_files(root, allowed_extensions, exclude_dirs, gitignore, aggignore);

    // Filter out already selected files
    let selected_set: HashSet<_> = already_selected.iter().collect();
    let candidates: Vec<String> = all_files
        .into_iter()
        .filter(|p| !selected_set.contains(p))
        .map(|p| p.display().to_string())
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    prompt_fuzzy_file(candidates)
}

/// Parse line ranges from user input
pub fn parse_line_ranges(input: &str) -> Vec<LineRange> {
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

/// Merge manual selections with LLM selections
pub fn merge_selections(
    manual_entries: &[SelectionEntry],
    llm_entries: Vec<SelectionEntry>,
    removed_paths: &HashSet<PathBuf>,
    keep_manual: bool,
) -> Vec<SelectionEntry> {
    if !keep_manual {
        return llm_entries
            .into_iter()
            .filter(|e| !removed_paths.contains(&e.path))
            .collect();
    }

    use std::collections::HashMap;

    let mut map: HashMap<PathBuf, SelectionEntry> = llm_entries
        .into_iter()
        .map(|e| (e.path.clone(), e))
        .collect();

    // Apply manual entries (they take precedence)
    for entry in manual_entries
        .iter()
        .filter(|e| e.source == SelectionSource::Manual)
    {
        map.entry(entry.path.clone())
            .and_modify(|existing| {
                existing.line_ranges = merge_ranges(&entry.line_ranges, &existing.line_ranges);
                existing.source = SelectionSource::Manual;
                existing.recalculate_tokens();
            })
            .or_insert_with(|| entry.clone());
    }

    // Remove explicitly removed paths
    for path in removed_paths {
        map.remove(path);
    }

    map.into_values().collect()
}

fn merge_ranges(manual: &[LineRange], llm: &[LineRange]) -> Vec<LineRange> {
    if manual.is_empty() {
        return Vec::new(); // Manual selected "full" file
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
