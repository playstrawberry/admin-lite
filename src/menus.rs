//! The windows: the staff menu (online players), the player menu (actions on one player), and
//! the read-only inventory and ender chest views. Layout in `menu`.
//!
//! The views are copies taken when they open (the SDK gives copies of a player's items, and no
//! window bound to another player's inventory): nothing can be taken out or put in, and
//! Refresh takes a new copy. Every click in one of these windows is cancelled.
//!
//! A click acts one tick later, in a task: opening the next window from inside the click event
//! would re-enter the plugin in the middle of it. Opening a window closes the old one first,
//! which fires the close event (it re-enters the plugin and forgets the old window); the new one
//! is noted after `open-gui` returns.

use std::sync::Arc;

use strawberry_plugin_api::{
    ItemStack, Player, Screen, Server, common::NamedColor, gui::Gui, scheduler::SchedulerExt,
    text::TextComponent,
};

use crate::{
    actions::{self, Target},
    admin::{Actor, Admin, Line, id_of, lock},
    duration,
    lang::{self, Msg},
    menu::{self, Action, InvSlot},
    perms,
};

/// The window a staff member has open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// The page (from 0) and the players shown on it, as (UUID, name).
    Staff {
        page: usize,
        shown: Vec<(String, String)>,
    },
    Player {
        id: String,
        name: String,
    },
    Inventory {
        id: String,
        name: String,
    },
    Ender {
        id: String,
        name: String,
    },
}

impl View {
    const fn screen(&self) -> Screen {
        match self {
            Self::Staff { .. } => Screen::Generic9x6,
            Self::Player { .. } => Screen::Generic9x3,
            Self::Inventory { .. } => Screen::Generic9x5,
            Self::Ender { .. } => Screen::Generic9x4,
        }
    }

    const fn size(&self) -> u32 {
        match self {
            Self::Staff { .. } => menu::STAFF_MENU_SIZE,
            Self::Player { .. } => menu::PLAYER_MENU_SIZE,
            Self::Inventory { .. } => menu::INV_VIEW_SIZE,
            Self::Ender { .. } => menu::ENDER_VIEW_SIZE,
        }
    }
}

fn language_of(player: &Player) -> &'static str {
    lang::language(&player.get_locale())
}

/// An item with a name and grey lore lines, not in italics (as vanilla shows renamed items).
fn icon(item: &str, name: &str, lore: &[String]) -> ItemStack {
    let stack = ItemStack::new(item, 1);
    stack.set_custom_name(Some(
        TextComponent::text(name)
            .italic(false)
            .color_named(NamedColor::White),
    ));
    if !lore.is_empty() {
        stack.set_lore(
            lore.iter()
                .map(|line| {
                    TextComponent::text(line)
                        .italic(false)
                        .color_named(NamedColor::Gray)
                })
                .collect(),
        );
    }
    stack
}

/// What a staff member should know about a player at a glance.
fn states(admin: &Admin, id: &str, language: &str) -> Vec<String> {
    let mut out = Vec::new();
    {
        let staff = lock(&admin.staff);
        if staff.vanished.contains(id) {
            out.push(Msg::new("state_vanished").render(language));
        }
        if staff.frozen.contains_key(id) {
            out.push(Msg::new("state_frozen").render(language));
        }
    }
    if lock(&admin.history).mute_of(id, duration::now()).is_some() {
        out.push(Msg::new("state_muted").render(language));
    }
    out
}

fn show(admin: &Admin, staff: &Player, gui: Gui, view: View) {
    gui.set_allow_grab_items(false);
    gui.set_allow_put_items(false);
    staff.open_gui(gui);
    lock(&admin.menus).insert(id_of(staff), view);
}

/// The staff menu: the online players, 45 a page.
pub fn open_staff(admin: &Admin, server: &Server, staff: &Player, page: usize) {
    let language = language_of(staff);
    let mut players: Vec<(String, String)> = server
        .get_all_players()
        .iter()
        .map(|p| (id_of(p), p.get_name()))
        .collect();
    players.sort_by_key(|(_, name)| name.to_ascii_lowercase());
    let pages = menu::pages(players.len());
    let page = page.min(pages - 1);
    let shown: Vec<(String, String)> = players[menu::page_range(players.len(), page)].to_vec();
    let title = Msg::new("menu_staff_title")
        .plain(players.len().to_string())
        .render(language);
    let gui = Gui::new(Screen::Generic9x6, TextComponent::text(&title));
    for (slot, (id, name)) in shown.iter().enumerate() {
        let head = icon("minecraft:player_head", name, &states(admin, id, language));
        gui.set_item(u32::try_from(slot).unwrap_or(0), head);
    }
    if page > 0 {
        gui.set_item(
            menu::PREVIOUS_SLOT,
            icon(
                "minecraft:arrow",
                &Msg::new("act_prev").render(language),
                &[],
            ),
        );
    }
    if page + 1 < pages {
        gui.set_item(
            menu::NEXT_SLOT,
            icon(
                "minecraft:arrow",
                &Msg::new("act_next").render(language),
                &[],
            ),
        );
    }
    show(admin, staff, gui, View::Staff { page, shown });
}

