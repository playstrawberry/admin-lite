//! The plugin's texts in English, French, Spanish and Turkish (`lang/<code>.lang`, `key=text`
//! lines; `{0}` to `{5}` are filled in). Any other language reads English.
//!
//! A text is a [`Msg`]: a key and its arguments, rendered in each reader's language, so a staff
//! notice sent to five staff members reaches each one in his own language. An argument can be
//! plain text, another key (a punishment kind) or a duration, which are rendered in the same
//! language.

use crate::duration;

const FILES: [(&str, &str); 4] = [
    ("en", include_str!("../lang/en.lang")),
    ("fr", include_str!("../lang/fr.lang")),
    ("es", include_str!("../lang/es.lang")),
    ("tr", include_str!("../lang/tr.lang")),
];

/// The language of a locale as Minecraft writes it (`fr_fr`, `FrCa`, `tr-tr`).
#[must_use]
pub fn language(locale: &str) -> &'static str {
    let code = locale.get(..2).unwrap_or("").to_ascii_lowercase();
    FILES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| *c == code)
        .unwrap_or("en")
}

fn lookup<'a>(file: &'a str, key: &str) -> Option<&'a str> {
    file.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
}

/// The raw text of `key` in `language`; English when the language lacks it, the key itself when
/// English lacks it too.
#[must_use]
pub fn raw(language: &str, key: &str) -> String {
    let file = |code: &str| FILES.iter().find(|(c, _)| *c == code).map(|(_, f)| *f);
    file(language)
        .and_then(|f| lookup(f, key))
        .or_else(|| file("en").and_then(|f| lookup(f, key)))
        .unwrap_or(key)
        .to_owned()
}

/// One argument of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// Text that is shown as it is (a player name, a reason, a number).
    Plain(String),
    /// Another key, rendered in the reader's language.
    Key(&'static str),
    /// A duration in seconds, rendered with the reader's unit names.
    Duration(u64),
}

/// A text for players: a key and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub key: &'static str,
    pub args: Vec<Arg>,
}

impl Msg {
    #[must_use]
    pub const fn new(key: &'static str) -> Self {
        Self {
            key,
            args: Vec::new(),
        }
    }

    #[must_use]
    pub fn plain(mut self, text: impl Into<String>) -> Self {
        self.args.push(Arg::Plain(text.into()));
        self
    }

    #[must_use]
    pub fn key(mut self, key: &'static str) -> Self {
        self.args.push(Arg::Key(key));
        self
    }

    #[must_use]
    pub fn duration(mut self, seconds: u64) -> Self {
        self.args.push(Arg::Duration(seconds));
        self
    }

    /// The message in `language`.
    #[must_use]
    pub fn render(&self, language: &str) -> String {
        let mut out = raw(language, self.key);
        for (i, arg) in self.args.iter().enumerate() {
            let value = match arg {
                Arg::Plain(text) => text.clone(),
                Arg::Key(key) => raw(language, key),
                Arg::Duration(seconds) => duration::render(language, *seconds),
            };
            out = out.replace(&format!("{{{i}}}"), &value);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(file: &str) -> Vec<&str> {
        file.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.split_once('=').map_or(line, |(k, _)| k.trim()))
            .collect()
    }

    #[test]
    fn every_language_has_exactly_the_english_keys_with_the_same_placeholders() {
        let english = keys(FILES[0].1);
        for (code, file) in FILES {
            for line in file.lines().filter(|l| !l.trim().is_empty()) {
                assert!(line.contains('='), "{code}: a line without '=': {line}");
            }
            let mine = keys(file);
            assert_eq!(
                mine, english,
                "{code} has other keys than English, or another order"
            );
            for key in &english {
                let found = lookup(file, key).unwrap_or_else(|| panic!("{code} lacks {key}"));
                assert!(!found.is_empty(), "{code} {key} is empty");
                let en = lookup(FILES[0].1, key).unwrap_or_default();
                for slot in 0..6 {
                    let slot = format!("{{{slot}}}");
                    assert_eq!(
                        found.contains(&slot),
                        en.contains(&slot),
                        "{code} {key} {slot}"
                    );
                }
            }
        }
    }

    /// Every key the code uses exists in English: a missing key would show the key to players.
    #[test]
    fn every_key_the_code_uses_exists() {
        let sources = [
            include_str!("lib.rs"),
            include_str!("admin.rs"),
            include_str!("actions.rs"),
            include_str!("commands.rs"),
            include_str!("handlers.rs"),
            include_str!("menus.rs"),
            include_str!("menu.rs"),
            include_str!("history.rs"),
            include_str!("duration.rs"),
            include_str!("vanish.rs"),
            include_str!("announce.rs"),
        ];
        let english = keys(FILES[0].1);
        let mut used = 0;
        for source in sources {
            for marker in ["Msg::new(\"", ".key(\""] {
                for (index, _) in source.match_indices(marker) {
                    let rest = &source[index + marker.len()..];
                    let key = &rest[..rest.find('"').unwrap_or(0)];
                    if key.is_empty() || key.contains(' ') {
                        continue;
                    }
                    used += 1;
                    assert!(
                        english.contains(&key),
                        "the code uses {key}, which en.lang lacks"
                    );
                }
            }
        }
        assert!(used > 40, "the scan found only {used} keys");
    }

    #[test]
    fn locales_pick_their_language_and_others_read_english() {
        assert_eq!(language("fr_fr"), "fr");
        assert_eq!(language("FrCa"), "fr");
        assert_eq!(language("es_mx"), "es");
        assert_eq!(language("tr-tr"), "tr");
        assert_eq!(language("TrTr"), "tr");
        assert_eq!(language("de_de"), "en");
        assert_eq!(language(""), "en");
        assert_eq!(raw("en", "no_such_key"), "no_such_key");
    }

    #[test]
    fn messages_render_arguments_keys_and_durations_in_the_readers_language() {
        let msg = Msg::new("mute_done")
            .plain("Alex")
            .duration(5400)
            .plain("spam");
        assert_eq!(msg.render("en"), "Muted Alex for 1h 30m: spam");
        assert_eq!(
            msg.render("tr"),
            "Alex, 1 sa 30 dk boyunca susturuldu: spam"
        );
        let line = Msg::new("history_line")
            .plain("3")
            .plain("2026-11-02 03:04")
            .key("kind_warn")
            .plain("Steve")
            .plain("spam");
        assert_eq!(
            line.render("en"),
            "#3 2026-11-02 03:04 Warning by Steve: spam"
        );
        assert!(line.render("fr").contains("Avertissement"));
        assert_eq!(Msg::new("unit_s").plain("1").render("de"), "1s");
    }
}
