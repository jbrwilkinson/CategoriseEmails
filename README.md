# Email Analyser

A command line utility to analyser folders containing internet email files
and show summary statistics about them.

## Example Output

```text
Scanning for .emlx files...
Found 254 .emlx files

══════════════════════════════════════════════════════════════════════
 AGGREGATE STATISTICS
══════════════════════════════════════════════════════════════════════

  Total emails parsed : 254
  Parse errors        : 0
  Total size on disk  : 7.6 MB
  Average size        : 30.7 KB
  Date range          : 2024-08-07 → 2026-05-02

──────────────────────────────────────────────────────────────────────
 TOP 10 SENDERS  (by message count)
──────────────────────────────────────────────────────────────────────
  emailsmetoomuch@gmail.com (417.9 KB)       ████████████████████    115
  mail@smileypeople.com (4.6 MB)             ████████░░░░░░░░░░░░     46
  noreply@nodispatch.com (343.9 KB)          █████░░░░░░░░░░░░░░░     30
  info@groupon.com (1.1 MB)                  ███░░░░░░░░░░░░░░░░░     19
  bookings@ritz-hotel.co.uk   (216.6 KB)     █░░░░░░░░░░░░░░░░░░░     11
  info@nbc.com (336.5 KB)                    █░░░░░░░░░░░░░░░░░░░      9
  gemini@accounts.google.com (85.8 KB)       █░░░░░░░░░░░░░░░░░░░      7
  noreply@email.fanclub.com (125.3 KB)       ░░░░░░░░░░░░░░░░░░░░      4
  top@banana.com (66.7 KB)                   ░░░░░░░░░░░░░░░░░░░░      4
  support@apple.com (40.3 KB)                ░░░░░░░░░░░░░░░░░░░░      2

──────────────────────────────────────────────────────────────────────
 EMAILS BY YEAR
──────────────────────────────────────────────────────────────────────
  2024                                       ████████████████████    112
  2025                                       ██████████░░░░░░░░░░     56
  2026                                       █████░░░░░░░░░░░░░░░     30
  (no date)                                  ██████████░░░░░░░░░░     56

──────────────────────────────────────────────────────────────────────
 SIZE DISTRIBUTION
──────────────────────────────────────────────────────────────────────
  < 1 KB                                     ░░░░░░░░░░░░░░░░░░░░      0
  1–10 KB                                    ████████████████████    117
  10–100 KB                                  █████████████████░░░    105
  100 KB–1 MB                                █████░░░░░░░░░░░░░░░     32
  1–10 MB                                    ░░░░░░░░░░░░░░░░░░░░      0
  > 10 MB                                    ░░░░░░░░░░░░░░░░░░░░      0
```

## Development

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Run

```bash
cargo run -- <folder>
```
