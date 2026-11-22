//! Command lines other commands produce, read for two reasons:
//!
//! - **Filters.** A frozen player may not run commands (a teleport would carry him out of the
//!   freeze), except private messages to talk with staff and the login commands of a login
//!   plugin. A muted player may not use the commands that talk to others.
//! - **Vanilla `/kick`, `/ban` and `/pardon` stay vanilla** and are written into the history
//!   too: [`observe`] reads their target and reason, and the plugin checks one tick later that
//!   the command really kicked, banned or unbanned (the engine's ban list says). So do the
//!   server's `/ban-ip <player>` and `/pardon-ip <player>`, which ban and lift by player: only a
//!   player's name is read. An address or a ban id typed instead is not a name and is not read.
//!
//! A command line may hold a password (`/login secret`): nothing here keeps, logs or returns
//! any part of a line other than the target and reason of these five vanilla commands.

use crate::text;

/// What a vanilla command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observed {
    Kick { target: String, reason: String },
    Ban { target: String, reason: String },
    Pardon { target: String },
    IpBan { target: String, reason: String },
    IpPardon { target: String },
}

fn words(line: &str) -> std::str::SplitWhitespace<'_> {
    let line = line.trim_start();
    line.strip_prefix('/').unwrap_or(line).split_whitespace()
}

/// The command's label: lowercase, without the `/` and a `namespace:` prefix.
#[must_use]
pub fn label(line: &str) -> String {
    let first = words(line).next().unwrap_or("");
    first
        .split_once(':')
        .map_or(first, |(_, label)| label)
        .to_ascii_lowercase()
}

/// A vanilla kick, ban or pardon of one named player (selectors such as `@a` are not read).
#[must_use]
pub fn observe(line: &str) -> Option<Observed> {
    let label = label(line);
    if !matches!(
        label.as_str(),
        "kick" | "ban" | "pardon" | "ban-ip" | "pardon-ip"
    ) {
        return None;
    }
    let mut rest = words(line).skip(1);
    let target = rest
        .next()
        .filter(|name| text::valid_name(name))?
        .to_owned();
    let reason = text::clean_reason(&rest.collect::<Vec<_>>().join(" "));
    Some(match label.as_str() {
        "kick" => Observed::Kick { target, reason },
        "ban" => Observed::Ban { target, reason },
        "ban-ip" => Observed::IpBan { target, reason },
        "pardon-ip" => Observed::IpPardon { target },
        _ => Observed::Pardon { target },
    })
}

/// Commands a muted player may not use: they talk to other players.
#[must_use]
pub fn blocked_when_muted(label: &str) -> bool {
    matches!(
        label,
        "msg"
            | "tell"
            | "w"
            | "m"
            | "whisper"
            | "pm"
            | "me"
            | "say"
            | "teammsg"
            | "tm"
            | "r"
            | "reply"
    )
}

/// Commands a frozen player may still use.
#[must_use]
pub fn allowed_when_frozen(label: &str) -> bool {
    matches!(
        label,
        "msg" | "tell" | "w" | "m" | "whisper" | "r" | "reply" | "login" | "l" | "register" | "reg"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_lose_the_slash_the_namespace_and_the_case() {
        assert_eq!(label("/minecraft:Kick Bob"), "kick");
        assert_eq!(label("msg Bob hi"), "msg");
        assert_eq!(label("/TELL"), "tell");
        assert_eq!(label(""), "");
        assert_eq!(label("   "), "");
    }

    #[test]
    fn vanilla_kick_ban_and_pardon_are_read() {
        assert_eq!(
            observe("/kick Bob being  rude"),
            Some(Observed::Kick {
                target: "Bob".into(),
                reason: "being rude".into()
            })
        );
        assert_eq!(
            observe("minecraft:ban Bob"),
            Some(Observed::Ban {
                target: "Bob".into(),
                reason: String::new()
            })
        );
        assert_eq!(
            observe("/pardon Bob"),
            Some(Observed::Pardon {
                target: "Bob".into()
            })
        );
    }

    #[test]
    fn vanilla_ip_bans_of_a_named_player_are_read() {
        assert_eq!(
            observe("/ban-ip Bob alt of 203.0.113.7"),
            Some(Observed::IpBan {
                target: "Bob".into(),
                reason: "alt of [IP]".into()
            })
        );
        assert_eq!(
            observe("minecraft:pardon-ip Bob"),
            Some(Observed::IpPardon {
                target: "Bob".into()
            })
        );
    }

    #[test]
    fn other_lines_are_not_read() {
        for line in [
            "/kick @a",
            "/kick",
            "/ban-ip 1.2.3.4",
            "/ban-ip 1.2.3.4 spam",
            "/ban-ip 2001:db8::1",
            "/ban-ip @a",
            "/pardon-ip 1.2.3.4",
            "/pardon-ip ip-1a2b3c4d",
            "/pardon-ip ::1",
            "/kickall",
            "/login hunter2",
            "/ban not-a-name",
            "/banlist",
        ] {
            assert_eq!(observe(line), None, "{line}");
        }
    }

    #[test]
    fn muted_players_cannot_talk_through_commands() {
        for talk in [
            "msg", "tell", "w", "me", "say", "teammsg", "tm", "r", "reply",
        ] {
            assert!(blocked_when_muted(talk), "{talk}");
        }
        for other in ["spawn", "home", "login", "help"] {
            assert!(!blocked_when_muted(other), "{other}");
        }
    }

    #[test]
    fn frozen_players_may_only_message_and_log_in() {
        for allowed in [
            "msg", "tell", "w", "r", "reply", "login", "l", "register", "reg",
        ] {
            assert!(allowed_when_frozen(allowed), "{allowed}");
        }
        for blocked in ["spawn", "home", "tpa", "warp", "kill", "", "me"] {
            assert!(!allowed_when_frozen(blocked), "{blocked}");
        }
    }
}
