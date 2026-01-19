use crate::llm::{run_llm, LlmConfig};
use ignore::gitignore::Gitignore;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const DEFAULT_BUDGET: usize = 8000;

#[derive(Clone, Debug)]
struct FileCandidate {
    path: PathBuf,
    size: u64,
    extension: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmResponse {
    #[serde(default)]
    files: Vec<LlmFileSelection>,
}

#[derive(Debug, Deserialize)]
struct LlmFileSelection {
    path: String,
    #[serde(default, alias = "line_ranges", alias = "lines")]
    line_ranges: Vec<LineRange>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct SelectedFile {
    pub path: PathBuf,
    pub line_ranges: Vec<LineRange>,
}

pub struct GoalContextArgs<'a> {
    pub root: &'a Path,
    pub writer: &'a mut Box<dyn Write>,
    pub allowed_extensions: &'a [String],
    pub include_binary: bool,
    pub exclude_dirs: &'a [String],
    pub gitignore: &'a Gitignore,
    pub aggignore: &'a Gitignore,
    pub goal: &'a str,
    pub budget: Option<usize>,
    pub llm_config: &'a LlmConfig,
}

pub fn write_goal_context(args: GoalContextArgs<'_>) -> io::Result<bool> {
    let selection = match select_goal_context(GoalContextArgs {
        root: args.root,
        writer: args.writer,
        allowed_extensions: args.allowed_extensions,
        include_binary: args.include_binary,
        exclude_dirs: args.exclude_dirs,
        gitignore: args.gitignore,
        aggignore: args.aggignore,
        goal: args.goal,
        budget: args.budget,
        llm_config: args.llm_config,
    })? {
        Some(selection) => selection,
        None => return Ok(false),
    };

    write_selected_context(
        args.writer,
        args.goal,
        args.budget.unwrap_or(DEFAULT_BUDGET),
        args.llm_config,
        args.include_binary,
        &selection,
    )?;

    Ok(true)
}

pub fn select_goal_context(args: GoalContextArgs<'_>) -> io::Result<Option<Vec<SelectedFile>>> {
    let candidates = collect_candidates(
        args.root,
        args.allowed_extensions,
        args.exclude_dirs,
        args.gitignore,
        args.aggignore,
    )?;
    let budget = args.budget.unwrap_or(DEFAULT_BUDGET);
    let prompt = build_prompt(args.goal, &candidates, budget);
    let response = match run_llm(&prompt, args.llm_config) {
        Ok(output) => output,
        Err(err) => {
            eprintln!("LLM invocation failed: {}", err);
            return Ok(None);
        }
    };

    let selection = match parse_llm_response(&response) {
        Some(files) if !files.is_empty() => files,
        _ => {
            eprintln!("LLM response was empty or invalid");
            return Ok(None);
        }
    };

    let resolved = selection
        .into_iter()
        .filter_map(|file| {
            resolve_path(args.root, &file.path).map(|path| SelectedFile {
                path,
                line_ranges: file.line_ranges,
            })
        })
        .collect::<Vec<_>>();

    if resolved.is_empty() {
        eprintln!("No valid files selected from LLM response");
        return Ok(None);
    }

    Ok(Some(resolved))
}

pub fn write_selected_context(
    writer: &mut Box<dyn Write>,
    goal: &str,
    budget: usize,
    llm_config: &LlmConfig,
    include_binary: bool,
    selection: &[SelectedFile],
) -> io::Result<()> {
    write_context_output(writer, goal, budget, llm_config, include_binary, selection)
}

fn collect_candidates(
    root: &Path,
    allowed_extensions: &[String],
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> io::Result<Vec<FileCandidate>> {
    let mut candidates = Vec::new();
    collect_candidates_inner(
        root,
        allowed_extensions,
        exclude_dirs,
        gitignore,
        aggignore,
        &mut candidates,
    )?;
    Ok(candidates)
}

fn collect_candidates_inner(
    dir: &Path,
    allowed_extensions: &[String],
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
    candidates: &mut Vec<FileCandidate>,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let current_name = path.file_name().and_then(|name| name.to_str());

            if let Some(dir_name) = current_name {
                if exclude_dirs.contains(&dir_name.to_string()) {
                    continue;
                }
            }

            if gitignore.matched(&path, path.is_dir()).is_ignore()
                || aggignore.matched(&path, path.is_dir()).is_ignore()
            {
                continue;
            }

            if path.is_dir() {
                collect_candidates_inner(
                    &path,
                    allowed_extensions,
                    exclude_dirs,
                    gitignore,
                    aggignore,
                    candidates,
                )?;
            } else if should_process_file(&path, allowed_extensions) {
                let metadata = fs::metadata(&path)?;
                let extension = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.to_lowercase());
                candidates.push(FileCandidate {
                    path,
                    size: metadata.len(),
                    extension,
                });
            }
        }
    }
    Ok(())
}

fn build_prompt(goal: &str, candidates: &[FileCandidate], budget: usize) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are selecting project context for an LLM.\n");
    prompt.push_str("Return JSON only (no markdown). Schema:\n");
    prompt.push_str(
        "{\"files\":[{\"path\":\"src/main.rs\",\"line_ranges\":[{\"start\":10,\"end\":50}]}]}\n",
    );
    prompt.push_str("Use empty line_ranges [] to include an entire file.\n");
    prompt.push_str("Use relative paths. Only select from available files below.\n");
    let budget_label = if budget == 0 {
        "0 (unlimited)".to_string()
    } else {
        budget.to_string()
    };
    prompt.push_str(&format!(
        "Goal: {}\nToken budget (soft): {}\n",
        goal, budget_label
    ));
    prompt.push_str("Available files:\n");
    for candidate in candidates {
        let path = candidate.path.display();
        let ext = candidate.extension.as_deref().unwrap_or("none");
        prompt.push_str(&format!(
            "- {} | {} bytes | ext:{}\n",
            path, candidate.size, ext
        ));
    }
    prompt
}

