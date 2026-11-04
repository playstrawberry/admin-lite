//! Every staff action, used by the commands and by the menus alike. Each one checks first,
//! records second (history and audit line), and acts last, then tells the actor and the other
//! staff. An action answers `Err(line)` when it cannot be done; nothing has changed then.

use std::sync::Arc;

use strawberry_plugin_api::{
    Player, Server,
    common::{GameMode, NamedColor},
    player::{BanPlayerOptions, BedrockDisconnectReason, BedrockKickOptions, JavaKickOptions},
    scheduler::SchedulerExt,
    text::TextComponent,
    uuid::Uuid,
};

use crate::{
    admin::{self, Actor, Admin, Line, Pending, VArg, id_of, lock, notify, tell},
    audit::Who,
    cmdline::Observed,
    duration,
    history::{Entry, Kind, Record},
    lang::{self, Msg},
    staffstate::{Frozen, Mode, Watch},
    text, vanish,
};

pub type Outcome = Result<(), Line>;

/// A player staff act on: online, or known from an earlier visit.
pub struct Target {
    pub id: String,
    pub name: String,
    pub player: Option<Player>,
}

/// A name as staff typed it, safe to show back.
fn shown(name: &str) -> String {
    text::clean_reason(name).chars().take(32).collect()
}

fn who(player: &Player) -> Who {
    Who {
        name: player.get_name(),
        id: id_of(player),
    }
}

/// The online player called `name`.
pub fn online(server: &Server, name: &str) -> Result<Player, Line> {
    let missing = || Line::from(Msg::new("not_online").plain(shown(name)));
    if !text::valid_name(name) {
        return Err(missing());
    }
    server.get_player_by_name(name).ok_or_else(missing)
}

/// The player called `name`, online or seen before.
pub fn known(admin: &Admin, server: &Server, name: &str) -> Result<Target, Line> {
    let unknown = || Line::from(Msg::new("unknown_player").plain(shown(name)));
    if !text::valid_name(name) {
        return Err(unknown());
    }
    if let Some(player) = server.get_player_by_name(name) {
        return Ok(Target {
            id: id_of(&player),
            name: player.get_name(),
            player: Some(player),
        });
    }
    let found = lock(&admin.seen).lookup(name);
    found
        .map(|(id, name)| Target {
            id,
            name,
            player: None,
        })
        .ok_or_else(unknown)
}

/// The player with this UUID if he is online.
#[must_use]
pub fn by_id(server: &Server, id: &str) -> Option<Player> {
    admin::uuid_from(id).and_then(|uuid| server.get_player_by_uuid(uuid))
}

/// Staff may not punish or freeze themselves, nor (unless they are the console) an exempt player.
fn guard(admin: &Admin, server: &Server, actor: &Actor, id: &str, name: &str) -> Outcome {
    if actor.id == id {
        return Err(Msg::new("self_target").into());
    }
    if actor.player.is_some() && admin.is_exempt(server, id) {
        return Err(Msg::new("exempt").plain(name).into());
    }
    Ok(())
}

fn with_reason(msg: Msg, reason: &str) -> Msg {
    if reason.is_empty() {
        msg.key("no_reason")
    } else {
        msg.plain(reason)
    }
}

fn self_or_other(
    actor: &Actor,
    target: &Player,
    self_key: &'static str,
    other_key: &'static str,
) -> (bool, Msg) {
    let name = target.get_name();
    if id_of(target) == actor.id {
        (true, Msg::new(self_key))
    } else {
        (false, Msg::new(other_key).plain(name))
    }
}

pub fn heal(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    target.set_health(target.get_max_health());
    let (own, msg) = self_or_other(actor, target, "healed_self", "healed_other");
    if !own {
        tell(target, &Msg::new("healed_self"), None);
    }
    actor.tell(&msg.into());
    notify(
        server,
        actor,
        &Msg::new("healed_other").plain(target.get_name()).into(),
    );
    admin.audit(server, actor, "heal", Some(&who(target)), "");
    Ok(())
}

