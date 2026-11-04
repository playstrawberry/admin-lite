//! Checks and clean-up for text that comes from outside: player names, the reasons staff type,
//! the fields of the data files, UUIDs.
//!
//! A reason is shown in chat, in the history, in the menus and in the audit log, so anything in
//! it that looks like an IP address is replaced by `[IP]`: staff sometimes paste one, and no IP
//! appears in chat, a log or a menu.

/// The longest reason kept, in characters.
pub const MAX_REASON: usize = 200;

/// A Minecraft name: 1 to 16 letters, digits or `_`.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Characters that change how text around them is drawn without being visible (zero-width
/// spaces, direction overrides, the byte-order mark): removed from reasons.
fn invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// The reason as it is kept: control characters removed, spaces collapsed, IP addresses
/// replaced by `[IP]`, at most [`MAX_REASON`] characters.
#[must_use]
pub fn clean_reason(reason: &str) -> String {
    let spaced: String = reason
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                Some(' ')
            } else if c.is_control() || invisible(c) {
                None
            } else {
                Some(c)
            }
        })
        .collect();
    let collapsed = spaced
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    mask_ips(&collapsed).chars().take(MAX_REASON).collect()
}

fn ip_char(c: char) -> bool {
    c.is_ascii_hexdigit() || c == '.' || c == ':'
}

/// Replaces every run of hexadecimal digits, dots and colons that reads as an IP address.
fn mask_ips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let core = run.trim_end_matches(['.', ':']);
        if is_ipv4(core) || is_ipv6(core) {
            out.push_str("[IP]");
            out.push_str(&run[core.len()..]);
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if ip_char(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// `a.b.c.d` with parts from 0 to 255, with or without a `:port`.
fn is_ipv4(text: &str) -> bool {
    let host = match text.split_once(':') {
        Some((host, port)) => {
            if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            host
        }
        None => text,
    };
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            (1..=3).contains(&p.len())
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u16>().is_ok_and(|v| v <= 255)
        })
}

/// Hexadecimal groups of at most four digits between colons, with a `::` or a letter so that a
/// time such as `12:30:45` does not count.
fn is_ipv6(text: &str) -> bool {
    if text.matches(':').count() < 2 || text.contains(":::") || text.matches("::").count() > 1 {
        return false;
    }
    let groups: Vec<&str> = text.split(':').collect();
    let (last, rest) = groups.split_last().unwrap_or((&"", &[]));
    let hex_group = |g: &&str| g.len() <= 4 && g.bytes().all(|b| b.is_ascii_hexdigit());
    let last_ok = hex_group(last) || is_ipv4(last);
    let has_letter = text.bytes().any(|b| b.is_ascii_alphabetic());
    rest.iter().all(hex_group)
        && last_ok
        && groups.len() <= 8
        && (text.contains("::") || has_letter || groups.len() == 8)
}

/// A field for a tab-separated line: `\`, tab, newline and carriage return escaped.
#[must_use]
pub fn escape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    for c in field.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

/// The field [`escape`] wrote.
#[must_use]
pub fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A UUID written as 36 characters (`8-4-4-4-12` hexadecimal digits), as `(high, low)`.
#[must_use]
pub fn parse_uuid(text: &str) -> Option<(u64, u64)> {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    let mut digits = String::with_capacity(32);
    for (i, b) in bytes.iter().enumerate() {
        if matches!(i, 8 | 13 | 18 | 23) {
            if *b != b'-' {
                return None;
            }
        } else if b.is_ascii_hexdigit() {
            digits.push(char::from(*b));
        } else {
            return None;
        }
    }
    let value = u128::from_str_radix(&digits, 16).ok()?;
    Some(((value >> 64) as u64, value as u64))
}

/// `(high, low)` as 36 lowercase characters.
#[must_use]
pub fn format_uuid(high: u64, low: u64) -> String {
    let hex = format!("{high:016x}{low:016x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_minecraft_names() {
        assert!(valid_name("Steve"));
        assert!(valid_name("a_b_1"));
        assert!(valid_name("abcdefghijklmnop"));
        for bad in [
            "",
            "abcdefghijklmnopq",
            "with space",
            "ünal",
            "a-b",
            "@a",
            "a\tb",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn reasons_lose_control_characters_and_extra_spaces() {
        assert_eq!(clean_reason("  spam \n\n in  chat\t"), "spam in chat");
        assert_eq!(clean_reason("a\u{0}b\u{1b}[31mc"), "ab[31mc");
        let long: String = "é".repeat(300);
        assert_eq!(clean_reason(&long).chars().count(), MAX_REASON);
    }

    #[test]
    fn ip_addresses_in_reasons_are_hidden() {
        assert_eq!(
            clean_reason("alt from 192.168.1.20 again"),
            "alt from [IP] again"
        );
        assert_eq!(clean_reason("ip=10.0.0.1:25565,"), "ip=[IP],");
        assert_eq!(clean_reason("v6 2001:db8::1 and ::1"), "v6 [IP] and [IP]");
        assert_eq!(clean_reason("(8.8.8.8)"), "([IP])");
        // Not addresses: a version, a time, a number with too large parts, plain words.
        assert_eq!(
            clean_reason("on 1.21.4 at 12:30:45"),
            "on 1.21.4 at 12:30:45"
        );
        assert_eq!(clean_reason("999.1.1.1 cafe face"), "999.1.1.1 cafe face");
    }

    #[test]
    fn fields_survive_escaping() {
        for field in [
            "plain",
            "tab\there",
            "line\nbreak\r",
            "back\\slash\\t",
            "",
            "\\",
        ] {
            let escaped = escape(field);
            assert!(!escaped.contains(['\t', '\n', '\r']), "{escaped:?}");
            assert_eq!(unescape(&escaped), field);
        }
    }

    #[test]
    fn uuids_round_trip_and_bad_ones_are_refused() {
        let text = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
        let (high, low) = parse_uuid(text).expect("a valid uuid");
        assert_eq!(high, 0x069a_79f4_44e9_4726);
        assert_eq!(low, 0xa5be_fca9_0e38_aaf5);
        assert_eq!(format_uuid(high, low), text);
        assert_eq!(
            parse_uuid("069A79F4-44E9-4726-A5BE-FCA90E38AAF5"),
            Some((high, low))
        );
        for bad in [
            "",
            "069a79f444e94726a5befca90e38aaf5",
            "069a79f4-44e9-4726-a5be-fca90e38aaf",
            "069a79f4-44e9-4726-a5be-fca90e38aag5",
            "069a79f4_44e9-4726-a5be-fca90e38aaf5",
            "+69a79f4-44e9-4726-a5be-fca90e38aaf5",
        ] {
            assert_eq!(parse_uuid(bad), None, "{bad:?}");
        }
    }
}
