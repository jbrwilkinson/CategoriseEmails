mod emlx;
mod render;
mod report;

use clap::{Parser, ValueEnum};
use emlx::Email;
use render::{
    Renderer,
    console::{ConsoleRenderer, MIN_TOTAL_WIDTH},
    markdown::MarkdownRenderer,
};
use report::{ReportBuilder, SIZE_BUCKET_ORDER};
use std::collections::HashMap;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Clone, ValueEnum)]
enum Format {
    /// Human-readable console output with bar charts
    Console,
    /// Markdown document with tables
    Markdown,
}

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
    #[arg(short, long, conflicts_with = "progress")]
    verbose: bool,

    /// Show scanning and parsing progress on stderr
    #[arg(short, long, conflicts_with = "verbose")]
    progress: bool,

    /// Output format
    #[arg(long, value_enum, default_value = "console")]
    format: Format,

    /// Percentage of the terminal width to use for console output (5–100)
    #[arg(long, default_value = "40", value_parser = clap::value_parser!(u8).range(5..=100))]
    width: u8,
}

// ── Terminal width ────────────────────────────────────────────────────────────

/// Resolve the character width to use for output, given a percentage of the
/// current terminal width. Falls back to 80 columns when the terminal size
/// cannot be detected. The result is clamped to [`MIN_TOTAL_WIDTH`].
fn resolve_width(percent: u8) -> usize {
    use terminal_size::{Width, terminal_size};
    let terminal_cols = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);
    (terminal_cols * percent as usize / 100).max(MIN_TOTAL_WIDTH)
}

// ── Progress display ──────────────────────────────────────────────────────────

const PROGRESS_MIN_BAR: usize = 5;

/// Writes live scanning/parsing progress to stderr.
///
/// All output uses `\r` to overwrite the current line so the progress display
/// collapses to a single line in the scrollback once parsing is complete.
struct Progress {
    enabled: bool,
    total_width: usize,
    stderr: io::Stderr,
}

impl Progress {
    fn new(enabled: bool, total_width: usize) -> Self {
        Self {
            enabled,
            total_width,
            stderr: io::stderr(),
        }
    }

    /// Compute the bar width for a given `total` file count.
    ///
    /// The fixed parts of the progress line are:
    ///   `"  Parsing   ["` = 14, `"] "` = 2, `"100%  "` = 6, `"("` + `"/"` + `")"` = 3
    /// plus two copies of `total`'s digit count for `done` and `total`,
    /// plus 1 safety margin so the line never touches the terminal's right edge.
    fn bar_width_for(&self, total: usize) -> usize {
        let digits = total.to_string().len();
        let overhead = 14 + 2 + 6 + 3 + 2 * digits + 1;
        self.total_width
            .saturating_sub(overhead)
            .max(PROGRESS_MIN_BAR)
    }

    /// Print a folder name as scanning begins.
    fn scanning_folder(&mut self, name: &str) {
        if self.enabled {
            eprintln!("  Scanning  {}", name);
        }
    }

    /// Print the email count found in the most-recently-scanned folder.
    fn folder_found(&mut self, name: &str, count: usize) {
        if self.enabled {
            eprintln!("  Found     {} — {} email(s)", name, count);
        }
    }

    /// Overwrite the current line with a parsing progress bar.
    ///
    /// Call with `done == total` to mark completion (bar becomes full, line kept).
    fn parsing(&mut self, done: usize, total: usize) {
        if !self.enabled {
            return;
        }
        let width = self.bar_width_for(total);
        let filled = if total > 0 {
            done * width / total
        } else {
            width
        };
        let bar: String = std::iter::repeat('█')
            .take(filled)
            .chain(std::iter::repeat('░').take(width - filled))
            .collect();
        let pct = if total > 0 { done * 100 / total } else { 100 };
        // \r returns to column 0; no \n so the next call overwrites this line.
        // On completion we print \n to leave the finished bar in the scrollback.
        if done < total {
            write!(
                self.stderr,
                "\r  Parsing   [{}] {:>3}%  ({}/{})",
                bar, pct, done, total
            )
            .ok();
            self.stderr.flush().ok();
        } else {
            eprintln!("\r  Parsing   [{}] {:>3}%  ({}/{})", bar, pct, done, total);
        }
    }

    /// Print a blank separator line after the progress block.
    fn done(&mut self) {
        if self.enabled {
            eprintln!();
        }
    }
}

// ── File discovery ────────────────────────────────────────────────────────────

/// Scan a single directory tree for `.emlx` files.
fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
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
    files
}

/// Scan all directories, reporting per-folder progress when enabled.
fn find_emlx_files(dirs: &[PathBuf], progress: &mut Progress) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());

        progress.scanning_folder(&name);
        let found = scan_dir(dir);
        progress.folder_found(&name, found.len());
        all.extend(found);
    }
    all
}

// ── Aggregation ───────────────────────────────────────────────────────────────

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

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();
    let total_width = resolve_width(args.width);
    let mut progress = Progress::new(args.progress, total_width);

    let paths = find_emlx_files(&args.dirs, &mut progress);

    if args.progress {
        eprintln!("  Total     {} email(s) found", paths.len());
    }

    if paths.is_empty() {
        return;
    }

    progress.done();

    let total = paths.len();
    let mut emails: Vec<(PathBuf, Email)> = Vec::new();
    let mut parse_errors = 0usize;

    for (i, path) in paths.iter().enumerate() {
        progress.parsing(i, total);

        match emlx::parse_emlx(path) {
            Some(email) => {
                if args.verbose {
                    eprintln!(
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

    progress.parsing(total, total); // draw completed bar
    progress.done();

    let report = build_report(&emails, parse_errors, &args.dirs, args.top_n, args.folders);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match args.format {
        Format::Console => ConsoleRenderer { total_width }.render(&report, &mut out),
        Format::Markdown => MarkdownRenderer.render(&report, &mut out),
    }
}
