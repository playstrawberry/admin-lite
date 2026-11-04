//! The plugin's shared state, its files and the helpers every command, menu and event uses.
//!
//! Rule: never hold one of these locks across a host call. A host call can re-enter the plugin
//! (opening a window fires the close event of the old one, a teleport fires the teleport event),
//! and `std::sync::Mutex` is not reentrant.
//!
//! Files (the plugin's data folder, `plugins/data/admin-lite/` on the server):
//! - `history.tsv`: the punishment history (`history`), appended to; rewritten only when a
//!   record changes (a warning delivered at join) or the oldest records go;
//! - `seen.tsv`: the players seen, for offline targets (`seen`);
//! - `state.tsv`: vanished, frozen and watching staff, exempt players (`staffstate`);
//! - `audit.log`: one line per staff action (`audit`), appended; `audit.log.1` is the one before.
//!
//! Nothing is written in an event handler or a command: a change marks the file dirty and one
//! async task writes it a second later (and again at unload). A file that does not read in full
//! is copied to `<file>.broken-<time>` before it is written again, so nothing is lost.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

use strawberry_plugin_api::{
    Player, Server,
    command::{CommandError, CommandSender},
    common::NamedColor,
    logging::{LogLevel, log},
    scheduler::SchedulerExt,
    text::TextComponent,
    uuid::Uuid,
};

use crate::{
    audit::{self, Who},
    duration,
    history::{Entry, History, Kind, Record},
    lang::{self, Msg},
    menus::View,
    perms,
    seen::Seen,
    staffstate::StaffState,
    text,
};

pub const HISTORY_FILE: &str = "data/history.tsv";
pub const SEEN_FILE: &str = "data/seen.tsv";
pub const STATE_FILE: &str = "data/state.tsv";
pub const AUDIT_FILE: &str = "data/audit.log";
const AUDIT_OLD: &str = "data/audit.log.1";
/// How long an expected kick counts, in seconds.
const KICK_WINDOW: u64 = 10;

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A UUID as the plugin writes it.
#[must_use]
pub fn id_of(player: &Player) -> String {
    player.get_id().to_string()
}

#[must_use]
pub fn uuid_from(text: &str) -> Option<Uuid> {
    text::parse_uuid(text).map(|(high, low)| Uuid { high, low })
}

/// A vanilla `/kick` or `/ban` seen in a command line, written into the history when the kick it
/// causes happens.
#[derive(Clone, Debug)]
pub struct Pending {
    pub staff: Who,
    pub kind: Kind,
    pub reason: String,
}

/// Whoever ran a command or clicked a menu.
pub struct Actor {
    pub name: String,
    /// A UUID, or `console`.
    pub id: String,
    pub player: Option<Player>,
    language: &'static str,
}

impl Actor {
    #[must_use]
    pub fn of_player(player: Player) -> Self {
        Self {
            name: player.get_name(),
            id: id_of(&player),
            language: lang::language(&player.get_locale()),
            player: Some(player),
        }
    }

    #[must_use]
    pub fn of_sender(sender: &CommandSender) -> Self {
        match sender.as_player() {
            Some(player) => Self::of_player(player),
            None => Self {
                name: sender.get_name(),
                id: crate::history::CONSOLE_ID.to_owned(),
                player: None,
                language: lang::language(&format!("{:?}", sender.get_locale())),
            },
        }
    }

    #[must_use]
    pub const fn language(&self) -> &'static str {
        self.language
    }

    #[must_use]
    pub fn who(&self) -> Who {
        Who {
            name: self.name.clone(),
            id: self.id.clone(),
        }
    }

    /// Whether the actor holds `node` (the console holds every node).
    #[must_use]
    pub fn has(&self, node: &str) -> bool {
        self.player.as_ref().is_none_or(|p| p.has_permission(node))
    }

    /// Sends a line to the actor (the console log for the console).
    pub fn tell(&self, line: &Line) {
        match &self.player {
            Some(player) => player.send_system_message(line.component(self.language), false),
            None => log(LogLevel::Info, &line.component("en").to_pretty_console()),
        }
    }
}

/// A text for players: one of ours, or one of the game's own (rendered by each client in its
/// language, so a vanilla-like action answers exactly as vanilla does).
#[derive(Clone, Debug)]
pub enum Line {
    Ours(Msg),
    Vanilla(&'static str, Vec<VArg>),
}

#[derive(Clone, Debug)]
pub enum VArg {
    Text(String),
    Key(&'static str),
}

impl Line {
    #[must_use]
    pub fn vanilla(key: &'static str, args: Vec<VArg>) -> Self {
        Self::Vanilla(key, args)
    }

