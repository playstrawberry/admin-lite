//! IP bans, made by player: the server holds every address and names a ban by an id (`ip-` and
//! hex digits). The plugin never receives, keeps or shows an address; it asks the server to ban
//! the address of a player it names by UUID, and records the ban in the history as a punishment
//! of that player.
//!
//! The server refuses a ban when the address is shared (a relay without the PROXY protocol, a
//! private network: a ban would lock out everybody behind it), when it has no record of the
//! player's last login, and when a player online on the address holds the protecting node
//! (admin-lite passes `admin-lite:exempt`). Its answer is an English sentence; [`refusal`] turns
//! it into the plugin's own text in the reader's language.

use crate::lang::Msg;

/// The language key that explains why the server banned nothing.
#[must_use]
pub fn refusal_key(why: &str) -> &'static str {
    let why = why.trim_start();
    if why.starts_with("This player's address is shared") {
        "ipban_shared"
    } else if why.starts_with("The server has no record") {
        "ipban_unknown"
    } else if why.starts_with("Someone on this address is protected") {
        "ipban_protected"
    } else {
        "ipban_failed"
    }
}

/// Why the server banned nothing, in the plugin's words. The server's sentence itself is never
/// shown: it is English only.
#[must_use]
pub fn refusal(why: &str, name: &str) -> Msg {
    Msg::new(refusal_key(why)).plain(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's sentences, as it answers a plugin (in English).
    const SHARED: &str = "This player's address is shared (a relay without PROXY protocol, or a private network): an IP ban would lock out everyone behind it, so nothing was banned.";
    const UNKNOWN: &str = "The server has no record of this player's last login.";
    const PROTECTED: &str =
        "Someone on this address is protected from IP bans, so nothing was banned.";

    #[test]
    fn each_refusal_of_the_server_has_its_own_text() {
        assert_eq!(refusal_key(SHARED), "ipban_shared");
        assert_eq!(refusal_key(UNKNOWN), "ipban_unknown");
        assert_eq!(refusal_key(PROTECTED), "ipban_protected");
        assert_eq!(refusal_key("the server is stopping"), "ipban_failed");
        assert_eq!(refusal_key(""), "ipban_failed");
    }

    #[test]
    fn a_refusal_is_told_in_the_readers_language_and_never_as_the_servers_sentence() {
        for why in [SHARED, UNKNOWN, PROTECTED, "something else 203.0.113.7"] {
            for code in ["en", "fr", "es", "tr"] {
                let text = refusal(why, "Alex").render(code);
                assert!(text.contains("Alex"), "{code}: {text}");
                assert!(!text.contains(why), "{code}: {text}");
                assert!(!text.contains("203.0.113.7"), "{code}: {text}");
            }
        }
        assert_ne!(
            refusal(SHARED, "Alex").render("en"),
            refusal(SHARED, "Alex").render("tr")
        );
    }
}
