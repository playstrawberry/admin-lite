//! Vanish: real hiding, through the server's `hide-player`. A vanished staff member is gone for
//! everyone who may not see vanished staff: no body, no armour, no held item, no tab-list entry,
//! not in `/list` or in command suggestions. No potion effect is involved.
//!
//! Holders of `admin-lite:vanish.see` keep seeing him as before.

use strawberry_plugin_api::{Player, Server, uuid::Uuid};

use crate::perms;

/// The SDK's UUID record has no ==.
fn same(a: &Uuid, b: &Uuid) -> bool {
    a.high == b.high && a.low == b.low
}

/// Hides `staff` from every online player who may not see vanished staff, and shows him to
/// the others.
pub fn hide(server: &Server, staff: &Player) {
    let id = staff.get_id();
    for viewer in server.get_all_players() {
        if same(&viewer.get_id(), &id) {
            continue;
        }
        let Some(handle) = server.get_player_by_uuid(id) else {
            return;
        };
        if viewer.has_permission(perms::VANISH_SEE) {
            viewer.show_player(handle);
        } else {
            viewer.hide_player(handle);
        }
    }
}

/// Shows `staff` to everyone again.
pub fn reveal(server: &Server, staff: &Player) {
    let id = staff.get_id();
    for viewer in server.get_all_players() {
        if same(&viewer.get_id(), &id) {
            continue;
        }
        if let Some(handle) = server.get_player_by_uuid(id) {
            viewer.show_player(handle);
        }
    }
}

/// A player joined: the vanished staff online are hidden from him unless he may see them.
pub fn hide_from(server: &Server, newcomer: &Player, vanished: &[Player]) {
    let sees = newcomer.has_permission(perms::VANISH_SEE);
    for staff in vanished {
        if same(&staff.get_id(), &newcomer.get_id()) {
            continue;
        }
        if let Some(handle) = server.get_player_by_uuid(staff.get_id()) {
            if sees {
                newcomer.show_player(handle);
            } else {
                newcomer.hide_player(handle);
            }
        }
    }
}
