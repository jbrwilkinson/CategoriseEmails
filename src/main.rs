mod emlx;

use chrono::{DateTime, FixedOffset};
use clap::Parser;
use emlx::Email;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "email-analyser")]
#[command(about = "Parse email files and show aggregate statistics")]
struct Args {
    /// Directories to scan for email files
    #[arg(required = true)]
    dirs: Vec<PathBuf>,

    /// Show top N senders (default: 10)
    #[arg(short = 'n', long, default_value = "10")]
    top_n: usize,

    /// Show all folders (default: top 20)
    #[arg(short, long)]
    folders: bool,

    /// Print one line per parsed email
    #[arg(short, long)]
    verbose: bool,
}

fn find_emlx_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in dirs {
        for entry in WalkDir::new(dir).follow_links(true) {
            match entry {
                Ok(e) if e.file_type().is_file() => {
                    if e.path().extension().and_then(|e| e.to_str()) == Some("emlx") {
                        files.push(e.path().to_path_buf());
                    }
                }
                Err(e) => eprintln!("Warning: {e}"),
                _ => {}
            }
        }
    }
    files
}

fn folder_label(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root) {
            if let Some(parent) = rel.parent() {
                let label = parent.to_string_lossy();
                if !label.is_empty() {
                    return label.into_owned();
                }
            }
        }
    }
    path.parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(unknown)".to_string())
}

fn print_bar(label: &str, value: usize, max_value: usize, bar_width: usize) {
    let filled = if max_value > 0 {
        value * bar_width / max_value
    } else {
        0
    };
    let bar: String = std::iter::repeat('█')
        .take(filled)
        .chain(std::iter::repeat('░').take(bar_width - filled))
        .collect();
    println!("  {:<42} {} {:>6}", truncate(label, 42), bar, value);
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars - 1].iter().collect::<String>() + "…"
    }
}

fn format_size(bytes: u64) -> String {
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

struct Stats<'a> {
    emails: &'a [Email],
}

impl<'a> Stats<'a> {
    fn sender_counts(&self) -> HashMap<&str, usize> {
        let mut m = HashMap::new();
        for e in self.emails {
            *m.entry(e.from.as_str()).or_default() += 1;
        }
        m
    }

    fn sender_sizes(&self) -> HashMap<&str, u64> {
        let mut m = HashMap::new();
        for e in self.emails {
            *m.entry(e.from.as_str()).or_default() += e.size_bytes;
        }
        m
    }

    fn year_counts(&self) -> (HashMap<i32, usize>, usize) {
        let mut m = HashMap::new();
        let mut no_date = 0;
        for e in self.emails {
            match e.date {
                Some(d) => {
                    *m.entry(d.format("%Y").to_string().parse::<i32>().unwrap_or(0))
                        .or_default() += 1
                }
                None => no_date += 1,
            }
        }
        (m, no_date)
    }

    fn size_buckets(&self) -> HashMap<&'static str, usize> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for e in self.emails {
            let label = match e.size_bytes {
                0..=1_023 => "< 1 KB",
                1_024..=9_999 => "1–10 KB",
                10_000..=99_999 => "10–100 KB",
                100_000..=999_999 => "100 KB–1 MB",
                1_000_000..=9_999_999 => "1–10 MB",
                _ => "> 10 MB",
            };
            *m.entry(label).or_default() += 1;
        }
        m
    }

    fn total_size(&self) -> u64 {
        self.emails.iter().map(|e| e.size_bytes).sum()
    }

    fn date_range(&self) -> (Option<DateTime<FixedOffset>>, Option<DateTime<FixedOffset>>) {
        let mut dated: Vec<DateTime<FixedOffset>> =
            self.emails.iter().filter_map(|e| e.date).collect();
        dated.sort();
        (dated.first().cloned(), dated.last().cloned())
    }
}