/// The actions on one player; each button only for staff who hold its node.
pub fn open_player(admin: &Admin, staff: &Player, id: &str, name: &str) {
    let language = language_of(staff);
    let title = Msg::new("menu_player_title").plain(name).render(language);
    let gui = Gui::new(Screen::Generic9x3, TextComponent::text(&title));
    gui.set_item(
        4,
        icon("minecraft:player_head", name, &states(admin, id, language)),
    );
    for button in &menu::PLAYER_MENU {
        if button.node.is_none_or(|node| staff.has_permission(node)) {
            gui.set_item(
                button.slot,
                icon(button.icon, &Msg::new(button.label).render(language), &[]),
            );
        }
    }
    let view = View::Player {
        id: id.to_owned(),
        name: name.to_owned(),
    };
    show(admin, staff, gui, view);
}

fn view_buttons(gui: &Gui, language: &str, refresh: u32, back: u32) {
    let copy_note = [Msg::new("view_copy").render(language)];
    gui.set_item(
        refresh,
        icon(
            "minecraft:clock",
            &Msg::new("act_refresh").render(language),
            &copy_note,
        ),
    );
    gui.set_item(
        back,
        icon(
            "minecraft:arrow",
            &Msg::new("act_back").render(language),
            &[],
        ),
    );
}

/// A copy of the player's inventory: main, hotbar, armour and off hand.
pub fn open_inventory(admin: &Admin, staff: &Player, target: &Player) {
    let language = language_of(staff);
    let title = Msg::new("menu_inv_title")
        .plain(target.get_name())
        .render(language);
    let gui = Gui::new(Screen::Generic9x5, TextComponent::text(&title));
    let inventory = target.get_inventory();
    for slot in 0..menu::INV_VIEW_SIZE {
        let item = match menu::inv_slot(slot) {
            Some(InvSlot::Main(index)) => target.get_inventory_item(index),
            Some(InvSlot::Helmet) => inventory.get_helmet(),
            Some(InvSlot::Chestplate) => inventory.get_chestplate(),
            Some(InvSlot::Leggings) => inventory.get_leggings(),
            Some(InvSlot::Boots) => inventory.get_boots(),
            Some(InvSlot::OffHand) => inventory.get_off_hand(),
            Some(InvSlot::Refresh | InvSlot::Back) | None => None,
        };
        if let Some(item) = item {
            gui.set_item(slot, item);
        }
    }
    view_buttons(&gui, language, 43, 44);
    let view = View::Inventory {
        id: id_of(target),
        name: target.get_name(),
    };
    show(admin, staff, gui, view);
}

/// A copy of the player's ender chest.
pub fn open_ender(admin: &Admin, staff: &Player, target: &Player) {
    let language = language_of(staff);
    let title = Msg::new("menu_ender_title")
        .plain(target.get_name())
        .render(language);
    let gui = Gui::new(Screen::Generic9x4, TextComponent::text(&title));
    for slot in 0..27u8 {
        if let Some(item) = target.get_ender_chest_item(slot) {
            gui.set_item(u32::from(slot), item);
        }
    }
    view_buttons(
        &gui,
        language,
        menu::ENDER_REFRESH_SLOT,
        menu::ENDER_BACK_SLOT,
    );
    let view = View::Ender {
        id: id_of(target),
        name: target.get_name(),
    };
    show(admin, staff, gui, view);
}

/// The view a click landed in, if it is one of the plugin's windows. A noted window that does
/// not match the window clicked is forgotten (the click is then not the plugin's).
#[must_use]
pub fn clicked_view(admin: &Admin, staff_id: &str, window: Option<Screen>) -> Option<View> {
    let mut menus = lock(&admin.menus);
    let view = menus.get(staff_id)?.clone();
    if window == Some(view.screen()) {
        Some(view)
    } else {
        menus.remove(staff_id);
        None
    }
}

/// Runs a click, one tick after it.
pub fn plan_click(admin: &Arc<Admin>, server: &Server, staff: &Player, view: View, slot: i16) {
    let Ok(slot) = u32::try_from(slot) else {
        return;
    };
    if slot >= view.size() {
        return;
    }
    let (admin, staff_uuid) = (Arc::clone(admin), staff.get_id());
    server.schedule_delayed_task(1, move |server| {
        if let Some(staff) = server.get_player_by_uuid(staff_uuid) {
            click(&admin, &server, staff, &view, slot);
        }
    });
}

