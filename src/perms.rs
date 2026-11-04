//! Every permission node of the plugin. Each staff action has its own node; the defaults follow
//! vanilla's operator levels (level 2 for the cheat-like tools, level 3 for kick and bans, as
//! vanilla's `/kick` and `/ban`), so a server without a permissions plugin behaves as vanilla
//! would for its operators.

/// Who holds a node when nothing explicit is set: nobody, or operators of this level and up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    Op(u8),
    /// Every player.
    All,
}

pub const MENU: &str = "admin-lite:menu";
pub const INVSEE: &str = "admin-lite:invsee";
pub const ENDERSEE: &str = "admin-lite:endersee";
pub const VANISH: &str = "admin-lite:vanish";
pub const VANISH_SEE: &str = "admin-lite:vanish.see";
pub const FREEZE: &str = "admin-lite:freeze";
pub const TP: &str = "admin-lite:tp";
pub const BRING: &str = "admin-lite:bring";
pub const WATCH: &str = "admin-lite:watch";
pub const HEAL: &str = "admin-lite:heal";
pub const HEAL_OTHERS: &str = "admin-lite:heal.others";
pub const FEED: &str = "admin-lite:feed";
pub const FEED_OTHERS: &str = "admin-lite:feed.others";
pub const FLY: &str = "admin-lite:fly";
pub const FLY_OTHERS: &str = "admin-lite:fly.others";
pub const GAMEMODE: &str = "admin-lite:gamemode";
pub const GAMEMODE_OTHERS: &str = "admin-lite:gamemode.others";
pub const WARN: &str = "admin-lite:warn";
pub const MUTE: &str = "admin-lite:mute";
pub const KICK: &str = "admin-lite:kick";
pub const TEMPBAN: &str = "admin-lite:tempban";
pub const UNBAN: &str = "admin-lite:unban";
pub const HISTORY: &str = "admin-lite:history";
pub const NOTIFY: &str = "admin-lite:notify";
pub const EXEMPT: &str = "admin-lite:exempt";
/// Who reads public punishment announcements: everyone by default.
/// Taking it from everyone turns the announcements off until the plugin settings format exists.
pub const ANNOUNCEMENTS: &str = "admin-lite:announcements";

/// `(node, description for the server owner, default)`.
pub const ALL: [(&str, &str, Default); 26] = [
    (MENU, "Open the staff menu with /staff", Default::Op(2)),
    (
        INVSEE,
        "View a player's inventory with /invsee",
        Default::Op(2),
    ),
    (
        ENDERSEE,
        "View a player's ender chest with /endersee",
        Default::Op(2),
    ),
    (VANISH, "Vanish with /vanish", Default::Op(2)),
    (VANISH_SEE, "See vanished staff", Default::Op(2)),
    (
        FREEZE,
        "Freeze and unfreeze players with /freeze",
        Default::Op(2),
    ),
    (TP, "Teleport to a player with /tpto", Default::Op(2)),
    (BRING, "Bring a player with /tphere", Default::Op(2)),
    (WATCH, "Watch a player with /watch", Default::Op(2)),
    (HEAL, "Heal yourself with /heal", Default::Op(2)),
    (HEAL_OTHERS, "Heal other players", Default::Op(2)),
    (FEED, "Feed yourself with /feed", Default::Op(2)),
    (FEED_OTHERS, "Feed other players", Default::Op(2)),
    (FLY, "Toggle your flight with /fly", Default::Op(2)),
    (FLY_OTHERS, "Toggle other players' flight", Default::Op(2)),
    (
        GAMEMODE,
        "Change your game mode with /gmc, /gms, /gma, /gmsp",
        Default::Op(2),
    ),
    (
        GAMEMODE_OTHERS,
        "Change other players' game mode",
        Default::Op(2),
    ),
    (WARN, "Warn players with /warn", Default::Op(2)),
    (
        MUTE,
        "Mute and unmute players with /mute and /unmute",
        Default::Op(2),
    ),
    (KICK, "Kick players from the staff menu", Default::Op(3)),
    (
        TEMPBAN,
        "Ban players for a time with /tempban",
        Default::Op(3),
    ),
    (UNBAN, "Unban players with /unban", Default::Op(3)),
    (
        HISTORY,
        "Read punishment history with /history",
        Default::Op(2),
    ),
    (NOTIFY, "Receive staff notices", Default::Op(2)),
    (
        EXEMPT,
        "Cannot be punished or frozen by other staff",
        Default::Op(4),
    ),
    (
        ANNOUNCEMENTS,
        "Read public punishment announcements",
        Default::All,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_are_unique_and_namespaced() {
        let mut nodes: Vec<&str> = ALL.iter().map(|(node, _, _)| *node).collect();
        assert!(nodes.iter().all(|node| node.starts_with("admin-lite:")));
        nodes.sort_unstable();
        nodes.dedup();
        assert_eq!(nodes.len(), ALL.len());
        assert!(ALL.iter().all(|(_, about, default)| !about.is_empty()
            && (matches!(default, Default::All)
                || matches!(default, Default::Op(level) if *level <= 4))));
    }

    #[test]
    fn kick_and_bans_need_vanillas_level_three() {
        for node in [KICK, TEMPBAN, UNBAN] {
            let (_, _, default) = ALL.iter().find(|(n, _, _)| *n == node).copied().unwrap();
            assert_eq!(default, Default::Op(3), "{node}");
        }
    }
}
