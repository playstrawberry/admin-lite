//! Public punishment announcements: on by default; a server turns them off by taking the node
//! `admin-lite:announcements` from everyone.
//!
//! Every online player reads one line when a player is warned, muted, kicked or banned:
//! the punished name, the kind, the length and the reason. **Never the staff member's name**
//! (it is not an input of [`message`]) and never an IP address: the name and the reason pass
//! [`text::clean_reason`], which shows any address as `[IP]`. Lifting a punishment (unmute,
//! unban) is not announced.
//!
//! On by default. Until the plugin settings format arrives, the switch is
//! the permission node `admin-lite:announcements`, which every player holds by default: a server
//! owner who takes it away from everyone (with a permissions plugin) turns the announcements off.

use crate::{
    history::{Kind, Record},
    lang::Msg,
    text,
};

/// The announcement of `record`, or `None` for a kind that is not announced.
#[must_use]
pub fn message(record: &Record) -> Option<Msg> {
    let key = match record.kind {
        Kind::Warn => "announce_warn",
        Kind::Mute => "announce_mute",
        Kind::Kick => "announce_kick",
        Kind::Ban => "announce_ban",
        Kind::TempBan => "announce_tempban",
        Kind::Unmute | Kind::Unban => return None,
    };
    let name: String = text::clean_reason(&record.target_name)
        .chars()
        .take(32)
        .collect();
    let msg = Msg::new(key).plain(name);
    let msg = match (record.kind, record.duration) {
        (Kind::Mute | Kind::TempBan, Some(seconds)) => msg.duration(seconds),
        (Kind::Mute | Kind::TempBan, None) => msg.plain("-"),
        _ => msg,
    };
    let reason = text::clean_reason(&record.reason);
    Some(if reason.is_empty() {
        msg.key("no_reason")
    } else {
        msg.plain(reason)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Entry;

    fn record(kind: Kind, duration: Option<u64>, reason: &str) -> Record {
        let mut history = crate::history::History::default();
        history.add(
            Entry {
                kind,
                target_id: "00000000-0000-0000-0000-00000000a1e7".into(),
                target_name: "Alex".into(),
                staff_id: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
                staff_name: "SecretStaff".into(),
                duration,
                pending: false,
                reason: reason.into(),
            },
            1_000,
        )
    }

    #[test]
    fn punishments_are_announced_with_name_length_and_reason() {
        let text = |kind, duration, reason| {
            message(&record(kind, duration, reason)).map(|m| m.render("en"))
        };
        assert_eq!(
            text(Kind::Warn, None, "spam").as_deref(),
            Some("Alex was warned. Reason: spam")
        );
        assert_eq!(
            text(Kind::Mute, Some(3_600), "spam").as_deref(),
            Some("Alex was muted for 1h. Reason: spam")
        );
        assert_eq!(
            text(Kind::Kick, None, "").as_deref(),
            Some("Alex was kicked. Reason: no reason given")
        );
        assert_eq!(
            text(Kind::Ban, None, "x-ray").as_deref(),
            Some("Alex was banned. Reason: x-ray")
        );
        assert_eq!(
            text(Kind::TempBan, Some(86_400), "x-ray").as_deref(),
            Some("Alex was banned for 1d. Reason: x-ray")
        );
        let tr = message(&record(Kind::Mute, Some(5_400), "spam")).map(|m| m.render("tr"));
        assert_eq!(
            tr.as_deref(),
            Some("Alex, 1 sa 30 dk boyunca susturuldu. Sebep: spam")
        );
    }

    #[test]
    fn lifting_a_punishment_is_not_announced() {
        assert_eq!(message(&record(Kind::Unmute, None, "")), None);
        assert_eq!(message(&record(Kind::Unban, None, "")), None);
    }

    #[test]
    fn the_staff_name_and_addresses_never_appear() {
        let mut announced = 0;
        for kind in Kind::ALL {
            let Some(msg) = message(&record(
                kind,
                Some(60),
                "alt of 203.0.113.7 and 2001:db8::1",
            )) else {
                continue;
            };
            announced += 1;
            for code in ["en", "fr", "es", "tr"] {
                let text = msg.render(code);
                assert!(!text.contains("SecretStaff"), "{code} {kind:?}: {text}");
                assert!(
                    !text.contains("203.0.113.7") && !text.contains("2001:db8"),
                    "{code} {kind:?}: {text}"
                );
                assert!(text.contains("[IP]"), "{code} {kind:?}: {text}");
            }
        }
        assert_eq!(announced, 5);
        let mut odd = record(Kind::Kick, None, "");
        odd.target_name = "10.0.0.8".into();
        let text = message(&odd).map(|m| m.render("en")).unwrap_or_default();
        assert!(!text.is_empty() && !text.contains("10.0.0.8"), "{text}");
    }
}
