use std::io::Write;

use crate::render::Renderer;
use crate::report::{Report, SIZE_BUCKET_ORDER, format_size};

const FOLDER_LIMIT: usize = 20;

/// Renders a [`Report`] as a Markdown document suitable for saving or pasting.
pub struct MarkdownRenderer;

impl Renderer for MarkdownRenderer {
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

impl MarkdownRenderer {
    fn render_summary(&self, r: &Report, out: &mut dyn Write) {
        writeln!(out, "# Email Statistics\n").unwrap();
        writeln!(out, "## Summary\n").unwrap();
        writeln!(out, "| Metric | Value |").unwrap();
        writeln!(out, "|--------|-------|").unwrap();
        writeln!(out, "| Total emails parsed | {} |", r.total_emails).unwrap();
        writeln!(out, "| Parse errors | {} |", r.parse_errors).unwrap();
        writeln!(
            out,
            "| Total size on disk | {} |",
            format_size(r.total_size_bytes)
        )
        .unwrap();
        writeln!(out, "| Average size | {} |", format_size(r.avg_size_bytes)).unwrap();
        if let Some((start, end)) = &r.date_range {
            writeln!(out, "| Date range | {} → {} |", start, end).unwrap();
        }
        writeln!(out).unwrap();
    }

    fn render_senders(&self, r: &Report, out: &mut dyn Write) {
        writeln!(out, "## Top {} Senders\n", r.top_senders_limit).unwrap();
        writeln!(out, "| Rank | Sender | Messages | Total Size |").unwrap();
        writeln!(out, "|------|--------|----------|------------|").unwrap();
        for (i, sender) in r.top_senders.iter().take(r.top_senders_limit).enumerate() {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                i + 1,
                sender.address,
                sender.count,
                format_size(sender.size_bytes),
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    fn render_years(&self, r: &Report, out: &mut dyn Write) {
        writeln!(out, "## Emails by Year\n").unwrap();
        writeln!(out, "| Year | Count |").unwrap();
        writeln!(out, "|------|-------|").unwrap();
        for (year, count) in &r.emails_by_year {
            writeln!(out, "| {} | {} |", year, count).unwrap();
        }
        if r.emails_without_date > 0 {
            writeln!(out, "| _(no date)_ | {} |", r.emails_without_date).unwrap();
        }
        writeln!(out).unwrap();
    }

    fn render_size_distribution(&self, r: &Report, out: &mut dyn Write) {
        writeln!(out, "## Size Distribution\n").unwrap();
        writeln!(out, "| Size Range | Count |").unwrap();
        writeln!(out, "|------------|-------|").unwrap();
        for (label, count) in &r.size_buckets {
            writeln!(out, "| {} | {} |", label, count).unwrap();
        }
        writeln!(out).unwrap();
    }

    fn render_folders(&self, r: &Report, out: &mut dyn Write) {
        writeln!(out, "## Emails by Folder\n").unwrap();
        writeln!(out, "| Folder | Messages | Total Size |").unwrap();
        writeln!(out, "|--------|----------|------------|").unwrap();
        let limit = if r.show_all_folders {
            r.folders.len()
        } else {
            FOLDER_LIMIT
        };
        for folder in r.folders.iter().take(limit) {
            writeln!(
                out,
                "| {} | {} | {} |",
                folder.name,
                folder.count,
                format_size(folder.size_bytes),
            )
            .unwrap();
        }
        if !r.show_all_folders && r.folders.len() > FOLDER_LIMIT {
            writeln!(
                out,
                "\n_{} more folders not shown. Use `--folders` to include all._",
                r.folders.len() - FOLDER_LIMIT
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ReportBuilder;
    use std::collections::HashMap;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn render_to_string(report: &Report) -> String {
        let mut buf: Vec<u8> = Vec::new();
        MarkdownRenderer.render(report, &mut buf);
        String::from_utf8(buf).expect("renderer produced non-UTF-8 output")
    }

    fn minimal_report() -> Report {
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
        .build()
    }

    fn report_with_senders(senders: &[(&str, usize, u64)]) -> Report {
        let mut b = ReportBuilder {
            total_emails: senders.iter().map(|(_, c, _)| c).sum(),
            top_senders_limit: 10,
            ..minimal_builder()
        };
        for (addr, count, size) in senders {
            b.sender_counts.insert(addr.to_string(), *count);
            b.sender_sizes.insert(addr.to_string(), *size);
        }
        b.build()
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

    fn contains_line(output: &str, expected: &str) -> bool {
        output.lines().any(|l| l == expected)
    }

    // ── Document structure ────────────────────────────────────────────────────

    #[test]
    fn output_starts_with_h1() {
        let output = render_to_string(&minimal_report());
        assert!(output.starts_with("# Email Statistics"), "got:\n{}", output);
    }

    #[test]
    fn summary_section_present() {
        let output = render_to_string(&minimal_report());
        assert!(output.contains("## Summary\n"), "got:\n{}", output);
    }

    #[test]
    fn senders_section_present() {
        let output = render_to_string(&minimal_report());
        assert!(output.contains("## Top 10 Senders\n"), "got:\n{}", output);
    }

    #[test]
    fn year_section_present() {
        let output = render_to_string(&minimal_report());
        assert!(output.contains("## Emails by Year\n"), "got:\n{}", output);
    }

    #[test]
    fn size_section_present() {
        let output = render_to_string(&minimal_report());
        assert!(
            output.contains("## Size Distribution\n"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn folder_section_absent_when_no_folders() {
        let output = render_to_string(&minimal_report());
        assert!(!output.contains("## Emails by Folder"), "got:\n{}", output);
    }

    // ── Summary table ─────────────────────────────────────────────────────────

    #[test]
    fn summary_table_has_header_row() {
        let output = render_to_string(&minimal_report());
        assert!(
            contains_line(&output, "| Metric | Value |"),
            "got:\n{}",
            output
        );
        assert!(
            contains_line(&output, "|--------|-------|"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn summary_shows_total_emails() {
        let mut b = minimal_builder();
        b.total_emails = 42;
        let output = render_to_string(&b.build());
        assert!(
            output.contains("| Total emails parsed | 42 |"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn summary_shows_parse_errors() {
        let mut b = minimal_builder();
        b.parse_errors = 3;
        let output = render_to_string(&b.build());
        assert!(output.contains("| Parse errors | 3 |"), "got:\n{}", output);
    }

    #[test]
    fn summary_shows_formatted_total_size() {
        let mut b = minimal_builder();
        b.total_size_bytes = 1_048_576;
        let output = render_to_string(&b.build());
        assert!(
            output.contains("| Total size on disk | 1.0 MB |"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn summary_shows_formatted_avg_size() {
        let mut b = minimal_builder();
        b.avg_size_bytes = 2048;
        let output = render_to_string(&b.build());
        assert!(
            output.contains("| Average size | 2.0 KB |"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn summary_shows_date_range_when_present() {
        let mut b = minimal_builder();
        b.date_range = Some(("2022-01-01".into(), "2024-12-31".into()));
        let output = render_to_string(&b.build());
        assert!(
            output.contains("| Date range | 2022-01-01 → 2024-12-31 |"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn summary_omits_date_range_when_absent() {
        let output = render_to_string(&minimal_report());
        assert!(!output.contains("Date range"), "got:\n{}", output);
    }

    // ── Senders table ─────────────────────────────────────────────────────────

    #[test]
    fn senders_table_has_header_row() {
        let output = render_to_string(&minimal_report());
        assert!(
            contains_line(&output, "| Rank | Sender | Messages | Total Size |"),
            "got:\n{}",
            output
        );
        assert!(
            contains_line(&output, "|------|--------|----------|------------|"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn senders_ranked_in_order() {
        let output = render_to_string(&report_with_senders(&[
            ("alice@x.com", 10, 0),
            ("bob@x.com", 5, 0),
        ]));
        let alice_pos = output.find("alice@x.com").expect("alice not found");
        let bob_pos = output.find("bob@x.com").expect("bob not found");
        assert!(alice_pos < bob_pos, "alice should appear before bob");
    }

    #[test]
    fn senders_rank_numbers_start_at_one() {
        let output = render_to_string(&report_with_senders(&[("a@x.com", 1, 0)]));
        assert!(output.contains("| 1 | a@x.com |"), "got:\n{}", output);
    }

    #[test]
    fn senders_shows_formatted_size() {
        let output = render_to_string(&report_with_senders(&[("a@x.com", 1, 4096)]));
        assert!(output.contains("4.0 KB"), "got:\n{}", output);
    }

    #[test]
    fn senders_capped_at_top_n() {
        let mut b = minimal_builder();
        b.top_senders_limit = 2;
        for i in 0..5u64 {
            b.sender_counts
                .insert(format!("s{}@x.com", i), (5 - i) as usize);
        }
        let output = render_to_string(&b.build());
        // Only ranks 1 and 2 should appear in data rows
        assert!(output.contains("| 1 |"), "got:\n{}", output);
        assert!(output.contains("| 2 |"), "got:\n{}", output);
        assert!(!output.contains("| 3 |"), "got:\n{}", output);
    }

    #[test]
    fn senders_heading_reflects_top_n() {
        let mut b = minimal_builder();
        b.top_senders_limit = 5;
        let output = render_to_string(&b.build());
        assert!(output.contains("## Top 5 Senders\n"), "got:\n{}", output);
    }

    // ── Years table ───────────────────────────────────────────────────────────

    #[test]
    fn years_table_has_header_row() {
        let output = render_to_string(&minimal_report());
        assert!(
            contains_line(&output, "| Year | Count |"),
            "got:\n{}",
            output
        );
        assert!(
            contains_line(&output, "|------|-------|"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn years_appear_in_ascending_order() {
        let mut b = minimal_builder();
        b.year_counts.insert(2023, 5);
        b.year_counts.insert(2021, 3);
        let output = render_to_string(&b.build());
        let pos_2021 = output.find("| 2021 |").expect("2021 not found");
        let pos_2023 = output.find("| 2023 |").expect("2023 not found");
        assert!(pos_2021 < pos_2023, "2021 should appear before 2023");
    }

    #[test]
    fn no_date_row_appears_when_undated_emails_exist() {
        let mut b = minimal_builder();
        b.emails_without_date = 4;
        let output = render_to_string(&b.build());
        assert!(output.contains("| _(no date)_ | 4 |"), "got:\n{}", output);
    }

    #[test]
    fn no_date_row_absent_when_all_emails_have_dates() {
        let output = render_to_string(&minimal_report());
        assert!(!output.contains("no date"), "got:\n{}", output);
    }

    // ── Size distribution table ───────────────────────────────────────────────

    #[test]
    fn size_table_has_header_row() {
        let output = render_to_string(&minimal_report());
        assert!(
            contains_line(&output, "| Size Range | Count |"),
            "got:\n{}",
            output
        );
        assert!(
            contains_line(&output, "|------------|-------|"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn all_size_buckets_present() {
        let output = render_to_string(&minimal_report());
        for label in SIZE_BUCKET_ORDER {
            assert!(
                output.contains(&format!("| {} |", label)),
                "missing bucket '{}' in:\n{}",
                label,
                output
            );
        }
    }

    #[test]
    fn size_bucket_counts_correct() {
        let mut b = minimal_builder();
        b.size_bucket_counts.insert("< 1 KB", 7);
        let output = render_to_string(&b.build());
        assert!(output.contains("| < 1 KB | 7 |"), "got:\n{}", output);
    }

    // ── Folders table ─────────────────────────────────────────────────────────

    #[test]
    fn folder_section_present_when_folders_exist() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Inbox".into(), 3);
        let output = render_to_string(&b.build());
        assert!(output.contains("## Emails by Folder\n"), "got:\n{}", output);
    }

    #[test]
    fn folder_table_has_header_row() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Inbox".into(), 1);
        let output = render_to_string(&b.build());
        assert!(
            contains_line(&output, "| Folder | Messages | Total Size |"),
            "got:\n{}",
            output
        );
        assert!(
            contains_line(&output, "|--------|----------|------------|"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn folder_rows_show_name_count_and_size() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Sent".into(), 8);
        b.folder_sizes.insert("Sent".into(), 1024);
        let output = render_to_string(&b.build());
        assert!(output.contains("| Sent | 8 | 1.0 KB |"), "got:\n{}", output);
    }

    #[test]
    fn folder_overflow_note_shown_when_capped() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.show_all_folders = false;
        for i in 0..=FOLDER_LIMIT {
            b.folder_counts.insert(format!("Folder{}", i), 1);
        }
        let output = render_to_string(&b.build());
        assert!(
            output.contains("more folders not shown"),
            "got:\n{}",
            output
        );
    }

    #[test]
    fn folder_overflow_note_absent_when_show_all() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.show_all_folders = true;
        for i in 0..=FOLDER_LIMIT {
            b.folder_counts.insert(format!("Folder{}", i), 1);
        }
        let output = render_to_string(&b.build());
        assert!(
            !output.contains("more folders not shown"),
            "got:\n{}",
            output
        );
    }
}
