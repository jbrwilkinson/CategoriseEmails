use chrono::{DateTime, FixedOffset};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Email {
    pub from: String,
    pub subject: String,
    pub date: Option<DateTime<FixedOffset>>,
    pub size_bytes: u64,
}

/// Parse a single `.emlx` file, returning `None` if the file is malformed.
pub fn parse_emlx(path: &Path) -> Option<Email> {
    let raw = std::fs::read(path).ok()?;
    let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
    parse_emlx_bytes(&raw, size_bytes)
}

/// Parse EMLX content from a byte slice. Separated from I/O for testing.
pub fn parse_emlx_bytes(raw: &[u8], size_bytes: u64) -> Option<Email> {
    // EMLX format:
    //   Line 1: decimal byte count\n
    //   Next N bytes: raw RFC 2822 email
    //   Remainder: Apple plist XML (ignored)
    let newline_pos = raw.iter().position(|&b| b == b'\n')?;
    let byte_count: usize = std::str::from_utf8(&raw[..newline_pos])
        .ok()?
        .trim()
        .parse()
        .ok()?;

    let email_start = newline_pos + 1;
    let email_end = (email_start + byte_count).min(raw.len());
    let email_bytes = &raw[email_start..email_end];

    let headers_end = find_headers_end(email_bytes);
    let header_bytes = &email_bytes[..headers_end];

    let header_str = match std::str::from_utf8(header_bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            let (s, _, _) = encoding_rs::WINDOWS_1252.decode(header_bytes);
            s.into_owned()
        }
    };

    let unfolded = unfold_headers(&header_str);

    let mut from = String::new();
    let mut subject = String::new();
    let mut date_str = String::new();

    for line in unfolded.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("from:") && from.is_empty() {
            from = decode_header_value(&line["from:".len()..]);
        } else if lower.starts_with("subject:") && subject.is_empty() {
            subject = decode_header_value(&line["subject:".len()..]);
        } else if lower.starts_with("date:") && date_str.is_empty() {
            date_str = line["date:".len()..].trim().to_string();
        }
    }

    Some(Email {
        from: extract_email_address(&from),
        subject,
        date: parse_date(&date_str),
        size_bytes,
    })
}

// ── Header utilities ──────────────────────────────────────────────────────────

/// Decode an RFC 2047 encoded header value.
pub fn decode_header_value(raw: &str) -> String {
    let raw = raw.trim();
    if !raw.contains("=?") {
        return raw.to_string();
    }

    let mut result = String::new();
    let mut remaining = raw;

    while let Some(start) = remaining.find("=?") {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];

        if let Some(end) = remaining.find("?=") {
            let decoded = decode_encoded_word(&remaining[..end + 2]);
            result.push_str(&decoded);
            remaining = &remaining[end + 2..];
            // RFC 2047: whitespace between adjacent encoded words is ignored,
            // but whitespace before plain text must be preserved.
            let trimmed = remaining.trim_start_matches([' ', '\t']);
            if trimmed.starts_with("=?") {
                remaining = trimmed;
            }
        } else {
            result.push_str(remaining);
            remaining = "";
        }
    }
    result.push_str(remaining);
    result
}

/// Decode a single RFC 2047 encoded word: `=?charset?encoding?text?=`.
fn decode_encoded_word(word: &str) -> String {
    let inner = word.trim_start_matches("=?").trim_end_matches("?=");
    let parts: Vec<&str> = inner.splitn(3, '?').collect();
    if parts.len() != 3 {
        return word.to_string();
    }
    let (charset, encoding_char, text) = (parts[0], parts[1].to_ascii_uppercase(), parts[2]);

    let raw_bytes = match encoding_char.as_str() {
        "B" => match base64_decode(text) {
            Ok(b) => b,
            Err(_) => return word.to_string(),
        },
        "Q" => quoted_printable_decode_header(text),
        _ => return word.to_string(),
    };

    let encoding =
        encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(&raw_bytes);
    text.into_owned()
}