    #[must_use]
    pub fn component(&self, language: &str) -> TextComponent {
        match self {
            Self::Ours(msg) => TextComponent::text(&msg.render(language)),
            Self::Vanilla(key, args) => {
                let with = args
                    .iter()
                    .map(|arg| match arg {
                        VArg::Text(text) => TextComponent::text(text),
                        VArg::Key(key) => TextComponent::translate(key, Vec::new()),
                    })
                    .collect();
                TextComponent::translate(key, with)
            }
        }
    }
}

impl From<Msg> for Line {
    fn from(msg: Msg) -> Self {
        Self::Ours(msg)
    }
}

/// Sends one of our texts to a player in his language.
pub fn tell(player: &Player, msg: &Msg, color: Option<NamedColor>) {
    let text = TextComponent::text(&msg.render(lang::language(&player.get_locale())));
    let text = match color {
        Some(color) => text.color_named(color),
        None => text,
    };
    player.send_system_message(text, false);
}

/// A failed command: the text in red, in the sender's language.
#[must_use]
pub fn failure(actor: &Actor, line: &Line) -> CommandError {
    CommandError::CommandFailed(
        line.component(actor.language())
            .color_named(NamedColor::Red),
    )
}

/// The plugin's state, shared by every handler.
#[derive(Default)]
pub struct Admin {
    pub history: Mutex<History>,
    pub seen: Mutex<Seen>,
    pub staff: Mutex<StaffState>,
    /// The menu each staff member has open, by UUID.
    pub menus: Mutex<HashMap<String, View>>,
    /// The UUIDs on the server's ban list as the plugin last saw it (read at load).
    pub known_bans: Mutex<std::collections::HashSet<String>>,
    /// Kicks the plugin caused or saw a vanilla command for, by lowercase name, with their time
    /// and, for a vanilla command, what to write when the kick happens.
    expected_kicks: Mutex<HashMap<String, (u64, Option<Pending>)>>,
    /// History lines not written yet.
    history_new: Mutex<Vec<String>>,
    history_rewrite: AtomicBool,
    audit_new: Mutex<Vec<String>>,
    seen_dirty: AtomicBool,
    staff_dirty: AtomicBool,
    flush_planned: AtomicBool,
}

/// Reads a file; `None` when it does not exist, an error when it cannot be read.
fn read_optional(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("admin-lite: cannot read {path}: {e}")),
    }
}

/// Keeps a copy of a file that did not read in full.
fn keep_broken(path: &str, bad: usize) {
    let copy = format!("{path}.broken-{}", duration::now());
    let kept = std::fs::copy(path, &copy).is_ok();
    log(
        LogLevel::Warn,
        &format!(
            "admin-lite: {bad} line(s) of {path} did not read and were skipped; {}",
            if kept {
                format!("the file was kept as {copy}")
            } else {
                "a copy could not be kept".to_owned()
            }
        ),
    );
}

fn write_atomic(path: &str, text: &str) {
    let temporary = format!("{path}.tmp");
    let written = std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, path));
    if let Err(e) = written {
        log(
            LogLevel::Error,
            &format!("admin-lite: could not write {path}: {e}"),
        );
    }
}

fn append(path: &str, lines: &[String]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut text =
        String::with_capacity(lines.iter().map(String::len).sum::<usize>() + lines.len());
    for line in lines {
        text.push_str(line);
        if !line.ends_with('\n') {
            text.push('\n');
        }
    }
    file.write_all(text.as_bytes())
}

impl Admin {
    /// Reads the three files. A file that cannot be read stops the load (the plugin must not
    /// start with an empty history and later write it over the real one).
    pub fn load(&self) -> Result<(), String> {
        std::fs::create_dir_all("data")
            .map_err(|e| format!("admin-lite: cannot create the data folder: {e}"))?;
        if let Some(text) = read_optional(HISTORY_FILE)? {
            let (history, bad) = History::from_text(&text);
            if bad > 0 {
                keep_broken(HISTORY_FILE, bad);
                self.history_rewrite.store(true, Ordering::Relaxed);
            }
            *lock(&self.history) = history;
        }
        if let Some(text) = read_optional(SEEN_FILE)? {
            let (seen, bad) = Seen::from_text(&text);
            if bad > 0 {
                keep_broken(SEEN_FILE, bad);
            }
            *lock(&self.seen) = seen;
        }
        if let Some(text) = read_optional(STATE_FILE)? {
            let (state, bad) = StaffState::from_text(&text);
            if bad > 0 {
                keep_broken(STATE_FILE, bad);
            }
            *lock(&self.staff) = state;
        }
        Ok(())
    }

