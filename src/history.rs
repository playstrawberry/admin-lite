//! The punishment history: every warning, mute, unmute, kick, ban, temporary ban and unban,
//! kept in `history.tsv` in the plugin's data folder, one record per line.
//!
//! The file is the plugin's own record; the server's ban list stays the authority on who is
//! banned. Mutes have no server counterpart, so the active mutes are derived from this history
//! (the newest mute or unmute of a player decides) and kept in an index for the chat check.
//!
//! Line format (tab-separated, fields escaped with [`text::escape`]):
//! `1  id  at  kind  target-id  target-name  staff-id  staff-name  duration|-  p|-  reason`.
//! A line that does not read is skipped and counted; the caller keeps a copy of the file before
//! it is written again, so nothing is lost.

use std::collections::HashMap;

use crate::text;

/// The most records kept. Past it, the oldest [`TRIM_BATCH`] go.
pub const MAX_RECORDS: usize = 50_000;
/// How many of the oldest records go at once when the history is full.
pub const TRIM_BATCH: usize = 1_000;

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Warn,
    Mute,
    Unmute,
    Kick,
    Ban,
    TempBan,
    Unban,
}

impl Kind {
    pub const ALL: [Self; 7] = [
        Self::Warn,
        Self::Mute,
        Self::Unmute,
        Self::Kick,
        Self::Ban,
        Self::TempBan,
        Self::Unban,
    ];

    /// The word in the file and in the audit log.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Kick => "kick",
            Self::Ban => "ban",
            Self::TempBan => "tempban",
            Self::Unban => "unban",
        }
    }

    /// The language key of its name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warn => "kind_warn",
            Self::Mute => "kind_mute",
            Self::Unmute => "kind_unmute",
            Self::Kick => "kind_kick",
            Self::Ban => "kind_ban",
            Self::TempBan => "kind_tempban",
            Self::Unban => "kind_unban",
        }
    }

    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }
}

/// The id of whoever acted: a player's UUID, or `console`.
pub const CONSOLE_ID: &str = "console";

/// One entry of the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Numbered from 1, never reused.
    pub id: u64,
    /// Unix time in seconds.
    pub at: u64,
    pub kind: Kind,
    pub target_id: String,
    pub target_name: String,
    pub staff_id: String,
    pub staff_name: String,
    /// Seconds, for a mute or a temporary ban.
    pub duration: Option<u64>,
    /// A warning the target has not seen yet (he was offline).
    pub pending: bool,
    pub reason: String,
}

/// A record before it gets its id and time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub target_id: String,
    pub target_name: String,
    pub staff_id: String,
    pub staff_name: String,
    pub duration: Option<u64>,
    pub pending: bool,
    pub reason: String,
}

/// An active mute: until when (`None` is never) and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mute {
    pub until: Option<u64>,
    pub reason: String,
}

impl Mute {
    /// Seconds left at `now`; `None` for a mute without an end.
    #[must_use]
    pub fn left(&self, now: u64) -> Option<u64> {
        self.until.map(|until| until.saturating_sub(now))
    }
}

#[derive(Default, Debug)]
pub struct History {
    records: Vec<Record>,
    next_id: u64,
    /// The deciding mute of each muted player (its record id and the mute), by target id.
    mutes: HashMap<String, (u64, Mute)>,
    /// Players with a warning they have not seen.
    pending: std::collections::HashSet<String>,
}

const VERSION: &str = "1";

fn parse_line(line: &str) -> Option<Record> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 11 || fields[0] != VERSION {
        return None;
    }
    let id = fields[1].parse::<u64>().ok().filter(|id| *id > 0)?;
    let at = fields[2].parse::<u64>().ok()?;
    let kind = Kind::from_code(fields[3])?;
    let text = |i: usize| Some(text::unescape(fields[i])).filter(|t| !t.is_empty());
    let duration = match fields[8] {
        "-" => None,
        n => Some(n.parse::<u64>().ok()?),
    };
    let pending = match fields[9] {
        "p" => true,
        "-" => false,
        _ => return None,
    };
    Some(Record {
        id,
        at,
        kind,
        target_id: text(4)?,
        target_name: text(5)?,
        staff_id: text(6)?,
        staff_name: text(7)?,
        duration,
        pending,
        reason: text::unescape(fields[10]),
    })
}

fn write_line(record: &Record, out: &mut String) {
    let duration = record
        .duration
        .map_or_else(|| "-".to_owned(), |d| d.to_string());
    let fields = [
        VERSION.to_owned(),
        record.id.to_string(),
        record.at.to_string(),
        record.kind.code().to_owned(),
        text::escape(&record.target_id),
        text::escape(&record.target_name),
        text::escape(&record.staff_id),
        text::escape(&record.staff_name),
        duration,
        if record.pending { "p" } else { "-" }.to_owned(),
        text::escape(&record.reason),
    ];
    out.push_str(&fields.join("\t"));
    out.push('\n');
}