fn base64_decode(s: &str) -> Result<Vec<u8>, ()> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [0u8; 256];
    for (i, &b) in alphabet.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }

    let s: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < s.len() {
        let n = (lookup[s[i] as usize] as u32) << 18
            | (lookup[s[i + 1] as usize] as u32) << 12
            | (lookup[s[i + 2] as usize] as u32) << 6
            | (lookup[s[i + 3] as usize] as u32);
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
        i += 4;
    }
    if i + 2 < s.len() {
        let n = (lookup[s[i] as usize] as u32) << 18
            | (lookup[s[i + 1] as usize] as u32) << 12
            | (lookup[s[i + 2] as usize] as u32) << 6;
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8]);
    } else if i + 1 < s.len() {
        let n = (lookup[s[i] as usize] as u32) << 18 | (lookup[s[i + 1] as usize] as u32) << 12;
        out.push((n >> 16) as u8);
    }
    Ok(out)
}

fn quoted_printable_decode_header(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'=' && i + 2 < bytes.len() {
            match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Extract a bare email address from a `Name <address>` or plain `address` string.
pub fn extract_email_address(s: &str) -> String {
    let s = s.trim();
    if let Some(start) = s.rfind('<')
        && let Some(end) = s[start..].find('>')
    {
        return s[start + 1..start + end].trim().to_lowercase();
    }
    s.to_lowercase()
}

/// Parse an RFC 2822 date string into a typed `DateTime`.
pub fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    // Strip optional leading weekday: "Mon, 01 Jan …"
    let s = s.find(',').map(|p| s[p + 1..].trim()).unwrap_or(s);
    // Remove RFC 2822 parenthesised comments, e.g. "+0100 (IST)" → "+0100".
    let uncommented = strip_comments(s);
    let s = uncommented.trim();
    // Expand any 2-digit year before parsing.
    let expanded = expand_two_digit_year(s);
    // Normalise non-standard timezone tokens (e.g. "GMT-0700" → "-0700").
    let normalised = normalize_timezone(&expanded);
    let s = normalised.as_str();

    let formats = [
        "%d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M %z",
        "%e %b %Y %H:%M:%S %z",
        "%e %b %Y %H:%M %z",
        // ISO 8601 dates occasionally appear in older mailers.
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%d %H:%M %z",
    ];
    formats
        .iter()
        .find_map(|fmt| DateTime::parse_from_str(s, fmt).ok())
}

/// If the date string has a 2-digit year (the third whitespace-delimited token),
/// expand it: years 0–49 become 2000–2049, years 50–99 become 1950–1999.
/// Returns the original string unchanged if no 2-digit year is detected.
fn expand_two_digit_year(s: &str) -> String {
    // After weekday stripping the format is "DD Mon YY HH:MM:SS +ZZZZ".
    // Split into at most 4 parts so the time+zone remainder stays intact.
    let parts: Vec<&str> = s.splitn(4, ' ').collect();
    if parts.len() >= 3 {
        let year_str = parts[2];
        if year_str.len() == 2
            && let Ok(y) = year_str.parse::<u32>()
        {
            let full_year = if y < 50 { 2000 + y } else { 1900 + y };
            return match parts.get(3) {
                Some(rest) => format!("{} {} {} {}", parts[0], parts[1], full_year, rest),
                None => format!("{} {} {}", parts[0], parts[1], full_year),
            };
        }
    }
    s.to_string()
}

/// Remove RFC 2822 parenthesised comments from a date string.
///
/// Comments may appear anywhere but in practice always trail the timezone:
/// `"06 May 2008 12:43:31 +0100 (IST)"` → `"06 May 2008 12:43:31 +0100"`.
/// Nested comments (`(foo (bar))`) are handled correctly.
///
/// **Exception:** when a top-level comment's trimmed content is itself a
/// timezone token (e.g. `(GMT)` or `(+0100)`), it is promoted into the
/// output so that `normalize_timezone` can convert it to a numeric offset.
/// This handles dates like `"15 Jul 2007 14:52:26 (GMT)"` where the timezone
/// is wrapped in parentheses rather than written bare.
fn strip_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut depth = 0usize;
    let mut comment_buf = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                if depth == 0 {
                    comment_buf.clear();
                }
                depth += 1;
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    // Promote the comment content only if it is a bare timezone
                    // token AND the text already written doesn't end with one.
                    // This prevents "+0000 (GMT)" from becoming "+0000 GMT".
                    let token = comment_buf.trim();
                    if is_timezone_token(token) {
                        let last = result.split_whitespace().next_back().unwrap_or("");
                        if !is_timezone_token(last) {
                            result.push(' ');
                            result.push_str(token);
                        }
                    }
                    comment_buf.clear();
                }
            }
            ')' => {} // unmatched — ignore
            _ if depth > 0 => comment_buf.push(ch),
            _ => result.push(ch),
        }
    }
    result
}