fn click(admin: &Arc<Admin>, server: &Server, staff: Player, view: &View, slot: u32) {
    if !staff.has_permission(perms::MENU) {
        return;
    }
    match view {
        View::Staff { page, shown } => {
            if slot == menu::PREVIOUS_SLOT && *page > 0 {
                open_staff(admin, server, &staff, page - 1);
            } else if slot == menu::NEXT_SLOT {
                open_staff(admin, server, &staff, page + 1);
            } else if let Some((id, name)) = shown.get(slot as usize) {
                open_player(admin, &staff, id, name);
            }
        }
        View::Player { id, name } => {
            if let Some(button) = menu::button_at(slot)
                && button.node.is_none_or(|node| staff.has_permission(node))
            {
                player_action(admin, server, staff, button.action, id, name);
            }
        }
        View::Inventory { id, name } | View::Ender { id, name } => {
            let inventory = matches!(view, View::Inventory { .. });
            let (refresh, back) = if inventory {
                (43, 44)
            } else {
                (menu::ENDER_REFRESH_SLOT, menu::ENDER_BACK_SLOT)
            };
            if slot == back {
                open_player(admin, &staff, id, name);
            } else if slot == refresh {
                match actions::by_id(server, id) {
                    Some(target) if inventory => open_inventory(admin, &staff, &target),
                    Some(target) => open_ender(admin, &staff, &target),
                    None => crate::admin::tell(
                        &staff,
                        &Msg::new("not_online").plain(name),
                        Some(NamedColor::Red),
                    ),
                }
            }
        }
    }
}

fn player_action(
    admin: &Arc<Admin>,
    server: &Server,
    staff: Player,
    action: Action,
    id: &str,
    name: &str,
) {
    let staff_uuid = staff.get_id();
    let actor = Actor::of_player(staff);
    let language = actor.language();
    let target = actions::by_id(server, id);
    let offline = || Line::from(Msg::new("not_online").plain(name));
    let known = || Target {
        id: id.to_owned(),
        name: name.to_owned(),
        player: actions::by_id(server, id),
    };
    let reopen = || {
        if let Some(staff) = server.get_player_by_uuid(staff_uuid) {
            open_player(admin, &staff, id, name);
        }
    };
    let outcome = match action {
        Action::Back => {
            if let Some(staff) = server.get_player_by_uuid(staff_uuid) {
                open_staff(admin, server, &staff, 0);
            }
            return;
        }
        Action::Inventory | Action::EnderChest => {
            match (target, server.get_player_by_uuid(staff_uuid)) {
                (Some(target), Some(staff)) if action == Action::Inventory => {
                    open_inventory(admin, &staff, &target)
                }
                (Some(target), Some(staff)) => open_ender(admin, &staff, &target),
                _ => actor.tell(&offline()),
            }
            return;
        }
        Action::TeleportTo => target
            .ok_or_else(offline)
            .and_then(|t| actions::tp_to(admin, server, &actor, &t)),
        Action::Bring => target
            .ok_or_else(offline)
            .and_then(|t| actions::bring(admin, server, &actor, &t)),
        Action::Watch => target
            .ok_or_else(offline)
            .and_then(|t| actions::watch(admin, server, &actor, &t)),
        Action::Freeze => match target {
            Some(t) => actions::freeze(admin, server, &actor, &t),
            None => actions::unfreeze(admin, server, &actor, name),
        },
        Action::Heal => target
            .ok_or_else(offline)
            .and_then(|t| actions::heal(admin, server, &actor, &t)),
        Action::Feed => target
            .ok_or_else(offline)
            .and_then(|t| actions::feed(admin, server, &actor, &t)),
        Action::Warn => actions::warn(
            admin,
            server,
            &actor,
            &known(),
            &Msg::new("menu_reason").render(language),
        ),
        Action::Mute => actions::mute(
            admin,
            server,
            &actor,
            &known(),
            menu::MENU_MUTE_SECONDS,
            &Msg::new("menu_reason").render(language),
        ),
        Action::Unmute => actions::unmute(admin, server, &actor, &known()),
        Action::Kick => target
            .ok_or_else(offline)
            .and_then(|t| actions::kick(admin, server, &actor, &t, "")),
        Action::TempBan => actions::tempban(
            admin,
            server,
            &actor,
            &known(),
            menu::MENU_BAN_SECONDS,
            &Msg::new("menu_reason").render(language),
        ),
        Action::History => actions::history(admin, server, &actor, name, 1),
    };
    if let Err(line) = outcome
        && let Some(staff) = &actor.player
    {
        staff.send_system_message(line.component(language).color_named(NamedColor::Red), false);
    }
    // Teleporting and watching move the staff member away: the menu is not opened again.
    if !matches!(action, Action::TeleportTo | Action::Watch | Action::History) {
        reopen();
    }
}