fn parse_llm_response(response: &str) -> Option<Vec<LlmFileSelection>> {
    let json = extract_json(response)?;
    let parsed: LlmResponse = serde_json::from_str(&json).ok()?;
    Some(parsed.files)
}

fn extract_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start >= end {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn write_context_output(
    writer: &mut Box<dyn Write>,
    goal: &str,
    budget: usize,
    llm_config: &LlmConfig,
    include_binary: bool,
    selection: &[SelectedFile],
) -> io::Result<()> {
    let mut tokens_used = 0usize;
    let header = format!(
        "<<<START_CONTEXT:goal=\"{}\" budget=\"{}\" llm=\"{}\">>>\n",
        sanitize_attr(goal),
        budget,
        sanitize_attr(&llm_config.label())
    );
    writer.write_all(header.as_bytes())?;
    tokens_used += estimate_tokens(&header);

    let unlimited = budget == 0;
    for file in selection {
        let path = &file.path;
        let Ok((content, lines_attr)) =
            extract_file_content(path, &file.line_ranges, include_binary)
        else {
            continue;
        };
        let display_path = path.display();
        let start_marker = match lines_attr {
            Some(ref lines) => format!("<<<START_FILE:{} lines=\"{}\">>>\n", display_path, lines),
            None => format!("<<<START_FILE:{}>>>\n", display_path),
        };
        let end_marker = format!("<<<END_FILE:{}>>>\n", display_path);

        if !unlimited && tokens_used >= budget {
            break;
        }

        let block = format!("{}{}\n{}", start_marker, content, end_marker);
        let block_tokens = estimate_tokens(&block);
        if unlimited || tokens_used + block_tokens <= budget {
            writer.write_all(block.as_bytes())?;
            tokens_used += block_tokens;
            continue;
        }

        let remaining_tokens = budget.saturating_sub(tokens_used);
        if remaining_tokens == 0 {
            break;
        }

        if let Some(truncated) =
            truncate_block(&start_marker, &content, &end_marker, remaining_tokens)
        {
            writer.write_all(truncated.as_bytes())?;
        }
        break;
    }

    let footer = "<<<END_CONTEXT>>>\n";
    writer.write_all(footer.as_bytes())?;
    Ok(())
}

fn extract_file_content(
    path: &Path,
    line_ranges: &[LineRange],
    include_binary: bool,
) -> io::Result<(String, Option<String>)> {
    let content = read_selected_content(path, line_ranges, include_binary)?;
    let lines_attr = format_line_ranges(line_ranges);
    Ok((content, lines_attr))
}

pub fn read_selected_content(
    path: &Path,
    line_ranges: &[LineRange],
    include_binary: bool,
) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    if let Ok(contents) = String::from_utf8(buffer.clone()) {
        if line_ranges.is_empty() {
            return Ok(contents);
        }
        let mut extracted = Vec::new();
        let lines: Vec<&str> = contents.lines().collect();
        for range in line_ranges {
            if range.start == 0 || range.end == 0 || range.start > range.end {
                continue;
            }
            let start = range.start.saturating_sub(1);
            let end = range.end.min(lines.len());
            if start >= end {
                continue;
            }
            extracted.push(lines[start..end].join("\n"));
        }
        Ok(extracted.join("\n\n"))
    } else if include_binary {
        #[allow(deprecated)]
        let base64 = base64::encode(&buffer);
        Ok(format!("[Binary data encoded as base64]:\n{}", base64))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Non-UTF8 file"))
    }
}

pub fn format_line_ranges(ranges: &[LineRange]) -> Option<String> {
    if ranges.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for range in ranges {
        if range.start == 0 || range.end == 0 || range.start > range.end {
            continue;
        }
        parts.push(format!("{}-{}", range.start, range.end));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

fn truncate_block(
    start_marker: &str,
    content: &str,
    end_marker: &str,
    remaining_tokens: usize,
) -> Option<String> {
    let marker_tokens = estimate_tokens(start_marker) + estimate_tokens(end_marker);
    if remaining_tokens <= marker_tokens {
        return None;
    }
    let truncation_notice = "\n[TRUNCATED]\n";
    let notice_tokens = estimate_tokens(truncation_notice);
    let available_tokens = remaining_tokens.saturating_sub(marker_tokens + notice_tokens);
    if available_tokens == 0 {
        return None;
    }
    let truncated_content = truncate_content(content, available_tokens);
    Some(format!(
        "{}{}{}{}",
        start_marker, truncated_content, truncation_notice, end_marker
    ))
}

fn truncate_content(content: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens.saturating_mul(4);
    if content.len() <= max_chars {
        return content.to_string();
    }
    // Find a safe byte index at a char boundary to avoid slicing inside a UTF-8 codepoint
    let end_byte = if content.is_char_boundary(max_chars) {
        max_chars
    } else {
        // Scan backwards to find the largest valid char boundary <= max_chars
        content
            .char_indices()
            .rev()
            .find(|(idx, _)| *idx <= max_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    };
    let mut truncated = content[..end_byte].to_string();
    if let Some(last_newline) = truncated.rfind('\n') {
        truncated.truncate(last_newline);
    }
    truncated
}

fn sanitize_attr(value: &str) -> String {
    value.replace('"', "'").replace('\n', " ")
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.len().saturating_add(3)) / 4
}

fn resolve_path(root: &Path, path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        if candidate.starts_with(root) {
            return Some(candidate.to_path_buf());
        }
        return None;
    }
    let joined = root.join(candidate);
    if joined.exists() {
        Some(joined)
    } else {
        None
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