/// Return `true` if `token` is a timezone value that `normalize_timezone` can
/// handle: a named zone, a bare four-digit offset, a signed numeric offset, or
/// an alpha-prefixed numeric offset such as `GMT-0700`.
fn is_timezone_token(token: &str) -> bool {
    // Named zones
    if matches!(
        token,
        "GMT" | "UT" | "UTC" | "EST" | "EDT" | "CST" | "CDT" | "MST" | "MDT" | "PST" | "PDT"
    ) {
        return true;
    }
    // Signed numeric offset: ±HHMM
    if token.len() == 5 {
        let b = token.as_bytes();
        if (b[0] == b'+' || b[0] == b'-') && token[1..].bytes().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    // Bare four-digit offset: HHMM
    if token.len() == 4 && token.bytes().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Alpha-prefixed offset: e.g. GMT-0700
    if let Some(sign_pos) = token.find(['+', '-'])
        && sign_pos > 0
    {
        let prefix = &token[..sign_pos];
        let offset = &token[sign_pos..];
        if prefix.chars().all(|c| c.is_ascii_alphabetic())
            && offset.len() == 5
            && offset[1..].bytes().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Normalise non-standard timezone tokens so that chrono's `%z` can parse them.
///
/// Two cases are handled:
///
/// * **Prefixed numeric offset** – e.g. `GMT-0700` or `UTC+0530`.  The leading
///   alpha prefix is dropped, leaving only the sign + four digits (`-0700`).
/// * **Standalone named zone** – RFC 2822 obsolete zones such as `GMT`, `EST`,
///   `PST`, etc. are mapped to their fixed numeric offsets.
///
/// Anything that does not match either pattern is returned unchanged.
fn normalize_timezone(s: &str) -> String {
    let s = s.trim_end();

    // Locate the last whitespace-delimited token.
    let (before, token) = match s.rfind(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&s[..pos], s[pos + 1..].trim()),
        None => ("", s),
    };

    let replacement: Option<&str> = 'find: {
        // Case 1: alpha prefix immediately before ±HHMM, e.g. "GMT-0700".
        if let Some(sign_pos) = token.find(['+', '-'])
            && sign_pos > 0
        {
            let prefix = &token[..sign_pos];
            let offset = &token[sign_pos..];
            if prefix.chars().all(|c| c.is_ascii_alphabetic())
                && offset.len() == 5
                && offset[1..].chars().all(|c| c.is_ascii_digit())
            {
                break 'find Some(offset);
            }
        }

        // Case 2: bare unsigned offset, e.g. "0000" or "0530" — prepend '+'.
        if token.len() == 4 && token.chars().all(|c| c.is_ascii_digit()) {
            let owned = format!("+{}", token);
            return match before {
                "" => owned,
                _ => format!("{} {}", before, owned),
            };
        }

        // Case 3: standalone named zone.
        break 'find match token {
            "GMT" | "UT" | "UTC" => Some("+0000"),
            "EST" => Some("-0500"),
            "EDT" => Some("-0400"),
            "CST" => Some("-0600"),
            "CDT" => Some("-0500"),
            "MST" => Some("-0700"),
            "MDT" => Some("-0600"),
            "PST" => Some("-0800"),
            "PDT" => Some("-0700"),
            _ => None,
        };
    };

    match replacement {
        Some(tz) if before.is_empty() => tz.to_string(),
        Some(tz) => format!("{} {}", before, tz),
        None => s.to_string(),
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn find_headers_end(bytes: &[u8]) -> usize {
    for i in 0..bytes.len() {
        if i + 3 < bytes.len()
            && bytes[i] == b'\r'
            && bytes[i + 1] == b'\n'
            && bytes[i + 2] == b'\r'
            && bytes[i + 3] == b'\n'
        {
            return i;
        }
        if i + 1 < bytes.len() && bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            return i;
        }
    }
    bytes.len()
}

fn unfold_headers(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
            if chars
                .peek()
                .map(|c| *c == ' ' || *c == '\t')
                .unwrap_or(false)
            {
                result.push(' ');
                chars.next();
            } else {
                result.push('\n');
            }
        } else if c == '\n' {
            if chars
                .peek()
                .map(|c| *c == ' ' || *c == '\t')
                .unwrap_or(false)
            {
                result.push(' ');
                chars.next();
            } else {
                result.push('\n');
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_emlx(headers: &str, body: &str) -> Vec<u8> {
        let email = format!("{}\n\n{}", headers, body);
        format!("{}\n{}", email.len(), email).into_bytes()
    }

    // ── parse_emlx_bytes ─────────────────────────────────────────────────────

    #[test]
    fn parses_basic_headers() {
        let raw = make_emlx(
            "From: alice@example.com\nSubject: Hello\nDate: Mon, 01 Jan 2024 10:00:00 +0000",
            "body text",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(email.from, "alice@example.com");
        assert_eq!(email.subject, "Hello");
        assert!(email.date.is_some());
    }

    #[test]
    fn extracts_address_from_display_name() {
        let raw = make_emlx(
            "From: Alice Smith <alice@example.com>\nSubject: Hi\nDate: Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(email.from, "alice@example.com");
    }

    #[test]
    fn lowercases_email_address() {
        let raw = make_emlx("From: Alice@Example.COM\nSubject: test", "");
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(email.from, "alice@example.com");
    }

    #[test]
    fn returns_none_for_missing_byte_count() {
        let result = parse_emlx_bytes(b"not-a-number\nFrom: x", 0);
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_for_empty_input() {
        assert!(parse_emlx_bytes(b"", 0).is_none());
    }

    #[test]
    fn tolerates_missing_date_header() {
        let raw = make_emlx("From: a@b.com\nSubject: No date", "");
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert!(email.date.is_none());
    }

    #[test]
    fn tolerates_unparseable_date_header() {
        let raw = make_emlx("From: a@b.com\nSubject: x\nDate: not a date at all", "");
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert!(email.date.is_none());
    }

    #[test]
    fn uses_provided_size_bytes() {
        let raw = make_emlx("From: a@b.com\nSubject: x", "");
        let email = parse_emlx_bytes(&raw, 1234).unwrap();
        assert_eq!(email.size_bytes, 1234);
    }

    #[test]
    fn ignores_apple_plist_after_email() {
        let email_part = "From: a@b.com\nSubject: x\n\nbody";
        let plist = "<?xml version=\"1.0\"?><plist></plist>";
        let raw = format!("{}\n{}{}", email_part.len(), email_part, plist).into_bytes();
        let result = parse_emlx_bytes(&raw, 0);
        assert!(result.is_some());
    }

    // ── decode_header_value ───────────────────────────────────────────────────

    #[test]
    fn plain_header_unchanged() {
        assert_eq!(decode_header_value("Hello World"), "Hello World");
    }

    #[test]
    fn decodes_utf8_base64_encoded_word() {
        // "Weekly Newsletter" base64-encoded as UTF-8
        assert_eq!(
            decode_header_value("=?UTF-8?B?V2Vla2x5IE5ld3NsZXR0ZXI=?="),
            "Weekly Newsletter"
        );
    }

    #[test]
    fn decodes_quoted_printable_encoded_word() {
        // "Héllo" in ISO-8859-1 QP
        assert_eq!(decode_header_value("=?ISO-8859-1?Q?H=E9llo?="), "Héllo");
    }

    #[test]
    fn decodes_qp_underscore_as_space() {
        assert_eq!(
            decode_header_value("=?UTF-8?Q?Hello_World?="),
            "Hello World"
        );
    }

    #[test]
    fn decodes_multiple_encoded_words() {
        // Two consecutive encoded words (whitespace between them should be collapsed)
        let input = "=?UTF-8?B?SGVsbG8=?= =?UTF-8?B?V29ybGQ=?=";
        assert_eq!(decode_header_value(input), "HelloWorld");
    }

    #[test]
    fn handles_mixed_plain_and_encoded() {
        let input = "prefix =?UTF-8?B?SGVsbG8=?= suffix";
        assert_eq!(decode_header_value(input), "prefix Hello suffix");
    }

    // ── extract_email_address ─────────────────────────────────────────────────

    #[test]
    fn extracts_angle_bracket_address() {
        assert_eq!(
            extract_email_address("Bob Smith <bob@example.com>"),
            "bob@example.com"
        );
    }

    #[test]
    fn plain_address_returned_as_is() {
        assert_eq!(extract_email_address("bob@example.com"), "bob@example.com");
    }

    #[test]
    fn trims_whitespace_from_address() {
        assert_eq!(
            extract_email_address("  bob@example.com  "),
            "bob@example.com"
        );
    }

    // ── parse_date ────────────────────────────────────────────────────────────

    #[test]
    fn parses_date_with_weekday() {
        let dt = parse_date("Mon, 01 Jan 2024 10:00:00 +0000");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-01");
    }

    #[test]
    fn parses_date_without_weekday() {
        let dt = parse_date("15 Feb 2024 14:30:00 +0100");
        assert!(dt.is_some());
    }

    #[test]
    fn returns_none_for_invalid_date() {
        assert!(parse_date("not a date").is_none());
    }

    #[test]
    fn returns_none_for_empty_date() {
        assert!(parse_date("").is_none());
    }

    // ── expand_two_digit_year ─────────────────────────────────────────────────

    #[test]
    fn two_digit_year_under_50_maps_to_2000s() {
        assert_eq!(
            expand_two_digit_year("01 Jan 24 10:00:00 +0000"),
            "01 Jan 2024 10:00:00 +0000"
        );
    }

    #[test]
    fn two_digit_year_zero_maps_to_2000() {
        assert_eq!(
            expand_two_digit_year("01 Jan 00 10:00:00 +0000"),
            "01 Jan 2000 10:00:00 +0000"
        );
    }

    #[test]
    fn two_digit_year_49_maps_to_2049() {
        assert_eq!(
            expand_two_digit_year("01 Jan 49 10:00:00 +0000"),
            "01 Jan 2049 10:00:00 +0000"
        );
    }

    #[test]
    fn two_digit_year_50_maps_to_1950() {
        assert_eq!(
            expand_two_digit_year("01 Jan 50 10:00:00 +0000"),
            "01 Jan 1950 10:00:00 +0000"
        );
    }

    #[test]
    fn two_digit_year_99_maps_to_1999() {
        assert_eq!(
            expand_two_digit_year("01 Jan 99 10:00:00 +0000"),
            "01 Jan 1999 10:00:00 +0000"
        );
    }

    #[test]
    fn four_digit_year_is_unchanged() {
        let s = "01 Jan 2024 10:00:00 +0000";
        assert_eq!(expand_two_digit_year(s), s);
    }

    #[test]
    fn non_numeric_year_token_is_unchanged() {
        let s = "01 Jan abc 10:00:00 +0000";
        assert_eq!(expand_two_digit_year(s), s);
    }

    // ── parse_date with 2-digit years ─────────────────────────────────────────

    #[test]
    fn parses_two_digit_year_under_50_with_weekday() {
        let dt = parse_date("Mon, 01 Jan 24 10:00:00 +0000").unwrap();
        assert_eq!(dt.format("%Y").to_string(), "2024");
    }

    #[test]
    fn parses_two_digit_year_50_or_over_with_weekday() {
        let dt = parse_date("Fri, 15 Jun 96 08:30:00 +0100").unwrap();
        assert_eq!(dt.format("%Y").to_string(), "1996");
    }

    #[test]
    fn parses_two_digit_year_without_weekday() {
        let dt = parse_date("15 Mar 03 12:00:00 +0000").unwrap();
        assert_eq!(dt.format("%Y").to_string(), "2003");
    }

    // ── strip_comments ────────────────────────────────────────────────────────

    #[test]
    fn trailing_non_timezone_comment_removed() {
        // "IST" is not in the named-zone list, so it is dropped
        assert_eq!(
            strip_comments("06 May 2008 12:43:31 +0100 (IST)"),
            "06 May 2008 12:43:31 +0100 "
        );
    }

    #[test]
    fn nested_comment_removed() {
        assert_eq!(
            strip_comments("01 Jan 2024 00:00:00 +0000 (foo (bar))"),
            "01 Jan 2024 00:00:00 +0000 "
        );
    }

    #[test]
    fn no_comment_unchanged() {
        let s = "01 Jan 2024 00:00:00 +0000";
        assert_eq!(strip_comments(s), s);
    }

    #[test]
    fn unmatched_close_paren_ignored() {
        // Malformed but should not panic
        assert_eq!(
            strip_comments("01 Jan 2024 +0000)extra"),
            "01 Jan 2024 +0000extra"
        );
    }

    #[test]
    fn parenthesised_named_timezone_is_promoted_when_no_offset_present() {
        // (GMT) is the sole timezone — it should be kept, not dropped
        assert_eq!(
            strip_comments("15 Jul 2007 14:52:26 (GMT)"),
            "15 Jul 2007 14:52:26  GMT"
        );
    }

    #[test]
    fn parenthesised_signed_offset_is_promoted_when_no_offset_present() {
        assert_eq!(
            strip_comments("15 Jul 2007 14:52:26 (+0100)"),
            "15 Jul 2007 14:52:26  +0100"
        );
    }

    #[test]
    fn parenthesised_timezone_not_promoted_when_offset_already_present() {
        // "+0000 (GMT)" — the +0000 is already a valid timezone; (GMT) is dropped
        assert_eq!(
            strip_comments("27 Mar 2008 22:51:33 +0000 (GMT)"),
            "27 Mar 2008 22:51:33 +0000 "
        );
    }

    // ── parse_date with parenthesised comments ────────────────────────────────

    #[test]
    fn parses_date_with_trailing_timezone_comment() {
        let dt = parse_date("Tue, 06 May 2008 12:43:31 +0100 (IST)").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-05-06");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "12:43:31");
    }

    #[test]
    fn parses_date_with_nested_comment() {
        let dt = parse_date("Mon, 01 Jan 2024 00:00:00 +0000 (UTC (Universal))").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-01");
    }

    #[test]
    fn parses_date_with_timezone_in_parens() {
        // Timezone is the parenthesised comment, no bare offset present
        let dt = parse_date("Sun, 15 Jul 2007 14:52:26 (GMT)").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2007-07-15");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "14:52:26");
    }

    #[test]
    fn parses_date_with_numeric_offset_in_parens() {
        let dt = parse_date("01 Jan 2024 10:00:00 (+0530)").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-01");
    }

    #[test]
    fn parses_date_with_offset_followed_by_parenthesised_zone_name() {
        // The reported failing case: +0000 is the real offset, (GMT) is a gloss
        let dt = parse_date("Thu, 27 Mar 2008 22:51:33 +0000 (GMT)").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-03-27");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "22:51:33");
    }

    // ── normalize_timezone ────────────────────────────────────────────────────

    #[test]
    fn gmt_minus_offset_stripped_to_numeric() {
        assert_eq!(
            normalize_timezone("22 May 2008 09:17:10 GMT-0700"),
            "22 May 2008 09:17:10 -0700"
        );
    }

    #[test]
    fn gmt_plus_offset_stripped_to_numeric() {
        assert_eq!(
            normalize_timezone("01 Jan 2024 00:00:00 GMT+0000"),
            "01 Jan 2024 00:00:00 +0000"
        );
    }

    #[test]
    fn utc_plus_offset_stripped_to_numeric() {
        assert_eq!(
            normalize_timezone("01 Jan 2024 12:00:00 UTC+0530"),
            "01 Jan 2024 12:00:00 +0530"
        );
    }

    #[test]
    fn standalone_gmt_mapped_to_zero_offset() {
        assert_eq!(
            normalize_timezone("01 Jan 2024 10:00:00 GMT"),
            "01 Jan 2024 10:00:00 +0000"
        );
    }

    #[test]
    fn standalone_est_mapped_to_minus_five() {
        assert_eq!(
            normalize_timezone("01 Jan 2024 10:00:00 EST"),
            "01 Jan 2024 10:00:00 -0500"
        );
    }

    #[test]
    fn standalone_pst_mapped_to_minus_eight() {
        assert_eq!(
            normalize_timezone("01 Jan 2024 10:00:00 PST"),
            "01 Jan 2024 10:00:00 -0800"
        );
    }

    #[test]
    fn bare_unsigned_offset_gets_plus_sign() {
        assert_eq!(
            normalize_timezone("11 Jan 2008 14:02:49 0000"),
            "11 Jan 2008 14:02:49 +0000"
        );
    }

    #[test]
    fn bare_nonzero_unsigned_offset_gets_plus_sign() {
        assert_eq!(
            normalize_timezone("11 Jan 2008 14:02:49 0530"),
            "11 Jan 2008 14:02:49 +0530"
        );
    }

    #[test]
    fn numeric_offset_unchanged() {
        let s = "01 Jan 2024 10:00:00 -0700";
        assert_eq!(normalize_timezone(s), s);
    }

    #[test]
    fn unknown_named_zone_unchanged() {
        let s = "01 Jan 2024 10:00:00 XYZ";
        assert_eq!(normalize_timezone(s), s);
    }

    // ── parse_date with non-standard timezones ────────────────────────────────

    #[test]
    fn parses_gmt_minus_offset() {
        // The reported failing case
        let dt = parse_date("Thu, 22 May 08 09:17:10 GMT-0700").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-05-22");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "09:17:10");
    }

    #[test]
    fn parses_standalone_gmt() {
        let dt = parse_date("Mon, 01 Jan 2024 10:00:00 GMT").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-01");
    }

    #[test]
    fn parses_standalone_pst() {
        let dt = parse_date("15 Jun 2023 08:30:00 PST").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2023-06-15");
    }

    #[test]
    fn parses_iso8601_date() {
        // Some older mailers emit YYYY-MM-DD instead of DD Mon YYYY.
        let dt = parse_date("2008-04-07 21:54:27 +0100").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-04-07");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "21:54:27");
    }

    #[test]
    fn parses_iso8601_date_no_seconds() {
        let dt = parse_date("2008-04-07 21:54 +0100").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-04-07");
    }

    #[test]
    fn parses_bare_unsigned_offset() {
        // The reported failing case
        let dt = parse_date("Fri, 11 Jan 2008 14:02:49 0000").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2008-01-11");
        assert_eq!(dt.format("%H:%M:%S").to_string(), "14:02:49");
    }

    // ── case-insensitive header names ────────────────────────────────────────

    #[test]
    fn lowercase_date_header_is_parsed() {
        let raw = make_emlx(
            "From: a@b.com\nSubject: x\ndate: Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(
            email.date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-01"
        );
    }

    #[test]
    fn uppercase_date_header_is_parsed() {
        let raw = make_emlx(
            "From: a@b.com\nSubject: x\nDATE: Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert!(email.date.is_some());
    }

    #[test]
    fn mixed_case_from_and_subject_headers_are_parsed() {
        let raw = make_emlx(
            "FROM: alice@example.com\nSUBJECT: Hello\nDate: Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(email.from, "alice@example.com");
        assert_eq!(email.subject, "Hello");
    }

    // ── header folding ────────────────────────────────────────────────────────

    #[test]
    fn unfolded_subject_parsed_correctly() {
        // A long subject folded across two lines
        let raw = make_emlx(
            "From: a@b.com\nSubject: This is a very\n long subject\nDate: Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert_eq!(email.subject, "This is a very long subject");
    }

    // ── parse_emlx_bytes date tolerance ──────────────────────────────────────

    #[test]
    fn empty_date_header_yields_none_date() {
        // "Date: " with no value — email parses but date is None
        let raw = make_emlx("From: a@b.com\nSubject: x\nDate: ", "");
        let email = parse_emlx_bytes(&raw, 0).unwrap();
        assert!(email.date.is_none());
    }
}
