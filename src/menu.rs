//! Where things sit in the staff menus. Pure layout; `menus.rs` builds the windows.
//!
//! - **Staff menu** (6 rows): the online players, 45 a page, with page buttons in the last row.
//! - **Player menu** (3 rows): the actions on one player, each shown only to staff who hold its
//!   permission node.
//! - **Inventory view** (5 rows): the main inventory, the hotbar, the armour and the off hand,
//!   a read-only copy.
//! - **Ender chest view** (4 rows): the 27 slots, a read-only copy.

use crate::perms;

/// One action of the player menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Back,
    Inventory,
    EnderChest,
    TeleportTo,
    Bring,
    Watch,
    Freeze,
    Heal,
    Feed,
    Warn,
    Mute,
    Unmute,
    Kick,
    TempBan,
    History,
}

/// A button: its slot, action, item, label key and the node that shows it (`None`: always).
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub slot: u32,
    pub action: Action,
    pub icon: &'static str,
    pub label: &'static str,
    pub node: Option<&'static str>,
}

const fn button(
    slot: u32,
    action: Action,
    icon: &'static str,
    label: &'static str,
    node: Option<&'static str>,
) -> Button {
    Button {
        slot,
        action,
        icon,
        label,
        node,
    }
}

pub const PLAYER_MENU_SIZE: u32 = 27;

pub const PLAYER_MENU: [Button; 15] = [
    button(18, Action::Back, "minecraft:arrow", "act_back", None),
    button(
        10,
        Action::Inventory,
        "minecraft:chest",
        "act_inventory",
        Some(perms::INVSEE),
    ),
    button(
        11,
        Action::EnderChest,
        "minecraft:ender_chest",
        "act_ender",
        Some(perms::ENDERSEE),
    ),
    button(
        12,
        Action::TeleportTo,
        "minecraft:ender_pearl",
        "act_tpto",
        Some(perms::TP),
    ),
    button(
        13,
        Action::Bring,
        "minecraft:lead",
        "act_bring",
        Some(perms::BRING),
    ),
    button(
        14,
        Action::Watch,
        "minecraft:ender_eye",
        "act_watch",
        Some(perms::WATCH),
    ),
    button(
        15,
        Action::Freeze,
        "minecraft:packed_ice",
        "act_freeze",
        Some(perms::FREEZE),
    ),
    button(
        16,
        Action::Heal,
        "minecraft:golden_apple",
        "act_heal",
        Some(perms::HEAL_OTHERS),
    ),
    button(
        19,
        Action::Feed,
        "minecraft:cooked_beef",
        "act_feed",
        Some(perms::FEED_OTHERS),
    ),
    button(
        20,
        Action::Warn,
        "minecraft:paper",
        "act_warn",
        Some(perms::WARN),
    ),
    button(
        21,
        Action::Mute,
        "minecraft:book",
        "act_mute",
        Some(perms::MUTE),
    ),
    button(
        22,
        Action::Unmute,
        "minecraft:milk_bucket",
        "act_unmute",
        Some(perms::MUTE),
    ),
    button(
        23,
        Action::Kick,
        "minecraft:iron_door",
        "act_kick",
        Some(perms::KICK),
    ),
    button(
        24,
        Action::TempBan,
        "minecraft:barrier",
        "act_tempban",
        Some(perms::TEMPBAN),
    ),
    button(
        25,
        Action::History,
        "minecraft:writable_book",
        "act_history",
        Some(perms::HISTORY),
    ),
];

/// The mute the player menu gives: one hour.
pub const MENU_MUTE_SECONDS: u64 = 3_600;
/// The ban the player menu gives: one day.
pub const MENU_BAN_SECONDS: u64 = 86_400;

/// The player-menu button at `slot`.
#[must_use]
pub fn button_at(slot: u32) -> Option<&'static Button> {
    PLAYER_MENU.iter().find(|button| button.slot == slot)
}

pub const STAFF_MENU_SIZE: u32 = 54;
pub const PLAYERS_PER_PAGE: usize = 45;
pub const PREVIOUS_SLOT: u32 = 45;
pub const NEXT_SLOT: u32 = 53;

/// The number of pages (at least 1) for `players` players.
#[must_use]
pub fn pages(players: usize) -> usize {
    players.div_ceil(PLAYERS_PER_PAGE).max(1)
}