impl History {
    /// The file line of one record, with its line break (for appending).
    #[must_use]
    pub fn line(record: &Record) -> String {
        let mut out = String::with_capacity(96);
        write_line(record, &mut out);
        out
    }

    /// Reads the file's text. Returns the history and how many lines did not read.
    #[must_use]
    pub fn from_text(text: &str) -> (Self, usize) {
        let mut history = Self::default();
        let mut bad = 0;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            match parse_line(line.trim_end_matches('\r')) {
                Some(record) => {
                    history.next_id = history.next_id.max(record.id + 1);
                    history.index(&record);
                    history.records.push(record);
                }
                None => bad += 1,
            }
        }
        (history, bad)
    }

    /// The file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(self.records.len() * 96);
        for record in &self.records {
            write_line(record, &mut out);
        }
        out
    }

    fn index(&mut self, record: &Record) {
        match record.kind {
            Kind::Mute => {
                let until = record.duration.map(|d| record.at.saturating_add(d));
                let mute = Mute {
                    until,
                    reason: record.reason.clone(),
                };
                self.mutes
                    .insert(record.target_id.clone(), (record.id, mute));
            }
            Kind::Unmute => {
                self.mutes.remove(&record.target_id);
            }
            Kind::Warn if record.pending => {
                self.pending.insert(record.target_id.clone());
            }
            _ => {}
        }
    }

    /// Adds a record at time `at` and returns it.
    pub fn add(&mut self, entry: Entry, at: u64) -> Record {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        let record = Record {
            id,
            at,
            kind: entry.kind,
            target_id: entry.target_id,
            target_name: entry.target_name,
            staff_id: entry.staff_id,
            staff_name: entry.staff_name,
            duration: entry.duration,
            pending: entry.pending,
            reason: entry.reason,
        };
        self.index(&record);
        self.records.push(record.clone());
        if self.records.len() > MAX_RECORDS {
            self.trim();
        }
        record
    }

    /// Drops the oldest [`TRIM_BATCH`] records, except the ones that decide an active mute.
    fn trim(&mut self) {
        let keep: std::collections::HashSet<u64> = self.mutes.values().map(|(id, _)| *id).collect();
        let mut dropped = 0;
        self.records.retain(|record| {
            if dropped < TRIM_BATCH && !keep.contains(&record.id) {
                dropped += 1;
                false
            } else {
                true
            }
        });
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The player's active mute at `now`, if any.
    #[must_use]
    pub fn mute_of(&self, target_id: &str, now: u64) -> Option<Mute> {
        let (_, mute) = self.mutes.get(target_id)?;
        mute.until
            .is_none_or(|until| until > now)
            .then(|| mute.clone())
    }

    /// The warnings the player has not seen, marked as seen now.
    pub fn take_pending(&mut self, target_id: &str) -> Vec<Record> {
        if !self.pending.remove(target_id) {
            return Vec::new();
        }
        let mut given = Vec::new();
        for record in &mut self.records {
            if record.pending && record.kind == Kind::Warn && record.target_id == target_id {
                record.pending = false;
                given.push(record.clone());
            }
        }
        given
    }

    /// One page (from 1) of the player's records, newest first, and the number of pages (at
    /// least 1).
    #[must_use]
    pub fn page(&self, target_id: &str, page: usize, per_page: usize) -> (Vec<Record>, usize) {
        let per_page = per_page.max(1);
        let mine: Vec<&Record> = self
            .records
            .iter()
            .rev()
            .filter(|r| r.target_id == target_id)
            .collect();
        let pages = mine.len().div_ceil(per_page).max(1);
        let start = page.max(1).saturating_sub(1).saturating_mul(per_page);
        let shown = mine
            .into_iter()
            .skip(start)
            .take(per_page)
            .cloned()
            .collect();
        (shown, pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALEX: &str = "00000000-0000-0000-0000-00000000a1e7";
    const BOB: &str = "00000000-0000-0000-0000-000000000b0b";

    fn entry(kind: Kind, target: &str, duration: Option<u64>, reason: &str) -> Entry {
        Entry {
            kind,
            target_id: target.into(),
            target_name: if target == ALEX { "Alex" } else { "Bob" }.into(),
            staff_id: CONSOLE_ID.into(),
            staff_name: "Server".into(),
            duration,
            pending: false,
            reason: reason.into(),
        }
    }

    #[test]
    fn every_kind_has_a_code_and_a_text() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_code(kind.code()), Some(kind));
            assert_ne!(crate::lang::raw("en", kind.label()), kind.label());
        }
        assert_eq!(Kind::from_code("jail"), None);
    }

    #[test]
    fn records_get_increasing_ids_and_survive_the_file() {
        let mut history = History::default();
        let first = history.add(entry(Kind::Warn, ALEX, None, "tab\there\nnew line"), 100);
        let second = history.add(entry(Kind::TempBan, BOB, Some(86_400), "x-ray"), 200);
        assert_eq!((first.id, second.id), (1, 2));
        assert_eq!((first.at, second.at), (100, 200));
        let text = history.to_text();
        assert_eq!(text.lines().count(), 2);
        let (read, bad) = History::from_text(&text);
        assert_eq!(bad, 0);
        assert_eq!(read.to_text(), text);
        let (page, pages) = read.page(ALEX, 1, 10);
        assert_eq!(pages, 1);
        assert_eq!(page, vec![first]);
        let mut read = read;
        assert_eq!(read.add(entry(Kind::Kick, ALEX, None, ""), 300).id, 3);
    }

    #[test]
    fn bad_lines_are_skipped_and_counted() {
        let mut history = History::default();
        history.add(entry(Kind::Warn, ALEX, None, "one"), 1);
        let text = format!(
            "{}garbage\n1\tx\t1\twarn\n2\t9\t1\twarn\ta\tb\tc\td\t-\t-\tr\n1\t9\t1\tnope\ta\tb\tc\td\t-\t-\tr\n",
            history.to_text()
        );
        let (read, bad) = History::from_text(&text);
        assert_eq!(bad, 4);
        assert_eq!(read.len(), 1);
    }

    #[test]
    fn the_newest_mute_or_unmute_decides() {
        let mut history = History::default();
        assert_eq!(history.mute_of(ALEX, 0), None);
        history.add(entry(Kind::Mute, ALEX, Some(3_600), "spam"), 1_000);
        let mute = history.mute_of(ALEX, 1_100).expect("muted");
        assert_eq!(mute.until, Some(4_600));
        assert_eq!(mute.left(1_100), Some(3_500));
        assert_eq!(mute.reason, "spam");
        assert_eq!(history.mute_of(ALEX, 4_600), None, "ended");
        assert_eq!(history.mute_of(BOB, 1_100), None);
        history.add(entry(Kind::Kick, ALEX, None, ""), 1_200);
        assert!(
            history.mute_of(ALEX, 1_300).is_some(),
            "a kick does not unmute"
        );
        history.add(entry(Kind::Unmute, ALEX, None, ""), 1_400);
        assert_eq!(history.mute_of(ALEX, 1_500), None);
        history.add(entry(Kind::Mute, ALEX, Some(60), "again"), 2_000);
        let (read, _) = History::from_text(&history.to_text());
        assert_eq!(
            read.mute_of(ALEX, 2_030).map(|m| m.reason),
            Some("again".into())
        );
    }

    #[test]
    fn pending_warnings_are_given_once_and_the_file_remembers() {
        let mut history = History::default();
        let mut warn = entry(Kind::Warn, ALEX, None, "while away");
        warn.pending = true;
        history.add(warn, 5);
        history.add(entry(Kind::Warn, ALEX, None, "seen"), 6);
        let (mut read, _) = History::from_text(&history.to_text());
        let given = read.take_pending(ALEX);
        assert_eq!(given.len(), 1);
        assert_eq!(given[0].reason, "while away");
        assert!(read.take_pending(ALEX).is_empty());
        let (mut again, _) = History::from_text(&read.to_text());
        assert!(again.take_pending(ALEX).is_empty());
    }

    #[test]
    fn pages_are_newest_first() {
        let mut history = History::default();
        for i in 0..23 {
            history.add(entry(Kind::Warn, ALEX, None, &i.to_string()), i);
        }
        history.add(entry(Kind::Warn, BOB, None, "other"), 99);
        let (first, pages) = history.page(ALEX, 1, 8);
        assert_eq!(pages, 3);
        assert_eq!(first.len(), 8);
        assert_eq!(first[0].reason, "22");
        let (last, _) = history.page(ALEX, 3, 8);
        assert_eq!(last.len(), 7);
        assert_eq!(last[6].reason, "0");
        let (beyond, _) = history.page(ALEX, 9, 8);
        assert!(beyond.is_empty());
        assert_eq!(history.page("nobody", 1, 8), (Vec::new(), 1));
    }

    #[test]
    fn a_full_history_drops_its_oldest_records_but_keeps_active_mutes() {
        let mut history = History::default();
        history.add(entry(Kind::Mute, BOB, None, "forever"), 0);
        for i in 0..MAX_RECORDS {
            history.add(entry(Kind::Warn, ALEX, None, ""), i as u64);
        }
        assert!(history.len() <= MAX_RECORDS);
        assert!(history.len() > MAX_RECORDS - TRIM_BATCH);
        assert!(history.mute_of(BOB, 10).is_some());
        let (_, pages) = history.page(ALEX, 1, MAX_RECORDS);
        assert_eq!(pages, 1);
    }
}