    /// Writes what changed. Runs in an async task, and at unload.
    pub fn flush(&self) {
        let (history_text, history_lines) = {
            let history = lock(&self.history);
            let mut new = lock(&self.history_new);
            let rewrite = self.history_rewrite.swap(false, Ordering::AcqRel);
            let lines = std::mem::take(&mut *new);
            (rewrite.then(|| history.to_text()), lines)
        };
        if let Some(text) = history_text {
            write_atomic(HISTORY_FILE, &text);
        } else if !history_lines.is_empty()
            && let Err(e) = append(HISTORY_FILE, &history_lines)
        {
            log(
                LogLevel::Error,
                &format!("admin-lite: could not write {HISTORY_FILE}: {e}"),
            );
            // Written in full next time, so no record is lost.
            self.history_rewrite.store(true, Ordering::Release);
        }
        if self.seen_dirty.swap(false, Ordering::AcqRel) {
            let text = lock(&self.seen).to_text();
            write_atomic(SEEN_FILE, &text);
        }
        if self.staff_dirty.swap(false, Ordering::AcqRel) {
            let text = lock(&self.staff).to_text();
            write_atomic(STATE_FILE, &text);
        }
        let audit_lines = std::mem::take(&mut *lock(&self.audit_new));
        if !audit_lines.is_empty() {
            let size = std::fs::metadata(AUDIT_FILE).map_or(0, |m| m.len());
            if audit::needs_rotation(size) {
                let _ = std::fs::remove_file(AUDIT_OLD);
                let _ = std::fs::rename(AUDIT_FILE, AUDIT_OLD);
            }
            if let Err(e) = append(AUDIT_FILE, &audit_lines) {
                log(
                    LogLevel::Error,
                    &format!("admin-lite: could not write {AUDIT_FILE}: {e}"),
                );
            }
        }
    }

    /// Plans one write a second from now (several changes share it).
    pub fn plan_flush(self: &Arc<Self>, server: &Server) {
        if self.flush_planned.swap(true, Ordering::AcqRel) {
            return;
        }
        let admin = Arc::clone(self);
        server.schedule_delayed_async_task(20, move |_| {
            admin.flush_planned.store(false, Ordering::Release);
            admin.flush();
        });
    }

    pub fn staff_changed(self: &Arc<Self>, server: &Server) {
        self.staff_dirty.store(true, Ordering::Release);
        self.plan_flush(server);
    }

    pub fn seen_changed(self: &Arc<Self>, server: &Server) {
        self.seen_dirty.store(true, Ordering::Release);
        self.plan_flush(server);
    }

    /// The history was changed in place: written in full next time.
    pub fn history_changed(self: &Arc<Self>, server: &Server) {
        self.history_rewrite.store(true, Ordering::Release);
        self.plan_flush(server);
    }

    /// Adds a punishment to the history (and its audit line) and returns the record.
    pub fn record(self: &Arc<Self>, server: &Server, actor: &Actor, entry: Entry) -> Record {
        let now = duration::now();
        let record = {
            let mut history = lock(&self.history);
            let before = history.len();
            let record = history.add(entry, now);
            if history.len() <= before {
                self.history_rewrite.store(true, Ordering::Release);
            }
            lock(&self.history_new).push(History::line(&record));
            record
        };
        let mut detail = record.reason.clone();
        if let Some(seconds) = record.duration {
            detail = format!("{} {detail}", duration::render("en", seconds));
        }
        let target = Who {
            name: record.target_name.clone(),
            id: record.target_id.clone(),
        };
        self.audit(
            server,
            actor,
            record.kind.code(),
            Some(&target),
            detail.trim(),
        );
        announce(server, &record, &[&actor.id]);
        record
    }

    /// Writes an audit line for an action.
    pub fn audit(
        self: &Arc<Self>,
        server: &Server,
        actor: &Actor,
        action: &str,
        target: Option<&Who>,
        detail: &str,
    ) {
        let line = audit::line(duration::now(), &actor.who(), action, target, detail);
        lock(&self.audit_new).push(line);
        self.plan_flush(server);
    }

    /// The same, for a vanilla command seen in the command line, whose sender is a name and id.
    pub fn audit_who(
        self: &Arc<Self>,
        server: &Server,
        staff: &Who,
        action: &str,
        target: Option<&Who>,
        detail: &str,
    ) {
        let line = audit::line(duration::now(), staff, action, target, detail);
        lock(&self.audit_new).push(line);
        self.plan_flush(server);
    }

