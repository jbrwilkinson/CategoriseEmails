use std::io::Write;

use crate::render::Renderer;
use crate::report::{Report, format_size};

const FOLDER_LIMIT: usize = 20;

// ── Layout ────────────────────────────────────────────────────────────────────

/// Fixed characters on every bar row that are not part of the label or bar.
///
/// `"  "` indent + `" "` label→bar gap + `" "` bar→count gap + 6-char count = 10
const FIXED_OVERHEAD: usize = 10;

/// Minimum widths so the output doesn't collapse into gibberish.
const MIN_BAR_WIDTH: usize = 5;
const MIN_COL_WIDTH: usize = 10;

/// Fraction of the flexible space (total_width − overhead) given to the bar.
const BAR_FRACTION_NUMERATOR: usize = 1;
const BAR_FRACTION_DENOMINATOR: usize = 3;

/// Derived per-render layout, computed once from `total_width`.
struct Layout {
    col_width: usize,
    bar_width: usize,
    /// The `═══` / `───` rule width matches total_width.
    rule_width: usize,
}

impl Layout {
    fn from_total_width(total_width: usize) -> Self {
        let flexible = total_width.saturating_sub(FIXED_OVERHEAD);
        let bar_width =
            (flexible * BAR_FRACTION_NUMERATOR / BAR_FRACTION_DENOMINATOR).max(MIN_BAR_WIDTH);
        let col_width = flexible.saturating_sub(bar_width).max(MIN_COL_WIDTH);
        Self {
            col_width,
            bar_width,
            rule_width: total_width,
        }
    }
}

// ── Renderer ──────────────────────────────────────────────────────────────────

/// Renders a [`Report`] as a human-readable console summary with bar charts.
///
/// `total_width` is the number of character columns the output may occupy.
/// Use [`ConsoleRenderer::with_terminal_percent`] to derive it automatically.
pub struct ConsoleRenderer {
    pub total_width: usize,
}

/// Minimum sensible total width (prevents layout collapse at extreme settings).
pub const MIN_TOTAL_WIDTH: usize = FIXED_OVERHEAD + MIN_COL_WIDTH + MIN_BAR_WIDTH;

impl Renderer for ConsoleRenderer {
    fn render(&self, report: &Report, out: &mut dyn Write) {
        let layout = Layout::from_total_width(self.total_width);
        self.render_summary(report, out, &layout);
        self.render_senders(report, out, &layout);
        self.render_years(report, out, &layout);
        self.render_size_distribution(report, out, &layout);
        if !report.folders.is_empty() {
            self.render_folders(report, out, &layout);
        }
    }
}

impl ConsoleRenderer {
    fn render_summary(&self, r: &Report, out: &mut dyn Write, layout: &Layout) {
        section_header("AGGREGATE STATISTICS", out, layout);
        writeln!(out).unwrap();
        writeln!(out, "  Total emails parsed : {}", r.total_emails).unwrap();
        writeln!(out, "  Parse errors        : {}", r.parse_errors).unwrap();
        writeln!(
            out,
            "  Total size on disk  : {}",
            format_size(r.total_size_bytes)
        )
        .unwrap();
        writeln!(
            out,
            "  Average size        : {}",
            format_size(r.avg_size_bytes)
        )
        .unwrap();
        if let Some((start, end)) = &r.date_range {
            writeln!(out, "  Date range          : {} → {}", start, end).unwrap();
        }
        writeln!(out).unwrap();
    }

    fn render_senders(&self, r: &Report, out: &mut dyn Write, layout: &Layout) {
        section_subheader(
            &format!("TOP {} SENDERS  (by message count)", r.top_senders_limit),
            out,
            layout,
        );
        let max = r.top_senders.first().map(|s| s.count).unwrap_or(1);
        for sender in r.top_senders.iter().take(r.top_senders_limit) {
            print_bar(
                &format!("{} ({})", sender.address, format_size(sender.size_bytes)),
                sender.count,
                max,
                out,
                layout,
            );
        }
        writeln!(out).unwrap();
    }

