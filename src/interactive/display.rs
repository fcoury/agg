use console::{style, Emoji, Term};
use std::path::Path;

use crate::context::{estimate_tokens, format_line_ranges, read_selected_content, LineRange};

// Emoji with fallbacks for terminals without emoji support
pub static CHECKMARK: Emoji<'_, '_> = Emoji("✓ ", "+ ");
pub static CROSS: Emoji<'_, '_> = Emoji("✗ ", "x ");
pub static STAR: Emoji<'_, '_> = Emoji("★ ", "* ");

const BOX_WIDTH: usize = 62;

/// Print the main header for interactive mode
pub fn print_header() {
    let term = Term::stdout();
    let _ = term.clear_screen();

    let title = "agg interactive context builder";
    let padding = (BOX_WIDTH - title.len() - 2) / 2;

    println!();
    println!("╭{}╮", "─".repeat(BOX_WIDTH));
    println!(
        "│{}{}{}│",
        " ".repeat(padding),
        style(title).bold().cyan(),
        " ".repeat(BOX_WIDTH - padding - title.len())
    );
    println!("╰{}╯", "─".repeat(BOX_WIDTH));
    println!();
}

/// Selection entry with source tracking
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionSource {
    Llm,
    Manual,
}

#[derive(Clone, Debug)]
pub struct SelectionEntry {
    pub path: std::path::PathBuf,
    pub line_ranges: Vec<LineRange>,
    pub source: SelectionSource,
    pub tokens: usize,
}

impl SelectionEntry {
    pub fn new(
        path: std::path::PathBuf,
        line_ranges: Vec<LineRange>,
        source: SelectionSource,
    ) -> Self {
        let tokens = match read_selected_content(&path, &line_ranges, false) {
            Ok(content) => estimate_tokens(&content),
            Err(_) => 0,
        };
        Self {
            path,
            line_ranges,
            source,
            tokens,
        }
    }

    pub fn recalculate_tokens(&mut self) {
        self.tokens = match read_selected_content(&self.path, &self.line_ranges, false) {
            Ok(content) => estimate_tokens(&content),
            Err(_) => 0,
        };
    }
}

/// Print the file selection list with token counts
pub fn print_file_list(entries: &[SelectionEntry], budget: usize) {
    let total_tokens: usize = entries.iter().map(|e| e.tokens).sum();

    let budget_str = if budget == 0 {
        "unlimited".to_string()
    } else {
        format!("{}", budget)
    };

    // Calculate header without ANSI codes for proper width calculation
    let header_plain = format!("Selected Files ({} / {} tokens)", total_tokens, budget_str);
    let header_display = format!(
        "Selected Files ({} / {} tokens)",
        style(total_tokens).cyan().bold(),
        budget_str
    );

    println!();
    let dashes_needed = BOX_WIDTH.saturating_sub(header_plain.len() + 4);
    println!("╭─ {} {}╮", header_display, "─".repeat(dashes_needed));
    println!("│{}│", " ".repeat(BOX_WIDTH));

    if entries.is_empty() {
        let msg = "No files selected";
        let padding = BOX_WIDTH.saturating_sub(msg.len() + 2);
        println!("│  {}{}│", style(msg).dim(), " ".repeat(padding));
    } else {
        // Column widths (must fit in BOX_WIDTH - 4 for "│  " and "│")
        // Layout: [marker][idx] [path] [range] [tokens]
        // Example: " [ 1] ./src/context.rs            L:1-126   1206 tok"
        let content_width = BOX_WIDTH - 4; // 58 chars available
        let path_width = 28; // file path
        let range_width = 14; // line ranges
        let tok_width = 10; // "1206 tok"

        for (i, entry) in entries.iter().enumerate() {
            let range_str = format_line_ranges(&entry.line_ranges)
                .map(|r| format!("L:{}", r))
                .unwrap_or_else(|| "full".to_string());

            let source_marker = match entry.source {
                SelectionSource::Manual => format!("{}", style(STAR).yellow()),
                SelectionSource::Llm => " ".to_string(),
            };

            // Format path - truncate if too long
            let path_str = entry.path.display().to_string();
            let display_path = if path_str.len() > path_width {
                format!("...{}", &path_str[path_str.len() - path_width + 3..])
            } else {
                path_str
            };

            // Truncate range if too long
            let display_range = if range_str.len() > range_width {
                format!("{}...", &range_str[..range_width - 3])
            } else {
                range_str
            };

            let tok_str = format!("{} tok", entry.tokens);

            // Build the line with fixed widths (no ANSI in width calc)
            // Format: marker[idx] path        range      tokens
            let plain_line = format!(
                " [{:>2}] {:<path_width$} {:>range_width$} {:>tok_width$}",
                i + 1,
                &display_path,
                &display_range,
                &tok_str,
                path_width = path_width,
                range_width = range_width,
                tok_width = tok_width,
            );

            // Now build with colors
            let colored_line = format!(
                " [{:>2}] {:<path_width$} {:>range_width$} {:>tok_width$}",
                style(i + 1).dim(),
                style(&display_path).white(),
                style(&display_range).dim(),
                &tok_str,
                path_width = path_width,
                range_width = range_width,
                tok_width = tok_width,
            );

            let padding = content_width.saturating_sub(plain_line.len() + 1); // +1 for marker
            println!(
                "│ {}{}{}│",
                source_marker,
                colored_line,
                " ".repeat(padding)
            );
        }
    }

    println!("│{}│", " ".repeat(BOX_WIDTH));
    println!("╰{}╯", "─".repeat(BOX_WIDTH));
}