    /// Adds a history record for a vanilla kick, ban or unban the plugin saw.
    pub fn record_vanilla(self: &Arc<Self>, server: &Server, staff: &Who, entry: Entry) {
        let now = duration::now();
        let record = {
            let mut history = lock(&self.history);
            let before = history.len();
            let record = history.add(entry, now);
            if history.len() <= before {
                self.history_rewrite.store(true, Ordering::Release);
            }
            lock(&self.history_new).push(History::line(&record));
            record
        };
        let target = Who {
            name: record.target_name.clone(),
            id: record.target_id.clone(),
        };
        self.audit_who(
            server,
            staff,
            record.kind.code(),
            Some(&target),
            &record.reason,
        );
        announce(server, &record, &[&staff.id]);
    }

    /// Notes that `name` is about to be kicked: by admin-lite itself (`None`, already in the
    /// history) or by a vanilla `/kick` or `/ban` the plugin saw (written when the kick happens).
    pub fn expect_kick(&self, name: &str, pending: Option<Pending>) {
        let now = duration::now();
        let mut expected = lock(&self.expected_kicks);
        expected.retain(|_, (at, _)| now.saturating_sub(*at) <= KICK_WINDOW);
        expected.insert(name.to_ascii_lowercase(), (now, pending));
    }

    /// The expected kick of `name`, if any (and forgets it): `Some(None)` for one of admin-lite's.
    pub fn take_expected_kick(&self, name: &str) -> Option<Option<Pending>> {
        let now = duration::now();
        lock(&self.expected_kicks)
            .remove(&name.to_ascii_lowercase())
            .filter(|(at, _)| now.saturating_sub(*at) <= KICK_WINDOW)
            .map(|(_, pending)| pending)
    }

    /// Whether the player with this UUID cannot be punished or frozen by staff.
    #[must_use]
    pub fn is_exempt(&self, server: &Server, target_id: &str) -> bool {
        match uuid_from(target_id).and_then(|id| server.get_player_by_uuid(id)) {
            Some(player) => player.has_permission(perms::EXEMPT),
            None => lock(&self.staff).exempt.contains(target_id),
        }
    }
}

/// The public announcement of a punishment (`announce`): to every online holder of
/// `admin-lite:announcements` (everyone by default) but the punished player and the actor, who
/// get their own lines, and but staff with `admin-lite:notify`, who get the staff notice. Never
/// the staff name, never an IP address. Nothing is written to the console: the audit log has it.
pub fn announce(server: &Server, record: &Record, skip: &[&str]) {
    let Some(msg) = crate::announce::message(record) else {
        return;
    };
    let color = match record.kind {
        crate::history::Kind::Ban | crate::history::Kind::TempBan => NamedColor::Red,
        _ => NamedColor::Yellow,
    };
    for viewer in server.get_all_players() {
        let id = id_of(&viewer);
        if id == record.target_id
            || skip.contains(&id.as_str())
            || !viewer.has_permission(perms::ANNOUNCEMENTS)
            || viewer.has_permission(perms::NOTIFY)
        {
            continue;
        }
        tell(&viewer, &msg, Some(color));
    }
}

/// A staff notice, as vanilla tells operators about another operator's command:
/// `[Steve: Muted Alex for 1h: spam]` in grey italics, to every online holder of
/// `admin-lite:notify` but the actor, and to the console.
pub fn notify(server: &Server, actor: &Actor, line: &Line) {
    for viewer in server.get_all_players() {
        if id_of(&viewer) == actor.id || !viewer.has_permission(perms::NOTIFY) {
            continue;
        }
        let language = lang::language(&viewer.get_locale());
        let notice = TextComponent::translate(
            "chat.type.admin",
            vec![TextComponent::text(&actor.name), line.component(language)],
        )
        .color_named(NamedColor::Gray)
        .italic(true);
        viewer.send_system_message(notice, false);
    }
    log(
        LogLevel::Info,
        &format!(
            "[{}: {}]",
            actor.name,
            line.component("en").to_pretty_console()
        ),
    );
}

/// A punishment kind for the history, named for the audit line too.
#[must_use]
pub fn entry(
    kind: Kind,
    actor: &Actor,
    target_id: &str,
    target_name: &str,
    duration: Option<u64>,
    reason: &str,
) -> Entry {
    Entry {
        kind,
        target_id: target_id.to_owned(),
        target_name: target_name.to_owned(),
        staff_id: actor.id.clone(),
        staff_name: actor.name.clone(),
        duration,
        pending: false,
        reason: text::clean_reason(reason),
    }
}
