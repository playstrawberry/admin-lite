//! The events the plugin listens to, as few as the tools need. Every handler answers fast (a
//! map lookup for a player who is not muted, frozen, vanished or in a menu) and does no file
//! work. Every blocking handler lets the event go on unchanged if the plugin fails
//! (`FailureVerdict::Allow`): a broken staff plugin must never stop every chat line, command or
//! click on the server.
//!
//! - chat (`PlayerChatEvent`, lowest): a muted player's line is cancelled;
//! - commands (`PlayerCommandSendEvent`, lowest): the server's gate before a player's command
//!   runs. A frozen player's commands (but private messages) and a muted player's
//!   talking commands are cancelled; a vanilla `/kick`, `/ban` or `/pardon` is noted for the
//!   history. A login plugin's password commands reach that plugin alone, so this handler never
//!   sees a password;
//! - console commands (`ServerCommandEvent`, observed only): a vanilla `/kick`, `/ban` or
//!   `/pardon` from the console is noted for the history;
//! - kicks (`PlayerKickEvent`, observed only): a player kicked by a vanilla `/ban` the plugin
//!   did not see is found on the ban list and written into the history;
//! - server list ping: vanished staff online are left out of the player count;
//! - join and leave: vanish, freeze, watch and warnings carry across; a vanished staff member's
//!   join and leave lines are not shown;
//! - inventory click and close: the plugin's windows.

use std::sync::Arc;

use strawberry_plugin_api::{
    Context, EventHandler, Player, Result, Server,
    common::NamedColor,
    events::{
        FailureVerdict, HandlerOptions, HandlerPriority, InventoryClickEvent, InventoryCloseEvent,
        PlayerChatEvent, PlayerCommandSendEvent, PlayerJoinEvent, PlayerKickEvent,
        PlayerLeaveEvent, ServerCommandEvent, ServerListPingEvent,
    },
    events_wit::{
        InventoryClickEventData, InventoryCloseEventData, PlayerChatEventData,
        PlayerCommandSendEventData, PlayerJoinEventData, PlayerKickEventData, PlayerLeaveEventData,
        ServerCommandEventData, ServerListPingEventData,
    },
    scheduler::SchedulerExt,
};

use crate::{
    actions,
    admin::{Admin, id_of, lock, tell},
    audit::Who,
    cmdline, duration,
    history::CONSOLE_ID,
    lang::Msg,
    menus, perms, vanish,
};

fn options(priority: HandlerPriority, blocking: bool, ignore_cancelled: bool) -> HandlerOptions {
    HandlerOptions {
        priority,
        blocking,
        ignore_cancelled,
        on_failure: FailureVerdict::Allow,
    }
}

struct Chat(Arc<Admin>);

impl EventHandler<PlayerChatEvent> for Chat {
    fn handle(&self, _server: Server, mut event: PlayerChatEventData) -> PlayerChatEventData {
        let id = id_of(&event.player);
        let mute = lock(&self.0.history).mute_of(&id, duration::now());
        if let Some(mute) = mute {
            event.cancelled = true;
            let msg = match mute.left(duration::now()) {
                Some(left) => Msg::new("muted_chat").duration(left.max(1)),
                None => Msg::new("muted_chat").plain("-"),
            };
            tell(&event.player, &msg, Some(NamedColor::Red));
        }
        event
    }
}

struct Commands(Arc<Admin>);

impl EventHandler<PlayerCommandSendEvent> for Commands {
    fn handle(
        &self,
        server: Server,
        mut event: PlayerCommandSendEventData,
    ) -> PlayerCommandSendEventData {
        let label = cmdline::label(&event.command);
        let id = id_of(&event.player);
        let frozen = lock(&self.0.staff).frozen.contains_key(&id);
        if frozen && !cmdline::allowed_when_frozen(&label) {
            event.cancelled = true;
            tell(
                &event.player,
                &Msg::new("frozen_command"),
                Some(NamedColor::Red),
            );
            return event;
        }
        if cmdline::blocked_when_muted(&label)
            && lock(&self.0.history)
                .mute_of(&id, duration::now())
                .is_some()
        {
            event.cancelled = true;
            tell(
                &event.player,
                &Msg::new("muted_command"),
                Some(NamedColor::Red),
            );
            return event;
        }
        if let Some(observed) = cmdline::observe(&event.command) {
            let staff = Who {
                name: event.player.get_name(),
                id,
            };
            actions::observe_vanilla(&self.0, &server, staff, observed);
        }
        event
    }
}