pub fn feed(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    target.set_food_level(20);
    target.set_saturation(20.0);
    let (own, msg) = self_or_other(actor, target, "fed_self", "fed_other");
    if !own {
        tell(target, &Msg::new("fed_self"), None);
    }
    actor.tell(&msg.into());
    notify(
        server,
        actor,
        &Msg::new("fed_other").plain(target.get_name()).into(),
    );
    admin.audit(server, actor, "feed", Some(&who(target)), "");
    Ok(())
}

pub fn fly(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    let mut abilities = target.get_abilities();
    let on = !abilities.allow_flying;
    abilities.allow_flying = on;
    abilities.flying = abilities.flying && on;
    target.set_abilities(abilities);
    let (self_key, other_key) = if on {
        ("fly_on_self", "fly_on_other")
    } else {
        ("fly_off_self", "fly_off_other")
    };
    let (own, msg) = self_or_other(actor, target, self_key, other_key);
    if !own {
        tell(target, &Msg::new(self_key), None);
    }
    actor.tell(&msg.into());
    notify(
        server,
        actor,
        &Msg::new(other_key).plain(target.get_name()).into(),
    );
    admin.audit(
        server,
        actor,
        if on { "fly-on" } else { "fly-off" },
        Some(&who(target)),
        "",
    );
    Ok(())
}

#[must_use]
pub const fn mode_key(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Survival => "gameMode.survival",
        GameMode::Creative => "gameMode.creative",
        GameMode::Adventure => "gameMode.adventure",
        GameMode::Spectator => "gameMode.spectator",
    }
}

const fn mode_of(mode: GameMode) -> Mode {
    match mode {
        GameMode::Survival => Mode::Survival,
        GameMode::Creative => Mode::Creative,
        GameMode::Adventure => Mode::Adventure,
        GameMode::Spectator => Mode::Spectator,
    }
}

const fn game_mode(mode: Mode) -> GameMode {
    match mode {
        Mode::Survival => GameMode::Survival,
        Mode::Creative => GameMode::Creative,
        Mode::Adventure => GameMode::Adventure,
        Mode::Spectator => GameMode::Spectator,
    }
}

/// `/gmc` and the others: vanilla's `/gamemode`, with vanilla's own messages. A player already
/// in that mode is left alone and nothing is said, as vanilla does.
pub fn gamemode(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    target: &Player,
    mode: GameMode,
) -> Outcome {
    if target.get_gamemode() == mode || !target.set_gamemode(mode) {
        return Ok(());
    }
    let key = mode_key(mode);
    let line = if id_of(target) == actor.id {
        Line::vanilla("commands.gamemode.success.self", vec![VArg::Key(key)])
    } else {
        let changed = Line::vanilla("gameMode.changed", vec![VArg::Key(key)]);
        target.send_system_message(
            changed.component(lang::language(&target.get_locale())),
            false,
        );
        Line::vanilla(
            "commands.gamemode.success.other",
            vec![VArg::Text(target.get_name()), VArg::Key(key)],
        )
    };
    actor.tell(&line);
    notify(server, actor, &line);
    admin.audit(
        server,
        actor,
        "gamemode",
        Some(&who(target)),
        mode_of(mode).code(),
    );
    Ok(())
}

pub fn vanish(admin: &Arc<Admin>, server: &Server, actor: &Actor) -> Outcome {
    let Some(player) = &actor.player else {
        return Err(Msg::new("only_players").into());
    };
    let was_vanished = lock(&admin.staff).vanished.remove(&actor.id);
    let (own, notice, action) = if was_vanished {
        vanish::reveal(server, player);
        ("vanish_off", "vanish_notify_off", "unvanish")
    } else {
        vanish::hide(server, player);
        lock(&admin.staff).vanished.insert(actor.id.clone());
        ("vanish_on", "vanish_notify_on", "vanish")
    };
    admin.staff_changed(server);
    actor.tell(&Msg::new(own).into());
    staff_notice(server, &actor.id, &Msg::new(notice).plain(&actor.name));
    admin.audit(server, actor, action, None, "");
    Ok(())
}

