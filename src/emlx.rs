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
            let trimmed = remaining.trim_start_matches(|c: char| c == ' ' || c == '\t');
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
    if let Some(start) = s.rfind('<') {
        if let Some(end) = s[start..].find('>') {
            return s[start + 1..start + end].trim().to_lowercase();
        }
    }
    s.to_lowercase()
}

/// Parse an RFC 2822 date string into a typed `DateTime`.
pub fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    // Strip optional leading weekday: "Mon, 01 Jan …"
    let s = s.find(',').map(|p| s[p + 1..].trim()).unwrap_or(s);

    let formats = [
        "%d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M %z",
        "%e %b %Y %H:%M:%S %z",
        "%e %b %Y %H:%M %z",
    ];
    formats
        .iter()
        .find_map(|fmt| DateTime::parse_from_str(s, fmt).ok())
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
    fn tolerates_missing_date() {
        let raw = make_emlx("From: a@b.com\nSubject: No date", "");
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
}