struct Console(Arc<Admin>);

impl EventHandler<ServerCommandEvent> for Console {
    fn handle(&self, server: Server, event: ServerCommandEventData) -> ServerCommandEventData {
        if let Some(observed) = cmdline::observe(&event.command) {
            let staff = Who {
                name: "Server".to_owned(),
                id: CONSOLE_ID.to_owned(),
            };
            actions::observe_vanilla(&self.0, &server, staff, observed);
        }
        event
    }
}

/// The vanished staff online, but `except`.
fn vanished_online(admin: &Admin, server: &Server, except: &str) -> Vec<Player> {
    let ids: Vec<String> = lock(&admin.staff)
        .vanished
        .iter()
        .filter(|id| *id != except)
        .cloned()
        .collect();
    ids.iter()
        .filter_map(|id| actions::by_id(server, id))
        .collect()
}

/// What a player brings along when he joins or is online when the plugin loads: the seen
/// index, the exempt flag, vanish and freeze. Returns whether he is vanished.
fn carry_over(admin: &Arc<Admin>, server: &Server, player: &Player) -> bool {
    let (id, name) = (id_of(player), player.get_name());
    // A player who can join is not banned (a ban lifted by a command the plugin did not see).
    lock(&admin.known_bans).remove(&id);
    if lock(&admin.seen).see(&id, &name, duration::now()) {
        admin.seen_changed(server);
    }
    let exempt = player.has_permission(perms::EXEMPT);
    let exempt_changed = {
        let mut staff = lock(&admin.staff);
        if exempt {
            staff.exempt.insert(id.clone())
        } else {
            staff.exempt.remove(&id)
        }
    };
    if exempt_changed {
        admin.staff_changed(server);
    }
    vanish::hide_from(server, player, &vanished_online(admin, server, &id));
    let (vanished, frozen) = {
        let staff = lock(&admin.staff);
        (staff.vanished.contains(&id), staff.frozen.contains_key(&id))
    };
    if vanished {
        vanish::hide(server, player);
    }
    if frozen {
        actions::lock_in_place(player);
    }
    vanished
}

struct ListPing(Arc<Admin>);

impl EventHandler<ServerListPingEvent> for ListPing {
    fn handle(
        &self,
        server: Server,
        mut event: ServerListPingEventData,
    ) -> ServerListPingEventData {
        let hidden = vanished_online(&self.0, &server, "").len();
        event.num_players = event
            .num_players
            .saturating_sub(u32::try_from(hidden).unwrap_or(u32::MAX));
        event
    }
}

struct Join(Arc<Admin>);

impl EventHandler<PlayerJoinEvent> for Join {
    fn handle(&self, server: Server, mut event: PlayerJoinEventData) -> PlayerJoinEventData {
        let admin = &self.0;
        let player = &event.player;
        let (id, name, uuid) = (id_of(player), player.get_name(), player.get_id());
        let vanished = carry_over(admin, &server, player);
        if vanished {
            event.cancelled = true;
        }
        let frozen = lock(&admin.staff).frozen.contains_key(&id);
        let watching = lock(&admin.staff).watching.get(&id).cloned();
        let warnings = lock(&admin.history).take_pending(&id);
        if !warnings.is_empty() {
            admin.history_changed(&server);
        }
        if frozen {
            actions::staff_notice(&server, &id, &Msg::new("frozen_join").plain(&name));
        }
        // The messages and the way back once the client has the world.
        let admin_ref = Arc::clone(admin);
        server.schedule_delayed_task(20, move |server| {
            let Some(player) = server.get_player_by_uuid(uuid) else {
                return;
            };
            if vanished {
                tell(&player, &Msg::new("vanish_rejoin"), Some(NamedColor::Gray));
            }
            if frozen && lock(&admin_ref.staff).frozen.contains_key(&id_of(&player)) {
                actions::show_frozen(&player);
            }
            if let Some(back) = &watching {
                let still = lock(&admin_ref.staff).watching.remove(&id_of(&player));
                if still.is_some() {
                    admin_ref.staff_changed(&server);
                    actions::restore(&server, &player, back);
                    tell(&player, &Msg::new("watch_restored"), Some(NamedColor::Gray));
                }
            }
            for warning in &warnings {
                let msg = if warning.reason.is_empty() {
                    Msg::new("warned_target_away").key("no_reason")
                } else {
                    Msg::new("warned_target_away").plain(&warning.reason)
                };
                tell(&player, &msg, Some(NamedColor::Red));
            }
        });
        event
    }
}