fn main() {
    let args = Args::parse();

    println!("Scanning for .emlx files...");
    let paths = find_emlx_files(&args.dirs);
    println!("Found {} .emlx files\n", paths.len());
    if paths.is_empty() {
        return;
    }

    let mut emails: Vec<(PathBuf, Email)> = Vec::new();
    let mut parse_errors = 0usize;

    for path in &paths {
        match emlx::parse_emlx(path) {
            Some(email) => {
                if args.verbose {
                    println!(
                        "[{}] From: {} | Subject: {} | Date: {}",
                        path.display(),
                        email.from,
                        email.subject,
                        email
                            .date
                            .map(|d| d.to_rfc2822())
                            .unwrap_or_else(|| "(no date)".into())
                    );
                }
                emails.push((path.clone(), email));
            }
            None => {
                parse_errors += 1;
                if args.verbose {
                    eprintln!("Failed to parse: {}", path.display());
                }
            }
        }
    }

    // Folder stats (needs path alongside email)
    let mut folder_counts: HashMap<String, usize> = HashMap::new();
    let mut folder_sizes: HashMap<String, u64> = HashMap::new();
    for (path, email) in &emails {
        let folder = folder_label(path, &args.dirs);
        *folder_counts.entry(folder.clone()).or_default() += 1;
        *folder_sizes.entry(folder).or_default() += email.size_bytes;
    }

    let email_slice: Vec<Email> = emails.into_iter().map(|(_, e)| e).collect();
    let stats = Stats {
        emails: &email_slice,
    };

    let total = email_slice.len();
    let total_size = stats.total_size();
    let avg_size = if total > 0 {
        total_size / total as u64
    } else {
        0
    };
    let (earliest, latest) = stats.date_range();

    println!("══════════════════════════════════════════════════════════════════════");
    println!(" AGGREGATE STATISTICS");
    println!("══════════════════════════════════════════════════════════════════════");
    println!();
    println!("  Total emails parsed : {}", total);
    println!("  Parse errors        : {}", parse_errors);
    println!("  Total size on disk  : {}", format_size(total_size));
    println!("  Average size        : {}", format_size(avg_size));
    if let (Some(e), Some(l)) = (earliest, latest) {
        println!(
            "  Date range          : {} → {}",
            e.format("%Y-%m-%d"),
            l.format("%Y-%m-%d")
        );
    }
    println!();

    let sender_counts = stats.sender_counts();
    let sender_sizes = stats.sender_sizes();
    println!("──────────────────────────────────────────────────────────────────────");
    println!(" TOP {} SENDERS  (by message count)", args.top_n);
    println!("──────────────────────────────────────────────────────────────────────");
    let mut senders: Vec<(&str, usize)> = sender_counts.iter().map(|(&k, &v)| (k, v)).collect();
    senders.sort_by(|a, b| b.1.cmp(&a.1));
    let max_count = senders.first().map(|s| s.1).unwrap_or(1);
    for (addr, count) in senders.iter().take(args.top_n) {
        let size = sender_sizes.get(addr).copied().unwrap_or(0);
        print_bar(
            &format!("{} ({})", addr, format_size(size)),
            *count,
            max_count,
            20,
        );
    }
    println!();

    let (year_counts, no_date_count) = stats.year_counts();
    println!("──────────────────────────────────────────────────────────────────────");
    println!(" EMAILS BY YEAR");
    println!("──────────────────────────────────────────────────────────────────────");
    let mut years: Vec<(i32, usize)> = year_counts.into_iter().collect();
    years.sort_by_key(|(y, _)| *y);
    let max_year = years.iter().map(|(_, c)| *c).max().unwrap_or(1);
    for (year, count) in &years {
        print_bar(&year.to_string(), *count, max_year, 20);
    }
    if no_date_count > 0 {
        print_bar("(no date)", no_date_count, max_year, 20);
    }
    println!();

    println!("──────────────────────────────────────────────────────────────────────");
    println!(" SIZE DISTRIBUTION");
    println!("──────────────────────────────────────────────────────────────────────");
    let size_buckets = stats.size_buckets();
    let bucket_order = [
        "< 1 KB",
        "1–10 KB",
        "10–100 KB",
        "100 KB–1 MB",
        "1–10 MB",
        "> 10 MB",
    ];
    let max_bucket = size_buckets.values().copied().max().unwrap_or(1);
    for label in &bucket_order {
        print_bar(
            label,
            size_buckets.get(*label).copied().unwrap_or(0),
            max_bucket,
            20,
        );
    }
    println!();

    println!("──────────────────────────────────────────────────────────────────────");
    println!(" EMAILS BY FOLDER");
    println!("──────────────────────────────────────────────────────────────────────");
    let mut folders: Vec<(&String, usize)> = folder_counts.iter().map(|(k, &v)| (k, v)).collect();
    folders.sort_by(|a, b| b.1.cmp(&a.1));
    let max_folder = folders.first().map(|f| f.1).unwrap_or(1);
    let limit = if args.folders { folders.len() } else { 20 };
    for (folder, count) in folders.iter().take(limit) {
        let size = folder_sizes.get(*folder).copied().unwrap_or(0);
        print_bar(
            &format!("{} ({})", folder, format_size(size)),
            *count,
            max_folder,
            20,
        );
    }
    if !args.folders && folder_counts.len() > 20 {
        println!(
            "  ... ({} more folders, use --folders to show all)",
            folder_counts.len() - 20
        );
    }
    println!();
}