/// A notice to the online holders of `admin-lite:notify` but one, in grey italics.
pub fn staff_notice(server: &Server, except_id: &str, msg: &Msg) {
    for viewer in server.get_all_players() {
        if id_of(&viewer) != except_id && viewer.has_permission(crate::perms::NOTIFY) {
            let text = TextComponent::text(&msg.render(lang::language(&viewer.get_locale())))
                .color_named(NamedColor::Gray)
                .italic(true);
            viewer.send_system_message(text, false);
        }
    }
}

/// The freeze screen: a title, a subtitle and a chat line.
pub fn show_frozen(player: &Player) {
    let language = lang::language(&player.get_locale());
    player.send_title_animation(10, 100, 20);
    player.show_subtitle(TextComponent::text(
        &Msg::new("frozen_subtitle").render(language),
    ));
    player.show_title(
        TextComponent::text(&Msg::new("frozen_title").render(language))
            .color_named(NamedColor::Aqua),
    );
    tell(player, &Msg::new("frozen_chat"), Some(NamedColor::Aqua));
}

/// Locks a frozen player in place (the server puts him back after every move) and makes him
/// invulnerable while he waits.
pub fn lock_in_place(player: &Player) {
    player.set_movement_locked(true);
    player.set_invulnerable(true);
}

/// `/freeze`: freezes the player, or unfreezes him when he is frozen.
pub fn freeze(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    let (id, name) = (id_of(target), target.get_name());
    if lock(&admin.staff).frozen.contains_key(&id) {
        return unfreeze(admin, server, actor, &name);
    }
    guard(admin, server, actor, &id, &name)?;
    let was_invulnerable = target.get_abilities().invulnerable;
    lock(&admin.staff).frozen.insert(
        id,
        Frozen {
            name: name.clone(),
            was_invulnerable,
        },
    );
    admin.staff_changed(server);
    lock_in_place(target);
    show_frozen(target);
    let done = Line::from(Msg::new("freeze_done").plain(&name));
    actor.tell(&done);
    notify(server, actor, &done);
    admin.audit(server, actor, "freeze", Some(&who(target)), "");
    Ok(())
}

/// `/unfreeze`: works for a player who is offline too.
pub fn unfreeze(admin: &Arc<Admin>, server: &Server, actor: &Actor, name: &str) -> Outcome {
    let found = {
        let mut staff = lock(&admin.staff);
        let id = staff
            .frozen
            .iter()
            .find(|(_, f)| f.name.eq_ignore_ascii_case(name))
            .map(|(id, _)| id.clone());
        id.and_then(|id| staff.frozen.remove(&id).map(|f| (id, f)))
    };
    let Some((id, frozen)) = found else {
        return Err(Msg::new("not_frozen").plain(shown(name)).into());
    };
    admin.staff_changed(server);
    if let Some(player) = by_id(server, &id) {
        player.set_movement_locked(false);
        player.set_invulnerable(frozen.was_invulnerable);
        tell(&player, &Msg::new("unfrozen"), Some(NamedColor::Green));
    }
    let done = Line::from(Msg::new("unfreeze_done").plain(&frozen.name));
    actor.tell(&done);
    notify(server, actor, &done);
    let target = Who {
        name: frozen.name,
        id,
    };
    admin.audit(server, actor, "unfreeze", Some(&target), "");
    Ok(())
}

fn teleported(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    moved: &Player,
    to: &Player,
    action: &str,
) {
    let line = Line::vanilla(
        "commands.teleport.success.entity.single",
        vec![VArg::Text(moved.get_name()), VArg::Text(to.get_name())],
    );
    actor.tell(&line);
    notify(server, actor, &line);
    admin.audit(
        server,
        actor,
        action,
        Some(&who(if action == "tpto" { to } else { moved })),
        "",
    );
}

/// `/tpto`: the staff member goes to the player.
pub fn tp_to(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    let Some(staff) = &actor.player else {
        return Err(Msg::new("only_players").into());
    };
    if id_of(target) == actor.id {
        return Err(Msg::new("self_target").into());
    }
    staff.teleport(
        target.get_position(),
        Some(target.get_yaw()),
        Some(target.get_pitch()),
        target.get_world(),
    );
    teleported(admin, server, actor, staff, target, "tpto");
    Ok(())
}

