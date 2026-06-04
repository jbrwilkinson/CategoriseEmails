use crate::render::Renderer;
use crate::report::{Report, format_size};

const BAR_WIDTH: usize = 20;
const COL_WIDTH: usize = 42;
const FOLDER_LIMIT: usize = 20;

/// Renders a [`Report`] as a human-readable terminal summary with bar charts.
pub struct TextRenderer;

impl Renderer for TextRenderer {
    fn render(&self, report: &Report) {
        self.render_summary(report);
        self.render_senders(report);
        self.render_years(report);
        self.render_size_distribution(report);
        if !report.folders.is_empty() {
            self.render_folders(report);
        }
    }
}

impl TextRenderer {
    fn render_summary(&self, r: &Report) {
        section_header("AGGREGATE STATISTICS");
        println!();
        println!("  Total emails parsed : {}", r.total_emails);
        println!("  Parse errors        : {}", r.parse_errors);
        println!(
            "  Total size on disk  : {}",
            format_size(r.total_size_bytes)
        );
        println!("  Average size        : {}", format_size(r.avg_size_bytes));
        if let Some((start, end)) = &r.date_range {
            println!("  Date range          : {} → {}", start, end);
        }
        println!();
    }

    fn render_senders(&self, r: &Report) {
        section_subheader(&format!(
            "TOP {} SENDERS  (by message count)",
            r.top_senders_limit
        ));
        let max = r.top_senders.first().map(|s| s.count).unwrap_or(1);
        for sender in r.top_senders.iter().take(r.top_senders_limit) {
            print_bar(
                &format!("{} ({})", sender.address, format_size(sender.size_bytes)),
                sender.count,
                max,
            );
        }
        println!();
    }

    fn render_years(&self, r: &Report) {
        section_subheader("EMAILS BY YEAR");
        let max = r
            .emails_by_year
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1)
            .max(r.emails_without_date);
        for (year, count) in &r.emails_by_year {
            print_bar(&year.to_string(), *count, max);
        }
        if r.emails_without_date > 0 {
            print_bar("(no date)", r.emails_without_date, max);
        }
        println!();
    }

    fn render_size_distribution(&self, r: &Report) {
        section_subheader("SIZE DISTRIBUTION");
        let max = r.size_buckets.iter().map(|(_, c)| *c).max().unwrap_or(1);
        for (label, count) in &r.size_buckets {
            print_bar(label, *count, max);
        }
        println!();
    }

    fn render_folders(&self, r: &Report) {
        section_subheader("EMAILS BY FOLDER");
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
            );
        }
        if !r.show_all_folders && r.folders.len() > FOLDER_LIMIT {
            println!(
                "  ... ({} more folders, use --folders to show all)",
                r.folders.len() - FOLDER_LIMIT
            );
        }
        println!();
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn section_header(title: &str) {
    println!("══════════════════════════════════════════════════════════════════════");
    println!(" {}", title);
    println!("══════════════════════════════════════════════════════════════════════");
}

fn section_subheader(title: &str) {
    println!("──────────────────────────────────────────────────────────────────────");
    println!(" {}", title);
    println!("──────────────────────────────────────────────────────────────────────");
}

fn print_bar(label: &str, value: usize, max_value: usize) {
    let filled = if max_value > 0 {
        value * BAR_WIDTH / max_value
    } else {
        0
    };
    let bar: String = std::iter::repeat('█')
        .take(filled)
        .chain(std::iter::repeat('░').take(BAR_WIDTH - filled))
        .collect();
    println!(
        "  {:<col$} {} {:>6}",
        truncate(label, COL_WIDTH),
        bar,
        value,
        col = COL_WIDTH
    );
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars - 1].iter().collect::<String>() + "…"
    }
}