/// Print the command bar with available actions
pub fn print_commands_bar() {
    println!();
    println!(
        "  {} add  {} remove  {} edit  {} goal  {} preview  {} accept  {} quit",
        style("[a]").cyan().bold(),
        style("[r]").cyan().bold(),
        style("[e]").cyan().bold(),
        style("[g]").cyan().bold(),
        style("[p]").cyan().bold(),
        style("[Enter]").green().bold(),
        style("[q]").red().bold(),
    );
    println!();
}

/// Print success message
pub fn print_success(message: &str) {
    println!("{}{}", style(CHECKMARK).green(), message);
}

/// Print error message
pub fn print_error(message: &str) {
    eprintln!("{}{}", style(CROSS).red(), message);
}

/// Print info message
pub fn print_info(message: &str) {
    println!("  {}", style(message).dim());
}

/// Print a file preview with line numbers
pub fn print_file_preview(path: &Path, line_ranges: &[LineRange]) {
    let content = match read_selected_content(path, line_ranges, false) {
        Ok(c) => c,
        Err(e) => {
            print_error(&format!("Cannot read file: {}", e));
            return;
        }
    };

    let range_str = format_line_ranges(line_ranges)
        .map(|r| format!(" (lines {})", r))
        .unwrap_or_default();

    let header = format!("{}{}", path.display(), range_str);

    println!();
    println!(
        "╭─ {} {}╮",
        style(&header).bold(),
        "─".repeat(BOX_WIDTH.saturating_sub(header.len() + 4))
    );

    let lines: Vec<&str> = content.lines().collect();
    let start_line = if line_ranges.is_empty() {
        1
    } else {
        line_ranges.first().map(|r| r.start).unwrap_or(1)
    };

    let max_lines = 25; // Show at most 25 lines
    let display_lines = if lines.len() > max_lines {
        &lines[..max_lines]
    } else {
        &lines
    };

    for (i, line) in display_lines.iter().enumerate() {
        let line_num = start_line + i;
        let truncated_line = if line.len() > BOX_WIDTH - 8 {
            format!("{}...", &line[..BOX_WIDTH - 11])
        } else {
            line.to_string()
        };
        println!(
            "│ {:>4} │ {}{}│",
            style(line_num).dim(),
            truncated_line,
            " ".repeat(BOX_WIDTH.saturating_sub(truncated_line.len() + 9))
        );
    }

    if lines.len() > max_lines {
        println!(
            "│ {:>4} │ {}{}│",
            style("...").dim(),
            style(format!("({} more lines)", lines.len() - max_lines)).dim(),
            " ".repeat(BOX_WIDTH - 30)
        );
    }

    println!("╰{}╯", "─".repeat(BOX_WIDTH));
    println!();
    print_info("Press any key to continue...");

    // Wait for keypress
    let term = Term::stdout();
    let _ = term.read_key();
}