/// `/tphere`: the player comes to the staff member.
pub fn bring(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    let Some(staff) = &actor.player else {
        return Err(Msg::new("only_players").into());
    };
    if id_of(target) == actor.id {
        return Err(Msg::new("self_target").into());
    }
    target.teleport(
        staff.get_position(),
        Some(staff.get_yaw()),
        Some(staff.get_pitch()),
        staff.get_world(),
    );
    teleported(admin, server, actor, target, staff, "tphere");
    Ok(())
}

/// `/watch <player>`: spectator mode, at the player, through his eyes. The place and game mode
/// before are kept in `state.tsv`, so `/watch` alone, a rejoin or a restart brings him back.
pub fn watch(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Player) -> Outcome {
    let Some(staff) = &actor.player else {
        return Err(Msg::new("only_players").into());
    };
    let target_id = id_of(target);
    if target_id == actor.id {
        return Err(Msg::new("self_target").into());
    }
    let earlier = lock(&admin.staff).watching.get(&actor.id).cloned();
    let mut back = earlier.unwrap_or_else(|| Watch {
        target_id: String::new(),
        mode: mode_of(staff.get_gamemode()),
        world: staff.get_world().get_name(),
        pos: staff.get_position(),
        yaw: staff.get_yaw(),
        pitch: staff.get_pitch(),
    });
    back.target_id.clone_from(&target_id);
    lock(&admin.staff).watching.insert(actor.id.clone(), back);
    admin.staff_changed(server);
    staff.set_gamemode(GameMode::Spectator);
    staff.teleport(
        target.get_position(),
        Some(target.get_yaw()),
        Some(target.get_pitch()),
        target.get_world(),
    );
    // The camera after the client has the target's entity.
    let (admin_ref, staff_id, staff_uuid, target_uuid) = (
        Arc::clone(admin),
        actor.id.clone(),
        staff.get_id(),
        target.get_id(),
    );
    server.schedule_delayed_task(5, move |server| {
        let still = lock(&admin_ref.staff)
            .watching
            .get(&staff_id)
            .is_some_and(|w| w.target_id == id_text(target_uuid));
        if let (true, Some(staff), Some(target)) = (
            still,
            server.get_player_by_uuid(staff_uuid),
            server.get_player_by_uuid(target_uuid),
        ) {
            staff.set_camera(Some(target.as_entity()));
        }
    });
    actor.tell(&Msg::new("watch_start").plain(target.get_name()).into());
    admin.audit(server, actor, "watch", Some(&who(target)), "");
    Ok(())
}

fn id_text(uuid: Uuid) -> String {
    uuid.to_string()
}

/// Back to where watching started.
pub fn restore(server: &Server, staff: &Player, back: &Watch) {
    staff.reset_camera();
    staff.set_gamemode(game_mode(back.mode));
    if let Some(world) = server.get_world_by_name(&back.world) {
        staff.teleport(back.pos, Some(back.yaw), Some(back.pitch), world);
    }
}

/// `/watch` alone.
pub fn unwatch(admin: &Arc<Admin>, server: &Server, actor: &Actor) -> Outcome {
    let Some(staff) = &actor.player else {
        return Err(Msg::new("only_players").into());
    };
    let Some(back) = lock(&admin.staff).watching.remove(&actor.id) else {
        return Err(Msg::new("watch_none").into());
    };
    admin.staff_changed(server);
    restore(server, staff, &back);
    actor.tell(&Msg::new("watch_stop").into());
    admin.audit(server, actor, "unwatch", None, "");
    Ok(())
}

