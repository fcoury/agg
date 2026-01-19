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

    let header = format!(
        "Selected Files ({} / {} tokens)",
        style(total_tokens).cyan().bold(),
        budget_str
    );

    println!();
    println!(
        "╭─ {} {}╮",
        header,
        "─".repeat(BOX_WIDTH.saturating_sub(header.len() + 6))
    );
    println!("│{}│", " ".repeat(BOX_WIDTH));

    if entries.is_empty() {
        println!(
            "│  {}{}│",
            style("No files selected").dim(),
            " ".repeat(BOX_WIDTH - 20)
        );
    } else {
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
            let max_path_len = 32;
            let display_path = if path_str.len() > max_path_len {
                format!("...{}", &path_str[path_str.len() - max_path_len + 3..])
            } else {
                path_str
            };

            let line = format!(
                "{}[{:>2}] {:<35} {:>10} {:>5} tok",
                source_marker,
                style(i + 1).dim(),
                style(&display_path).white(),
                style(&range_str).dim(),
                entry.tokens
            );

            // Pad to box width
            let visible_len = i.to_string().len() + display_path.len() + range_str.len() + 25;
            let padding = BOX_WIDTH.saturating_sub(visible_len);
            println!("│  {}{}│", line, " ".repeat(padding));
        }
    }

    println!("│{}│", " ".repeat(BOX_WIDTH));
    println!("╰{}╯", "─".repeat(BOX_WIDTH));
}

/// Print the command bar with available actions
pub fn print_commands_bar() {
    println!();
    println!(
        "  {}dd  {}emove  {}dit  {}oal  {}review  {}ccept  {}uit",
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
