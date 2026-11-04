//! The audit log, `audit.log` in the plugin's data folder: one line per staff action, written
//! for the server owner. Nothing in a line comes from a player's connection: no IP address
//! (the plugin never asks the server for one), no password, no command line other than the
//! staff command's own words. Reasons are cleaned with [`text::clean_reason`] first, which also
//! hides IP addresses a staff member typed.
//!
//! Format: `2026-11-02T03:04:05Z staff=Steve(uuid) action=warn target=Alex(uuid) detail="..."`.
//! When the file passes [`ROTATE_AT`] bytes it becomes `audit.log.1` (the older one goes).

use crate::{duration, text};

pub const ROTATE_AT: u64 = 4 * 1024 * 1024;

/// Who did it or to whom: a name and an id (a UUID, or `console`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    pub name: String,
    pub id: String,
}

/// A name or id as one token: anything but letters, digits, `_` and `-` becomes `_`.
fn token(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// One audit line, without the line break.
#[must_use]
pub fn line(at: u64, staff: &Who, action: &str, target: Option<&Who>, detail: &str) -> String {
    let mut out = format!(
        "{} staff={}({}) action={}",
        duration::iso(at),
        token(&staff.name),
        token(&staff.id),
        token(action)
    );
    if let Some(target) = target {
        out.push_str(&format!(
            " target={}({})",
            token(&target.name),
            token(&target.id)
        ));
    }
    let detail = text::clean_reason(detail);
    if !detail.is_empty() {
        let quoted = detail.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!(" detail=\"{quoted}\""));
    }
    out
}

/// Whether a log of `size` bytes is rotated before the next line.
#[must_use]
pub const fn needs_rotation(size: u64) -> bool {
    size >= ROTATE_AT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steve() -> Who {
        Who {
            name: "Steve".into(),
            id: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
        }
    }

    #[test]
    fn a_line_names_the_time_staff_action_target_and_detail() {
        let alex = Who {
            name: "Alex".into(),
            id: "00000000-0000-0000-0000-00000000a1e7".into(),
        };
        let text = line(1_793_588_645, &steve(), "warn", Some(&alex), "spam in chat");
        assert_eq!(
            text,
            "2026-11-02T03:04:05Z staff=Steve(069a79f4-44e9-4726-a5be-fca90e38aaf5) action=warn \
             target=Alex(00000000-0000-0000-0000-00000000a1e7) detail=\"spam in chat\""
        );
        let console = Who {
            name: "Server".into(),
            id: "console".into(),
        };
        assert_eq!(
            line(0, &console, "vanish", None, ""),
            "1970-01-01T00:00:00Z staff=Server(console) action=vanish"
        );
    }

    #[test]
    fn a_detail_cannot_forge_a_second_line_or_show_an_ip() {
        let text = line(
            0,
            &steve(),
            "warn",
            None,
            "x\"\n1970-01-01T00:00:00Z staff=Owner from 1.2.3.4",
        );
        assert!(!text.contains('\n'));
        assert!(!text.contains("1.2.3.4"));
        assert!(
            text.ends_with("detail=\"x\\\" 1970-01-01T00:00:00Z staff=Owner from [IP]\""),
            "{text}"
        );
    }

    #[test]
    fn the_log_rotates_past_four_megabytes() {
        assert!(!needs_rotation(0));
        assert!(!needs_rotation(ROTATE_AT - 1));
        assert!(needs_rotation(ROTATE_AT));
    }
}
