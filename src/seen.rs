//! The players this server has seen since the plugin was installed, by UUID, with the name each
//! one last used: so staff can warn, mute or ban a player who is offline. The SDK has no lookup
//! of offline players, so the plugin keeps its own, in `seen.tsv`
//! (`uuid  name  last-seen`), at most [`MAX_PLAYERS`] entries; past that the longest unseen go.

use std::collections::HashMap;

use crate::text;

pub const MAX_PLAYERS: usize = 20_000;
const EVICT_BATCH: usize = 500;

#[derive(Default, Debug)]
pub struct Seen {
    players: HashMap<String, (String, u64)>,
}

impl Seen {
    /// Notes that `id` was seen as `name` at `now`. True when the entry is new or the name changed.
    pub fn see(&mut self, id: &str, name: &str, now: u64) -> bool {
        if let Some((known, last)) = self.players.get_mut(id) {
            *last = now;
            if known == name {
                return false;
            }
            name.clone_into(known);
            return true;
        }
        self.players.insert(id.to_owned(), (name.to_owned(), now));
        if self.players.len() > MAX_PLAYERS {
            let mut by_age: Vec<(u64, String)> = self
                .players
                .iter()
                .map(|(id, (_, last))| (*last, id.clone()))
                .collect();
            by_age.sort_unstable();
            for (_, old) in by_age.into_iter().take(EVICT_BATCH) {
                self.players.remove(&old);
            }
        }
        true
    }

    /// The UUID and name of the player who last used `name` (any letter case).
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<(String, String)> {
        self.players
            .iter()
            .filter(|(_, (known, _))| known.eq_ignore_ascii_case(name))
            .max_by_key(|(_, (_, last))| *last)
            .map(|(id, (known, _))| (id.clone(), known.clone()))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.players.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    #[must_use]
    pub fn from_text(text: &str) -> (Self, usize) {
        let mut seen = Self::default();
        let mut bad = 0;
        for line in text
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.is_empty())
        {
            let fields: Vec<&str> = line.split('\t').collect();
            let entry = match fields.as_slice() {
                [id, name, last] if !id.is_empty() && text::valid_name(name) => last
                    .parse::<u64>()
                    .ok()
                    .map(|last| (text::unescape(id), ((*name).to_owned(), last))),
                _ => None,
            };
            match entry {
                Some((id, value)) if seen.players.len() < MAX_PLAYERS => {
                    seen.players.insert(id, value);
                }
                Some(_) => {}
                None => bad += 1,
            }
        }
        (seen, bad)
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        let mut ids: Vec<&String> = self.players.keys().collect();
        ids.sort_unstable();
        let mut out = String::with_capacity(ids.len() * 64);
        for id in ids {
            let (name, last) = &self.players[id];
            out.push_str(&format!("{}\t{name}\t{last}\n", text::escape(id)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_are_found_by_name_in_any_case() {
        let mut seen = Seen::default();
        assert!(seen.see("u1", "Alex", 10));
        assert!(!seen.see("u1", "Alex", 20), "same name");
        assert!(seen.see("u1", "Alexa", 30), "renamed");
        assert_eq!(seen.lookup("ALEXA"), Some(("u1".into(), "Alexa".into())));
        assert_eq!(seen.lookup("Alex"), None);
        assert_eq!(seen.lookup("nobody"), None);
    }

    #[test]
    fn a_name_used_by_two_players_finds_the_one_seen_last() {
        let mut seen = Seen::default();
        seen.see("old", "Steve", 10);
        seen.see("new", "steve", 50);
        assert_eq!(seen.lookup("Steve"), Some(("new".into(), "steve".into())));
    }

    #[test]
    fn the_index_survives_the_file_and_skips_bad_lines() {
        let mut seen = Seen::default();
        seen.see("u1", "Alex", 10);
        seen.see("u2", "Bob", 20);
        let text = format!("{}bad\nu3\tnot a name\t5\nu4\tCarl\tsoon\n", seen.to_text());
        let (read, bad) = Seen::from_text(&text);
        assert_eq!(bad, 3);
        assert_eq!(read.len(), 2);
        assert_eq!(read.lookup("bob"), Some(("u2".into(), "Bob".into())));
    }

    #[test]
    fn a_full_index_forgets_the_longest_unseen() {
        let mut seen = Seen::default();
        for i in 0..=MAX_PLAYERS {
            seen.see(&format!("u{i}"), &format!("p{i}"), i as u64);
        }
        assert!(seen.len() <= MAX_PLAYERS);
        assert_eq!(seen.lookup("p0"), None);
        assert!(seen.lookup(&format!("p{MAX_PLAYERS}")).is_some());
    }
}