    fn render_years(&self, r: &Report, out: &mut dyn Write, layout: &Layout) {
        section_subheader("EMAILS BY YEAR", out, layout);
        let max = r
            .emails_by_year
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1)
            .max(r.emails_without_date);
        for (year, count) in &r.emails_by_year {
            print_bar(&year.to_string(), *count, max, out, layout);
        }
        if r.emails_without_date > 0 {
            print_bar("(no date)", r.emails_without_date, max, out, layout);
        }
        writeln!(out).unwrap();
    }

    fn render_size_distribution(&self, r: &Report, out: &mut dyn Write, layout: &Layout) {
        section_subheader("SIZE DISTRIBUTION", out, layout);
        let max = r.size_buckets.iter().map(|(_, c)| *c).max().unwrap_or(1);
        for (label, count) in &r.size_buckets {
            print_bar(label, *count, max, out, layout);
        }
        writeln!(out).unwrap();
    }

    fn render_folders(&self, r: &Report, out: &mut dyn Write, layout: &Layout) {
        section_subheader("EMAILS BY FOLDER", out, layout);
        let max = r.folders.iter().map(|f| f.count).max().unwrap_or(1);
        let limit = if r.show_all_folders {
            r.folders.len()
        } else {
            FOLDER_LIMIT
        };
        for folder in r.folders.iter().take(limit) {
            print_bar(
                &format!("{} ({})", folder.name, format_size(folder.size_bytes)),
                folder.count,
                max,
                out,
                layout,
            );
        }
        if !r.show_all_folders && r.folders.len() > FOLDER_LIMIT {
            writeln!(
                out,
                "  ... ({} more folders, use --folders to show all)",
                r.folders.len() - FOLDER_LIMIT
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn section_header(title: &str, out: &mut dyn Write, layout: &Layout) {
    let rule = "═".repeat(layout.rule_width);
    writeln!(out, "{}", rule).unwrap();
    writeln!(out, " {}", title).unwrap();
    writeln!(out, "{}", rule).unwrap();
}

fn section_subheader(title: &str, out: &mut dyn Write, layout: &Layout) {
    let rule = "─".repeat(layout.rule_width);
    writeln!(out, "{}", rule).unwrap();
    writeln!(out, " {}", title).unwrap();
    writeln!(out, "{}", rule).unwrap();
}

fn print_bar(label: &str, value: usize, max_value: usize, out: &mut dyn Write, layout: &Layout) {
    let filled = if max_value > 0 {
        value * layout.bar_width / max_value
    } else {
        0
    };
    let bar: String = std::iter::repeat_n('█', filled)
        .chain(std::iter::repeat_n('░', layout.bar_width - filled))
        .collect();
    writeln!(
        out,
        "  {:<col$} {} {:>6}",
        truncate(label, layout.col_width),
        bar,
        value,
        col = layout.col_width,
    )
    .unwrap();
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars - 1].iter().collect::<String>() + "…"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ReportBuilder;
    use std::collections::HashMap;

    fn render_to_string(renderer: &ConsoleRenderer, report: &Report) -> String {
        let mut buf: Vec<u8> = Vec::new();
        renderer.render(report, &mut buf);
        String::from_utf8(buf).expect("renderer produced non-UTF-8 output")
    }

    fn minimal_builder() -> ReportBuilder {
        ReportBuilder {
            total_emails: 0,
            parse_errors: 0,
            total_size_bytes: 0,
            avg_size_bytes: 0,
            date_range: None,
            sender_counts: HashMap::new(),
            sender_sizes: HashMap::new(),
            year_counts: HashMap::new(),
            emails_without_date: 0,
            size_bucket_counts: HashMap::new(),
            folder_counts: HashMap::new(),
            folder_sizes: HashMap::new(),
            top_senders_limit: 10,
            show_all_folders: false,
            include_folders: false,
        }
    }

    fn minimal_report() -> Report {
        minimal_builder().build()
    }

    fn renderer(total_width: usize) -> ConsoleRenderer {
        ConsoleRenderer { total_width }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    #[test]
    fn layout_bar_width_is_one_third_of_flexible_space() {
        let layout = Layout::from_total_width(100);
        let flexible = 100 - FIXED_OVERHEAD;
        assert_eq!(layout.bar_width, flexible / 3);
    }

    #[test]
    fn layout_col_width_fills_remaining_flexible_space() {
        let layout = Layout::from_total_width(100);
        assert_eq!(layout.col_width + layout.bar_width, 100 - FIXED_OVERHEAD);
    }

    #[test]
    fn layout_respects_minimum_bar_width() {
        let layout = Layout::from_total_width(0);
        assert!(layout.bar_width >= MIN_BAR_WIDTH);
    }

    #[test]
    fn layout_respects_minimum_col_width() {
        let layout = Layout::from_total_width(0);
        assert!(layout.col_width >= MIN_COL_WIDTH);
    }

    #[test]
    fn rule_width_matches_total_width() {
        let layout = Layout::from_total_width(72);
        assert_eq!(layout.rule_width, 72);
    }

    // ── Width reflected in output ─────────────────────────────────────────────

    #[test]
    fn header_rule_length_matches_total_width() {
        let output = render_to_string(&renderer(60), &minimal_report());
        let rule_line = output.lines().next().unwrap();
        // Each '═' is 3 bytes in UTF-8; count chars not bytes
        assert_eq!(rule_line.chars().count(), 60);
    }

    #[test]
    fn wider_renderer_produces_wider_rules() {
        let narrow = render_to_string(&renderer(50), &minimal_report());
        let wide = render_to_string(&renderer(120), &minimal_report());
        let narrow_rule = narrow.lines().next().unwrap().chars().count();
        let wide_rule = wide.lines().next().unwrap().chars().count();
        assert!(wide_rule > narrow_rule);
    }

    // ── Content ───────────────────────────────────────────────────────────────

    #[test]
    fn summary_header_present() {
        let output = render_to_string(&renderer(72), &minimal_report());
        assert!(output.contains("AGGREGATE STATISTICS"), "got:\n{}", output);
    }

    #[test]
    fn total_emails_shown_in_summary() {
        let mut b = minimal_builder();
        b.total_emails = 99;
        let output = render_to_string(&renderer(72), &b.build());
        assert!(output.contains("99"), "got:\n{}", output);
    }

    #[test]
    fn date_range_shown_when_present() {
        let mut b = minimal_builder();
        b.date_range = Some(("2021-01-01".into(), "2024-12-31".into()));
        let output = render_to_string(&renderer(72), &b.build());
        assert!(output.contains("2021-01-01"), "got:\n{}", output);
        assert!(output.contains("2024-12-31"), "got:\n{}", output);
    }

    #[test]
    fn folder_section_absent_when_no_folders() {
        let output = render_to_string(&renderer(72), &minimal_report());
        assert!(!output.contains("EMAILS BY FOLDER"), "got:\n{}", output);
    }

    #[test]
    fn folder_section_present_when_folders_exist() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Inbox".into(), 5);
        let output = render_to_string(&renderer(72), &b.build());
        assert!(output.contains("EMAILS BY FOLDER"), "got:\n{}", output);
        assert!(output.contains("Inbox"), "got:\n{}", output);
    }
}
