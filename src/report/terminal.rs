use colored::*;

use crate::audit::design_system::DesignSystemPair;
use crate::contrast::levels::{ConformanceLevel, MinimumLevel};
use crate::scanner::component::ColorPair;

/// Print design system audit results as a formatted terminal table.
pub fn print_design_system_report(pairs: &[DesignSystemPair], minimum: MinimumLevel) {
    if pairs.is_empty() {
        println!(
            "{}",
            "No design system color pairs found.".yellow()
        );
        return;
    }

    println!();
    println!(
        "{}",
        " WCAG Contrast — Design System Audit ".bold().on_blue().white()
    );
    println!();

    // Header
    println!("{}", format_row(&[
        Column::left("Theme".bold().to_string(), 8),
        Column::left("Foreground".bold().to_string(), 28),
        Column::left("Background".bold().to_string(), 28),
        Column::right("Ratio".bold().to_string(), 8),
        Column::left("Level".bold().to_string(), 10),
    ]));
    println!("  {}", "─".repeat(86));

    let mut pass_count = 0;
    let mut fail_count = 0;

    for pair in pairs {
        let ratio_str = format!("{:.2}:1", pair.ratio);
        let level_str = format_level(pair.level);
        let passes = pair.level.meets_minimum(minimum);

        if passes {
            pass_count += 1;
        } else {
            fail_count += 1;
        }

        let fg_display = format!(
            "{} {}",
            pair.foreground_name,
            pair.foreground_color.to_hex().dimmed()
        );
        let bg_display = format!(
            "{} {}",
            pair.background_name,
            pair.background_color.to_hex().dimmed()
        );

        let line = format_row(&[
            Column::left(pair.theme.to_string(), 8),
            Column::left(fg_display, 28),
            Column::left(bg_display, 28),
            Column::right(ratio_str, 8),
            Column::left(level_str.to_string(), 10),
        ]);

        if passes {
            println!("{line}");
        } else {
            println!("{}", line.red());
        }
    }

    println!("  {}", "─".repeat(86));
    print_summary(pass_count, fail_count, minimum);
}

/// Print component scan results as a formatted terminal table.
pub fn print_component_report(pairs: &[ColorPair], minimum: MinimumLevel) {
    if pairs.is_empty() {
        println!(
            "{}",
            "No color pairs detected in components.".yellow()
        );
        return;
    }

    println!();
    println!(
        "{}",
        " WCAG Contrast — Component Scan ".bold().on_purple().white()
    );
    println!();

    println!("{}", format_row(&[
        Column::left("Location".bold().to_string(), 36),
        Column::left("Theme".bold().to_string(), 6),
        Column::left("Foreground".bold().to_string(), 20),
        Column::left("Background".bold().to_string(), 20),
        Column::right("Ratio".bold().to_string(), 8),
        Column::left("Level".bold().to_string(), 10),
    ]));
    println!("  {}", "─".repeat(105));

    let mut pass_count = 0;
    let mut fail_count = 0;

    for pair in pairs {
        let passes = pair.level.meets_minimum(minimum);
        if passes {
            pass_count += 1;
        } else {
            fail_count += 1;
        }

        let short_file = shorten_path(&pair.file);
        let location = format!("{}:{} <{}>", short_file, pair.line, pair.element);
        let ratio_str = format!("{:.2}:1", pair.ratio);
        let level_str = format_level(pair.level);

        let line = format_row(&[
            Column::left(truncate(&location, 34), 36),
            Column::left(pair.theme.clone(), 6),
            Column::left(truncate(&pair.foreground_name, 18), 20),
            Column::left(truncate(&pair.background_name, 18), 20),
            Column::right(ratio_str, 8),
            Column::left(level_str.to_string(), 10),
        ]);

        if passes {
            println!("{line}");
        } else {
            println!("{}", line.red());
        }
    }

    println!("  {}", "─".repeat(105));
    print_summary(pass_count, fail_count, minimum);
}

fn format_level(level: ConformanceLevel) -> ColoredString {
    match level {
        ConformanceLevel::Aaa => "AAA".green().bold(),
        ConformanceLevel::Aa => "AA".yellow().bold(),
        ConformanceLevel::AaLargeOnly => "AA Lg".yellow(),
        ConformanceLevel::Fail => "FAIL".red().bold(),
    }
}

fn print_summary(pass: usize, fail: usize, minimum: MinimumLevel) {
    let total = pass + fail;
    let min_label = match minimum {
        MinimumLevel::AaLarge => "AA Large",
        MinimumLevel::Aa => "AA",
        MinimumLevel::Aaa => "AAA",
    };

    println!();
    if fail == 0 {
        println!(
            "  {} All {total} pairs meet WCAG {min_label}",
            "PASS".green().bold()
        );
    } else {
        println!(
            "  {} {fail}/{total} pairs fail WCAG {min_label} ({pass} pass)",
            "FAIL".red().bold()
        );
    }
    println!();
}

fn shorten_path(path: &str) -> String {
    // Try to show just the last 2-3 path segments.
    // Use std::path for platform-agnostic separator handling.
    let components: Vec<&str> = std::path::Path::new(path)
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    if components.len() > 3 {
        let sep = std::path::MAIN_SEPARATOR;
        format!(
            "...{sep}{}",
            components[components.len() - 3..].join(&sep.to_string())
        )
    } else {
        path.to_string()
    }
}

#[derive(Debug, Clone, Copy)]
enum Align {
    Left,
    Right,
}

struct Column {
    value: String,
    width: usize,
    align: Align,
}

impl Column {
    fn left(value: String, width: usize) -> Self {
        Self {
            value,
            width,
            align: Align::Left,
        }
    }

    fn right(value: String, width: usize) -> Self {
        Self {
            value,
            width,
            align: Align::Right,
        }
    }
}

fn format_row(columns: &[Column]) -> String {
    let mut out = String::from("  ");
    for (idx, column) in columns.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(&pad_visible(&column.value, column.width, column.align));
    }
    out
}

fn pad_visible(value: &str, width: usize, align: Align) -> String {
    let visible = visible_width(value);
    if visible >= width {
        return value.to_string();
    }

    let padding = " ".repeat(width - visible);
    match align {
        Align::Left => format!("{value}{padding}"),
        Align::Right => format!("{padding}{value}"),
    }
}

fn visible_width(value: &str) -> usize {
    let mut width = 0;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && matches!(chars.peek(), Some('[')) {
            chars.next();
            for ansi_ch in chars.by_ref() {
                if ('@'..='~').contains(&ansi_ch) {
                    break;
                }
            }
            continue;
        }

        width += 1;
    }

    width
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
mod tests {
    use colored::Colorize;

    use super::{pad_visible, truncate, visible_width, Align};

    #[test]
    fn truncate_handles_utf8_without_panicking() {
        assert_eq!(truncate("héllo", 4), "h...");
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn visible_width_ignores_ansi_sequences() {
        assert_eq!(visible_width(&"hello".red().to_string()), 5);
        assert_eq!(visible_width(&"AA".yellow().bold().to_string()), 2);
    }

    #[test]
    fn pad_visible_aligns_colored_text_by_visible_width() {
        let padded = pad_visible(&"hex".dimmed().to_string(), 8, Align::Left);
        assert_eq!(visible_width(&padded), 8);

        let padded = pad_visible(&"4.50:1".green().to_string(), 10, Align::Right);
        assert_eq!(visible_width(&padded), 10);
    }
}
