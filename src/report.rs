use std::collections::HashMap;

// ── Public data types ─────────────────────────────────────────────────────────

/// A single folder's aggregate numbers.
#[derive(Debug, PartialEq)]
pub struct FolderStat {
    pub name: String,
    pub count: usize,
    pub size_bytes: u64,
}

/// A sender's aggregate numbers.
#[derive(Debug, PartialEq)]
pub struct SenderStat {
    pub address: String,
    pub count: usize,
    pub size_bytes: u64,
}

/// Pure aggregate data, independent of any output format.
#[derive(Debug)]
pub struct Report {
    pub total_emails: usize,
    pub parse_errors: usize,
    pub total_size_bytes: u64,
    pub avg_size_bytes: u64,
    /// Inclusive date range as "YYYY-MM-DD" strings.
    pub date_range: Option<(String, String)>,

    /// Senders sorted descending by message count.
    pub top_senders: Vec<SenderStat>,
    /// How many senders a renderer should show.
    pub top_senders_limit: usize,

    /// Emails per calendar year, sorted ascending.
    pub emails_by_year: Vec<(i32, usize)>,
    /// Emails with no parseable Date header.
    pub emails_without_date: usize,

    /// Size-bucket distribution in canonical order (see `SIZE_BUCKET_ORDER`).
    pub size_buckets: Vec<(&'static str, usize)>,

    /// Per-folder breakdown — empty when only one root dir was scanned.
    pub folders: Vec<FolderStat>,
    /// Hint to renderers: whether to cap the folder list.
    pub show_all_folders: bool,
}

// ── Canonical size-bucket labels (shared with main for classification) ────────

pub const SIZE_BUCKET_ORDER: [&str; 6] = [
    "< 1 KB",
    "1–10 KB",
    "10–100 KB",
    "100 KB–1 MB",
    "1–10 MB",
    "> 10 MB",
];

// ── Shared formatting utility ─────────────────────────────────────────────────

/// Format a byte count as a human-readable size string.
pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct ReportBuilder {
    pub total_emails: usize,
    pub parse_errors: usize,
    pub total_size_bytes: u64,
    pub avg_size_bytes: u64,
    pub date_range: Option<(String, String)>,
    pub sender_counts: HashMap<String, usize>,
    pub sender_sizes: HashMap<String, u64>,
    pub year_counts: HashMap<i32, usize>,
    pub emails_without_date: usize,
    pub size_bucket_counts: HashMap<&'static str, usize>,
    pub folder_counts: HashMap<String, usize>,
    pub folder_sizes: HashMap<String, u64>,
    pub top_senders_limit: usize,
    pub show_all_folders: bool,
    /// When `false` (single root dir), the folder list is left empty.
    pub include_folders: bool,
}

impl ReportBuilder {
    pub fn build(self) -> Report {
        // Senders: merge counts + sizes, sort descending by count
        let mut senders: Vec<SenderStat> = self
            .sender_counts
            .into_iter()
            .map(|(address, count)| {
                let size_bytes = self.sender_sizes.get(&address).copied().unwrap_or(0);
                SenderStat {
                    address,
                    count,
                    size_bytes,
                }
            })
            .collect();
        senders.sort_by(|a, b| b.count.cmp(&a.count));

        // Years: sort ascending
        let mut years: Vec<(i32, usize)> = self.year_counts.into_iter().collect();
        years.sort_by_key(|(y, _)| *y);

        // Size buckets: enforce canonical display order
        let size_buckets = SIZE_BUCKET_ORDER
            .iter()
            .map(|&label| {
                (
                    label,
                    self.size_bucket_counts.get(label).copied().unwrap_or(0),
                )
            })
            .collect();

        // Folders: sort descending by count, omit entirely for single-root scans
        let folders = if self.include_folders {
            let mut v: Vec<FolderStat> = self
                .folder_counts
                .into_iter()
                .map(|(name, count)| {
                    let size_bytes = self.folder_sizes.get(&name).copied().unwrap_or(0);
                    FolderStat {
                        name,
                        count,
                        size_bytes,
                    }
                })
                .collect();
            v.sort_by(|a, b| b.count.cmp(&a.count));
            v
        } else {
            vec![]
        };

        Report {
            total_emails: self.total_emails,
            parse_errors: self.parse_errors,
            total_size_bytes: self.total_size_bytes,
            avg_size_bytes: self.avg_size_bytes,
            date_range: self.date_range,
            top_senders: senders,
            top_senders_limit: self.top_senders_limit,
            emails_by_year: years,
            emails_without_date: self.emails_without_date,
            size_buckets,
            folders,
            show_all_folders: self.show_all_folders,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── format_size ───────────────────────────────────────────────────────────

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(2048), "2.0 KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1_048_576), "1.0 MB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    // ── ReportBuilder::build — senders ────────────────────────────────────────

    #[test]
    fn senders_sorted_descending_by_count() {
        let mut b = minimal_builder();
        b.sender_counts.insert("a@x.com".into(), 1);
        b.sender_counts.insert("b@x.com".into(), 5);
        b.sender_counts.insert("c@x.com".into(), 3);
        let r = b.build();
        let counts: Vec<usize> = r.top_senders.iter().map(|s| s.count).collect();
        assert_eq!(counts, vec![5, 3, 1]);
    }

    #[test]
    fn sender_size_attached_correctly() {
        let mut b = minimal_builder();
        b.sender_counts.insert("a@x.com".into(), 2);
        b.sender_sizes.insert("a@x.com".into(), 4096);
        let r = b.build();
        assert_eq!(r.top_senders[0].size_bytes, 4096);
    }

    #[test]
    fn sender_size_defaults_to_zero_when_missing() {
        let mut b = minimal_builder();
        b.sender_counts.insert("a@x.com".into(), 1);
        let r = b.build();
        assert_eq!(r.top_senders[0].size_bytes, 0);
    }

    // ── ReportBuilder::build — years ──────────────────────────────────────────

    #[test]
    fn years_sorted_ascending() {
        let mut b = minimal_builder();
        b.year_counts.insert(2023, 10);
        b.year_counts.insert(2021, 5);
        b.year_counts.insert(2022, 8);
        let r = b.build();
        let years: Vec<i32> = r.emails_by_year.iter().map(|(y, _)| *y).collect();
        assert_eq!(years, vec![2021, 2022, 2023]);
    }

    // ── ReportBuilder::build — size buckets ───────────────────────────────────

    #[test]
    fn size_buckets_in_display_order() {
        let mut b = minimal_builder();
        b.size_bucket_counts.insert("> 10 MB", 1);
        b.size_bucket_counts.insert("< 1 KB", 9);
        let r = b.build();
        let labels: Vec<&str> = r.size_buckets.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, SIZE_BUCKET_ORDER.to_vec());
    }

    #[test]
    fn size_bucket_zero_for_missing_label() {
        let b = minimal_builder();
        let r = b.build();
        assert!(r.size_buckets.iter().all(|(_, c)| *c == 0));
    }

    // ── ReportBuilder::build — folders ────────────────────────────────────────

    #[test]
    fn folders_empty_when_include_folders_false() {
        let mut b = minimal_builder();
        b.folder_counts.insert("Inbox".into(), 5);
        b.include_folders = false;
        let r = b.build();
        assert!(r.folders.is_empty());
    }

    #[test]
    fn folders_sorted_descending_by_count() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Sent".into(), 2);
        b.folder_counts.insert("Inbox".into(), 7);
        b.folder_counts.insert("Archive".into(), 4);
        let r = b.build();
        let names: Vec<&str> = r.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["Inbox", "Archive", "Sent"]);
    }

    #[test]
    fn folder_size_attached_correctly() {
        let mut b = minimal_builder();
        b.include_folders = true;
        b.folder_counts.insert("Inbox".into(), 3);
        b.folder_sizes.insert("Inbox".into(), 8192);
        let r = b.build();
        assert_eq!(r.folders[0].size_bytes, 8192);
    }

    // ── Report fields ─────────────────────────────────────────────────────────

    #[test]
    fn date_range_preserved() {
        let mut b = minimal_builder();
        b.date_range = Some(("2020-01-01".into(), "2024-12-31".into()));
        let r = b.build();
        assert_eq!(
            r.date_range,
            Some(("2020-01-01".into(), "2024-12-31".into()))
        );
    }

    #[test]
    fn totals_preserved() {
        let mut b = minimal_builder();
        b.total_emails = 42;
        b.parse_errors = 2;
        b.total_size_bytes = 1_048_576;
        b.avg_size_bytes = 24_966;
        let r = b.build();
        assert_eq!(r.total_emails, 42);
        assert_eq!(r.parse_errors, 2);
        assert_eq!(r.total_size_bytes, 1_048_576);
        assert_eq!(r.avg_size_bytes, 24_966);
    }
}