/// The players (by index in the sorted list) on page `page` (from 0).
#[must_use]
pub fn page_range(players: usize, page: usize) -> std::ops::Range<usize> {
    let start = page.saturating_mul(PLAYERS_PER_PAGE).min(players);
    start..(start + PLAYERS_PER_PAGE).min(players)
}

/// What a slot of the inventory view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvSlot {
    /// The player's inventory slot (0 to 8 hotbar, 9 to 35 main, as vanilla numbers them).
    Main(u8),
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    OffHand,
    Refresh,
    Back,
}

pub const INV_VIEW_SIZE: u32 = 45;
pub const ENDER_VIEW_SIZE: u32 = 36;

/// What the inventory view shows at `slot`.
#[must_use]
pub fn inv_slot(slot: u32) -> Option<InvSlot> {
    let narrow = |v: u32| u8::try_from(v).ok();
    Some(match slot {
        0..=26 => InvSlot::Main(narrow(slot + 9)?),
        27..=35 => InvSlot::Main(narrow(slot - 27)?),
        36 => InvSlot::Helmet,
        37 => InvSlot::Chestplate,
        38 => InvSlot::Leggings,
        39 => InvSlot::Boots,
        40 => InvSlot::OffHand,
        43 => InvSlot::Refresh,
        44 => InvSlot::Back,
        _ => return None,
    })
}

/// The ender chest view's refresh and back buttons.
pub const ENDER_REFRESH_SLOT: u32 = ENDER_VIEW_SIZE - 2;
pub const ENDER_BACK_SLOT: u32 = ENDER_VIEW_SIZE - 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_menu_buttons_have_their_own_slots() {
        let mut slots: Vec<u32> = PLAYER_MENU.iter().map(|b| b.slot).collect();
        assert!(slots.iter().all(|slot| *slot < PLAYER_MENU_SIZE));
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), PLAYER_MENU.len());
        for button in &PLAYER_MENU {
            assert_eq!(
                button_at(button.slot).map(|b| b.action),
                Some(button.action)
            );
            assert!(button.icon.starts_with("minecraft:"));
        }
        assert!(button_at(0).is_none());
        assert!(button_at(PLAYER_MENU_SIZE).is_none());
    }

    #[test]
    fn every_button_label_has_a_text() {
        let labels = PLAYER_MENU.iter().map(|b| b.label);
        let others = ["act_refresh", "act_prev", "act_next", "view_copy"];
        for key in labels.chain(others) {
            assert_ne!(crate::lang::raw("en", key), key, "{key}");
        }
    }

    #[test]
    fn the_staff_menu_pages_by_45() {
        assert_eq!(pages(0), 1);
        assert_eq!(pages(45), 1);
        assert_eq!(pages(46), 2);
        assert_eq!(page_range(46, 0), 0..45);
        assert_eq!(page_range(46, 1), 45..46);
        assert_eq!(page_range(46, 2), 46..46);
        assert_eq!(page_range(0, 0), 0..0);
        assert!(PREVIOUS_SLOT >= PLAYERS_PER_PAGE as u32 && NEXT_SLOT < STAFF_MENU_SIZE);
    }

    #[test]
    fn the_inventory_view_shows_every_slot_once() {
        let mut mains: Vec<u8> = (0..INV_VIEW_SIZE)
            .filter_map(inv_slot)
            .filter_map(|s| {
                if let InvSlot::Main(i) = s {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        mains.sort_unstable();
        assert_eq!(mains, (0..36).collect::<Vec<u8>>());
        assert_eq!(inv_slot(0), Some(InvSlot::Main(9)));
        assert_eq!(inv_slot(27), Some(InvSlot::Main(0)));
        assert_eq!(inv_slot(36), Some(InvSlot::Helmet));
        assert_eq!(inv_slot(40), Some(InvSlot::OffHand));
        assert_eq!(inv_slot(43), Some(InvSlot::Refresh));
        assert_eq!(inv_slot(44), Some(InvSlot::Back));
        assert_eq!(inv_slot(45), None);
        assert!(ENDER_REFRESH_SLOT >= 27 && ENDER_BACK_SLOT < ENDER_VIEW_SIZE);
    }
}
