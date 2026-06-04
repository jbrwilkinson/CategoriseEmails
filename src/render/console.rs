use std::io::Write;

use crate::render::Renderer;
use crate::report::{Report, format_size};

const BAR_WIDTH: usize = 20;
const COL_WIDTH: usize = 42;
const FOLDER_LIMIT: usize = 20;

/// Renders a [`Report`] as a human-readable console summary with bar charts.
pub struct ConsoleRenderer;

impl Renderer for ConsoleRenderer {
    fn render(&self, report: &Report, out: &mut dyn Write) {
        self.render_summary(report, out);
        self.render_senders(report, out);
        self.render_years(report, out);
        self.render_size_distribution(report, out);
        if !report.folders.is_empty() {
            self.render_folders(report, out);
        }
    }
}

impl ConsoleRenderer {
    fn render_summary(&self, r: &Report, out: &mut dyn Write) {
        section_header("AGGREGATE STATISTICS", out);
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

    fn render_senders(&self, r: &Report, out: &mut dyn Write) {
        section_subheader(
            &format!("TOP {} SENDERS  (by message count)", r.top_senders_limit),
            out,
        );
        let max = r.top_senders.first().map(|s| s.count).unwrap_or(1);
        for sender in r.top_senders.iter().take(r.top_senders_limit) {
            print_bar(
                &format!("{} ({})", sender.address, format_size(sender.size_bytes)),
                sender.count,
                max,
                out,
            );
        }
        writeln!(out).unwrap();
    }

    fn render_years(&self, r: &Report, out: &mut dyn Write) {
        section_subheader("EMAILS BY YEAR", out);
        let max = r
            .emails_by_year
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1)
            .max(r.emails_without_date);
        for (year, count) in &r.emails_by_year {
            print_bar(&year.to_string(), *count, max, out);
        }
        if r.emails_without_date > 0 {
            print_bar("(no date)", r.emails_without_date, max, out);
        }
        writeln!(out).unwrap();
    }

    fn render_size_distribution(&self, r: &Report, out: &mut dyn Write) {
        section_subheader("SIZE DISTRIBUTION", out);
        let max = r.size_buckets.iter().map(|(_, c)| *c).max().unwrap_or(1);
        for (label, count) in &r.size_buckets {
            print_bar(label, *count, max, out);
        }
        writeln!(out).unwrap();
    }

    fn render_folders(&self, r: &Report, out: &mut dyn Write) {
        section_subheader("EMAILS BY FOLDER", out);
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

fn section_header(title: &str, out: &mut dyn Write) {
    writeln!(
        out,
        "══════════════════════════════════════════════════════════════════════"
    )
    .unwrap();
    writeln!(out, " {}", title).unwrap();
    writeln!(
        out,
        "══════════════════════════════════════════════════════════════════════"
    )
    .unwrap();
}

fn section_subheader(title: &str, out: &mut dyn Write) {
    writeln!(
        out,
        "──────────────────────────────────────────────────────────────────────"
    )
    .unwrap();
    writeln!(out, " {}", title).unwrap();
    writeln!(
        out,
        "──────────────────────────────────────────────────────────────────────"
    )
    .unwrap();
}

fn print_bar(label: &str, value: usize, max_value: usize, out: &mut dyn Write) {
    let filled = if max_value > 0 {
        value * BAR_WIDTH / max_value
    } else {
        0
    };
    let bar: String = std::iter::repeat('█')
        .take(filled)
        .chain(std::iter::repeat('░').take(BAR_WIDTH - filled))
        .collect();
    writeln!(
        out,
        "  {:<col$} {} {:>6}",
        truncate(label, COL_WIDTH),
        bar,
        value,
        col = COL_WIDTH
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
