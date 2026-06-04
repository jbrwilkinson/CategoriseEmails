mod emlx;
mod report;

use clap::Parser;
use emlx::Email;
use report::{ReportBuilder, SIZE_BUCKET_ORDER};
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

/// Return the command-line root directory name that contains `path`.
fn folder_label(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if path.starts_with(root) {
            return root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());
        }
    }
    "(unknown)".to_string()
}

fn build_report(
    emails: &[(PathBuf, Email)],
    parse_errors: usize,
    dirs: &[PathBuf],
    top_n: usize,
    show_all_folders: bool,
) -> report::Report {
    let total_emails = emails.len();
    let total_size_bytes: u64 = emails.iter().map(|(_, e)| e.size_bytes).sum();
    let avg_size_bytes = if total_emails > 0 {
        total_size_bytes / total_emails as u64
    } else {
        0
    };

    let mut sender_counts: HashMap<String, usize> = HashMap::new();
    let mut sender_sizes: HashMap<String, u64> = HashMap::new();
    let mut year_counts: HashMap<i32, usize> = HashMap::new();
    let mut emails_without_date = 0usize;
    let mut size_bucket_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut folder_counts: HashMap<String, usize> = HashMap::new();
    let mut folder_sizes: HashMap<String, u64> = HashMap::new();

    let mut dated_timestamps: Vec<chrono::DateTime<chrono::FixedOffset>> = Vec::new();

    for (path, email) in emails {
        *sender_counts.entry(email.from.clone()).or_default() += 1;
        *sender_sizes.entry(email.from.clone()).or_default() += email.size_bytes;

        match email.date {
            Some(d) => {
                let year = d.format("%Y").to_string().parse::<i32>().unwrap_or(0);
                *year_counts.entry(year).or_default() += 1;
                dated_timestamps.push(d);
            }
            None => emails_without_date += 1,
        }

        let bucket = match email.size_bytes {
            0..=1_023 => SIZE_BUCKET_ORDER[0],
            1_024..=9_999 => SIZE_BUCKET_ORDER[1],
            10_000..=99_999 => SIZE_BUCKET_ORDER[2],
            100_000..=999_999 => SIZE_BUCKET_ORDER[3],
            1_000_000..=9_999_999 => SIZE_BUCKET_ORDER[4],
            _ => SIZE_BUCKET_ORDER[5],
        };
        *size_bucket_counts.entry(bucket).or_default() += 1;

        let folder = folder_label(path, dirs);
        *folder_counts.entry(folder.clone()).or_default() += 1;
        *folder_sizes.entry(folder).or_default() += email.size_bytes;
    }

    dated_timestamps.sort();
    let date_range = match (dated_timestamps.first(), dated_timestamps.last()) {
        (Some(earliest), Some(latest)) => Some((
            earliest.format("%Y-%m-%d").to_string(),
            latest.format("%Y-%m-%d").to_string(),
        )),
        _ => None,
    };

    ReportBuilder {
        total_emails,
        parse_errors,
        total_size_bytes,
        avg_size_bytes,
        date_range,
        sender_counts,
        sender_sizes,
        year_counts,
        emails_without_date,
        size_bucket_counts,
        folder_counts,
        folder_sizes,
        top_senders_limit: top_n,
        show_all_folders,
        include_folders: dirs.len() > 1,
    }
    .build()
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

    let report = build_report(&emails, parse_errors, &args.dirs, args.top_n, args.folders);
    report.print();
}