struct Leave(Arc<Admin>);

impl EventHandler<PlayerLeaveEvent> for Leave {
    fn handle(&self, server: Server, mut event: PlayerLeaveEventData) -> PlayerLeaveEventData {
        let admin = &self.0;
        let (id, name) = (id_of(&event.player), event.player.get_name());
        lock(&admin.menus).remove(&id);
        let (vanished, frozen, watchers) = {
            let staff = lock(&admin.staff);
            (
                staff.vanished.contains(&id),
                staff.frozen.contains_key(&id),
                staff.watchers_of(&id),
            )
        };
        if vanished {
            event.cancelled = true;
        }
        if frozen {
            actions::staff_notice(&server, &id, &Msg::new("frozen_quit").plain(&name));
        }
        for watcher in watchers.iter().filter_map(|w| actions::by_id(&server, w)) {
            watcher.reset_camera();
            tell(
                &watcher,
                &Msg::new("watch_target_left").plain(&name),
                Some(NamedColor::Gray),
            );
        }
        event
    }
}

struct Kicked(Arc<Admin>);

impl EventHandler<PlayerKickEvent> for Kicked {
    fn handle(&self, server: Server, event: PlayerKickEventData) -> PlayerKickEventData {
        if !event.cancelled {
            actions::kicked(&self.0, &server, &event.player);
        }
        event
    }
}

struct Click(Arc<Admin>);

impl EventHandler<InventoryClickEvent> for Click {
    fn handle(
        &self,
        server: Server,
        mut event: InventoryClickEventData,
    ) -> InventoryClickEventData {
        let id = id_of(&event.player);
        let Some(view) = menus::clicked_view(&self.0, &id, event.window_type) else {
            return event;
        };
        // Nothing moves in the plugin's windows, whatever was clicked.
        event.cancelled = true;
        menus::plan_click(&self.0, &server, &event.player, view, event.slot);
        event
    }
}

struct Close(Arc<Admin>);

impl EventHandler<InventoryCloseEvent> for Close {
    fn handle(&self, _server: Server, event: InventoryCloseEventData) -> InventoryCloseEventData {
        lock(&self.0.menus).remove(&id_of(&event.player));
        event
    }
}

pub fn register(context: &Context, admin: &Arc<Admin>) -> Result<()> {
    let a = || Arc::clone(admin);
    context.register_event_handler_with_options::<PlayerChatEvent, _>(
        Chat(a()),
        options(HandlerPriority::Lowest, true, true),
    )?;
    context.register_event_handler_with_options::<PlayerCommandSendEvent, _>(
        Commands(a()),
        options(HandlerPriority::Lowest, true, true),
    )?;
    context.register_event_handler_with_options::<ServerCommandEvent, _>(
        Console(a()),
        options(HandlerPriority::Monitor, false, true),
    )?;
    context.register_event_handler_with_options::<PlayerKickEvent, _>(
        Kicked(a()),
        options(HandlerPriority::Monitor, false, true),
    )?;
    context.register_event_handler_with_options::<ServerListPingEvent, _>(
        ListPing(a()),
        options(HandlerPriority::Normal, true, false),
    )?;
    context.register_event_handler_with_options::<PlayerJoinEvent, _>(
        Join(a()),
        options(HandlerPriority::High, true, false),
    )?;
    context.register_event_handler_with_options::<PlayerLeaveEvent, _>(
        Leave(a()),
        options(HandlerPriority::High, true, false),
    )?;
    context.register_event_handler_with_options::<InventoryClickEvent, _>(
        Click(a()),
        options(HandlerPriority::Normal, true, false),
    )?;
    context.register_event_handler_with_options::<InventoryCloseEvent, _>(
        Close(a()),
        options(HandlerPriority::Monitor, true, false),
    )?;
    Ok(())
}

/// After a load (a reload, or a fresh instance after a fault): the players online now get their
/// vanish and freeze back.
pub fn reapply(admin: &Arc<Admin>, server: &Server) {
    actions::read_known_bans(admin, server);
    for player in server.get_all_players() {
        carry_over(admin, server, &player);
    }
}