pub fn warn(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    target: &Target,
    reason: &str,
) -> Outcome {
    guard(admin, server, actor, &target.id, &target.name)?;
    let mut entry = admin::entry(Kind::Warn, actor, &target.id, &target.name, None, reason);
    entry.pending = target.player.is_none();
    let record = admin.record(server, actor, entry);
    if let Some(player) = &target.player {
        tell(
            player,
            &with_reason(Msg::new("warned_target"), &record.reason),
            Some(NamedColor::Red),
        );
    }
    let done = Line::from(with_reason(
        Msg::new("warn_done").plain(&target.name),
        &record.reason,
    ));
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

pub fn mute(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    target: &Target,
    seconds: u64,
    reason: &str,
) -> Outcome {
    guard(admin, server, actor, &target.id, &target.name)?;
    let record = admin.record(
        server,
        actor,
        admin::entry(
            Kind::Mute,
            actor,
            &target.id,
            &target.name,
            Some(seconds),
            reason,
        ),
    );
    if let Some(player) = &target.player {
        tell(
            player,
            &with_reason(Msg::new("muted_target").duration(seconds), &record.reason),
            Some(NamedColor::Red),
        );
    }
    let done = Line::from(with_reason(
        Msg::new("mute_done").plain(&target.name).duration(seconds),
        &record.reason,
    ));
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

pub fn unmute(admin: &Arc<Admin>, server: &Server, actor: &Actor, target: &Target) -> Outcome {
    if actor.id == target.id {
        return Err(Msg::new("self_target").into());
    }
    if lock(&admin.history)
        .mute_of(&target.id, duration::now())
        .is_none()
    {
        return Err(Msg::new("not_muted").plain(&target.name).into());
    }
    admin.record(
        server,
        actor,
        admin::entry(Kind::Unmute, actor, &target.id, &target.name, None, ""),
    );
    if let Some(player) = &target.player {
        tell(player, &Msg::new("unmuted_target"), Some(NamedColor::Green));
    }
    let done = Line::from(Msg::new("unmute_done").plain(&target.name));
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

/// Disconnects a player with a reason, or with vanilla's "Kicked by an operator." without one.
pub fn kick_player(player: &Player, reason: &str) {
    if let Some(java) = player.as_java() {
        let text = if reason.is_empty() {
            TextComponent::translate("multiplayer.disconnect.kicked", Vec::new())
        } else {
            TextComponent::text(reason)
        };
        java.kick(JavaKickOptions::new(text));
    } else if let Some(bedrock) = player.as_bedrock() {
        let text = if reason.is_empty() {
            Msg::new("kick_message").render(lang::language(&player.get_locale()))
        } else {
            reason.to_owned()
        };
        bedrock.kick(&BedrockKickOptions::new(
            BedrockDisconnectReason::Kicked,
            text,
        ));
    }
}

/// The staff menu's kick (vanilla `/kick` stays vanilla and is written into the history too).
pub fn kick(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    target: &Player,
    reason: &str,
) -> Outcome {
    let (id, name) = (id_of(target), target.get_name());
    guard(admin, server, actor, &id, &name)?;
    let record = admin.record(
        server,
        actor,
        admin::entry(Kind::Kick, actor, &id, &name, None, reason),
    );
    admin.expect_kick(&name, None);
    kick_player(target, &record.reason);
    let shown_reason = if record.reason.is_empty() {
        VArg::Key("multiplayer.disconnect.kicked")
    } else {
        VArg::Text(record.reason.clone())
    };
    let done = Line::vanilla(
        "commands.kick.success",
        vec![VArg::Text(name), shown_reason],
    );
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

/// `/tempban`: the server's own ban list, with an end. Vanilla `/ban` stays vanilla.
pub fn tempban(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    target: &Target,
    seconds: u64,
    reason: &str,
) -> Outcome {
    guard(admin, server, actor, &target.id, &target.name)?;
    let Some(uuid) = admin::uuid_from(&target.id) else {
        return Err(Msg::new("unknown_player").plain(&target.name).into());
    };
    let record = admin.record(
        server,
        actor,
        admin::entry(
            Kind::TempBan,
            actor,
            &target.id,
            &target.name,
            Some(seconds),
            reason,
        ),
    );
    let options = BanPlayerOptions {
        reason: (!record.reason.is_empty()).then(|| TextComponent::text(&record.reason)),
        source: Some(actor.name.clone()),
        expires_at_utc: None,
        duration_seconds: Some(seconds),
        kick_if_online: true,
        log_to_console: true,
    };
    admin.expect_kick(&target.name, None);
    lock(&admin.known_bans).insert(target.id.clone());
    server
        .get_ban_manager()
        .ban_player(&target.name, uuid, options);
    let done = Line::from(with_reason(
        Msg::new("ban_done").plain(&target.name).duration(seconds),
        &record.reason,
    ));
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

/// The server's ban of the player last called `name` (any case): his UUID and name.
#[must_use]
pub fn ban_by_name(server: &Server, name: &str) -> Option<(Uuid, String)> {
    server
        .get_ban_manager()
        .list_player_bans()
        .into_iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name))
        .map(|entry| (entry.uuid, entry.name))
}

/// `/unban`: lifts a ban of the server's list (a vanilla `/ban` or a `/tempban`), with vanilla's
/// `/pardon` messages.
pub fn unban(admin: &Arc<Admin>, server: &Server, actor: &Actor, name: &str) -> Outcome {
    let failed = || Line::vanilla("commands.pardon.failed", Vec::new());
    if !text::valid_name(name) {
        return Err(failed());
    }
    let Some((uuid, banned_name)) = ban_by_name(server, name) else {
        return Err(failed());
    };
    if !server.get_ban_manager().unban_player(uuid) {
        return Err(failed());
    }
    lock(&admin.known_bans).remove(&uuid.to_string());
    admin.record(
        server,
        actor,
        admin::entry(
            Kind::Unban,
            actor,
            &uuid.to_string(),
            &banned_name,
            None,
            "",
        ),
    );
    let done = Line::vanilla("commands.pardon.success", vec![VArg::Text(banned_name)]);
    actor.tell(&done);
    notify(server, actor, &done);
    Ok(())
}

pub const HISTORY_PER_PAGE: usize = 8;

fn history_line(record: &Record) -> Msg {
    let msg = Msg::new(if record.duration.is_some() {
        "history_line_timed"
    } else {
        "history_line"
    })
    .plain(record.id.to_string())
    .plain(duration::stamp(record.at))
    .key(record.kind.label());
    let msg = match record.duration {
        Some(seconds) => msg.duration(seconds),
        None => msg,
    };
    with_reason(msg.plain(&record.staff_name), &record.reason)
}

/// `/history <player> [page]`: the player's records, newest first.
pub fn history(admin: &Admin, server: &Server, actor: &Actor, name: &str, page: usize) -> Outcome {
    let (id, target_name) = match known(admin, server, name) {
        Ok(target) => (target.id, target.name),
        Err(unknown) => match ban_by_name(server, name).filter(|_| text::valid_name(name)) {
            Some((uuid, banned_name)) => (uuid.to_string(), banned_name),
            None => return Err(unknown),
        },
    };
    let (records, pages) = lock(&admin.history).page(&id, page, HISTORY_PER_PAGE);
    if records.is_empty() {
        if page <= 1 {
            actor.tell(&Msg::new("history_empty").plain(&target_name).into());
            return Ok(());
        }
        return Err(Msg::new("history_header")
            .plain(&target_name)
            .plain(page.to_string())
            .plain(pages.to_string())
            .into());
    }
    actor.tell(
        &Msg::new("history_header")
            .plain(&target_name)
            .plain(page.to_string())
            .plain(pages.to_string())
            .into(),
    );
    for record in &records {
        actor.tell(&history_line(record).into());
    }
    if page < pages {
        actor.tell(
            &Msg::new("history_next")
                .plain(&target_name)
                .plain((page + 1).to_string())
                .into(),
        );
    }
    Ok(())
}

/// A vanilla `/kick`, `/ban` or `/pardon` was typed. Nothing is replaced: vanilla answers as
/// vanilla does. A kick or a ban of an online player is written into the history when its kick
/// happens ([`kicked`]), so a command that failed, or a kick another plugin cancelled, writes
/// nothing. A ban of an offline player and a pardon are checked against the ban list a second
/// later.
pub fn observe_vanilla(admin: &Arc<Admin>, server: &Server, staff: Who, observed: Observed) {
    let admin = Arc::clone(admin);
    match observed {
        Observed::Kick { target, reason } => {
            if let Some(player) = server.get_player_by_name(&target) {
                let pending = Pending {
                    staff,
                    kind: Kind::Kick,
                    reason,
                };
                admin.expect_kick(&player.get_name(), Some(pending));
            }
        }
        Observed::Ban { target, reason } => {
            if ban_by_name(server, &target).is_some() {
                return;
            }
            if let Some(player) = server.get_player_by_name(&target) {
                let pending = Pending {
                    staff,
                    kind: Kind::Ban,
                    reason,
                };
                admin.expect_kick(&player.get_name(), Some(pending));
                return;
            }
            server.schedule_delayed_task(20, move |server| {
                if let Some((uuid, name)) = ban_by_name(&server, &target)
                    && lock(&admin.known_bans).insert(uuid.to_string())
                {
                    let entry = vanilla_entry(Kind::Ban, &staff, &uuid.to_string(), &name, &reason);
                    admin.record_vanilla(&server, &staff, entry);
                }
            });
        }
        Observed::Pardon { target } => {
            let Some((uuid, name)) = ban_by_name(server, &target) else {
                return;
            };
            server.schedule_delayed_task(20, move |server| {
                if !server.get_ban_manager().is_player_banned(uuid) {
                    lock(&admin.known_bans).remove(&uuid.to_string());
                    let entry = vanilla_entry(Kind::Unban, &staff, &uuid.to_string(), &name, "");
                    admin.record_vanilla(&server, &staff, entry);
                }
            });
        }
    }
}

fn vanilla_entry(kind: Kind, staff: &Who, id: &str, name: &str, reason: &str) -> Entry {
    Entry {
        kind,
        target_id: id.to_owned(),
        target_name: name.to_owned(),
        staff_id: staff.id.clone(),
        staff_name: staff.name.clone(),
        duration: None,
        pending: false,
        reason: text::clean_reason(reason),
    }
}

/// A player was kicked. One of admin-lite's own kicks is already in the history; a vanilla `/kick`
/// or `/ban` seen in a command line is written now. Otherwise, if he is now on the server's ban
/// list and was not before (a ban by another plugin or a path the plugin did not see), it is
/// written with the ban list's own source and reason. Other kicks are not punishments (a time-out,
/// a login plugin's wrong password) and are not written.
pub fn kicked(admin: &Arc<Admin>, server: &Server, player: &Player) {
    let (uuid, id, name) = (player.get_id(), id_of(player), player.get_name());
    match admin.take_expected_kick(&name) {
        // admin-lite's own kick or ban: already in the history.
        Some(None) => return,
        Some(Some(pending)) if pending.kind == Kind::Kick => {
            let entry = vanilla_entry(Kind::Kick, &pending.staff, &id, &name, &pending.reason);
            admin.record_vanilla(server, &pending.staff, entry);
            return;
        }
        Some(Some(pending)) => {
            let admin = Arc::clone(admin);
            server.schedule_delayed_task(2, move |server| {
                if server.get_ban_manager().is_player_banned(uuid)
                    && lock(&admin.known_bans).insert(id.clone())
                {
                    let entry =
                        vanilla_entry(Kind::Ban, &pending.staff, &id, &name, &pending.reason);
                    admin.record_vanilla(&server, &pending.staff, entry);
                }
            });
            return;
        }
        None => {}
    }
    if lock(&admin.known_bans).contains(&id) {
        return;
    }
    let admin = Arc::clone(admin);
    server.schedule_delayed_task(2, move |server| {
        let Some(ban) = server.get_ban_manager().get_player_ban(uuid) else {
            return;
        };
        if !lock(&admin.known_bans).insert(id.clone()) {
            return;
        }
        let staff_id = lock(&admin.seen)
            .lookup(&ban.source)
            .map_or_else(|| "unknown".to_owned(), |(id, _)| id);
        let staff = Who {
            name: ban.source.clone(),
            id: staff_id,
        };
        let kind = if ban.expires.is_some() {
            Kind::TempBan
        } else {
            Kind::Ban
        };
        admin.record_vanilla(
            &server,
            &staff,
            vanilla_entry(kind, &staff, &id, &name, &ban.reason),
        );
    });
}

/// After a load: the bans already on the server's list are not new.
pub fn read_known_bans(admin: &Admin, server: &Server) {
    let bans: std::collections::HashSet<String> = server
        .get_ban_manager()
        .list_player_bans()
        .into_iter()
        .map(|ban| ban.uuid.to_string())
        .collect();
    *lock(&admin.known_bans) = bans;
}
