//! admin-lite (first-party): tools for a server's staff.
//!
//! - inventory and ender chest views, a staff menu and a menu per player;
//! - vanish, freeze, teleport to and bring, watch (spectator through the player's eyes);
//! - heal, feed, fly and game mode shortcuts;
//! - warnings, temporary mutes, temporary bans on the server's own ban list, unban, and a
//!   persistent punishment history that also records vanilla `/kick`, `/ban` and `/pardon`.
//!
//! Every action has its own permission node (`perms`) and writes an audit line to the data
//! folder (`audit`). The plugin never asks the server for a player's IP address, and an address
//! typed into a reason is shown as `[IP]`.
//!
//! Who acted is taken from the command sender, which another plugin can forge today (it can run
//! a command as a player or as the console), so permissions are the guard.

pub mod actions;
pub mod admin;
pub mod announce;
pub mod audit;
pub mod cmdline;
pub mod commands;
pub mod duration;
pub mod handlers;
pub mod history;
pub mod lang;
pub mod menu;
pub mod menus;
pub mod perms;
pub mod seen;
pub mod staffstate;
pub mod text;
pub mod vanish;

use std::sync::Arc;

use strawberry_plugin_api::{
    Context, Plugin, PluginMetadata, Result,
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions, register_plugin,
    scheduler::SchedulerExt,
};

use crate::admin::Admin;

const fn level(level: u8) -> PermissionLevel {
    match level {
        0 => PermissionLevel::Zero,
        1 => PermissionLevel::One,
        2 => PermissionLevel::Two,
        3 => PermissionLevel::Three,
        _ => PermissionLevel::Four,
    }
}

struct AdminLite {
    admin: Arc<Admin>,
}

impl Plugin for AdminLite {
    fn new() -> Self {
        Self {
            admin: Arc::new(Admin::default()),
        }
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "admin-lite".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Strawberry Interactive LLC".into()],
            description: "Staff tools: inventory views, vanish, freeze, teleport, watch, warnings, mutes, bans, history.".into(),
            dependencies: Vec::new(),
            permissions: vec![permissions::FS_READ_DATA.into(), permissions::FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        self.admin.load()?;
        for (node, description, default) in perms::ALL {
            context.register_permission(&Permission {
                node: node.into(),
                description: description.into(),
                default: match default {
                    perms::Default::Op(op) => PermissionDefault::Op(level(op)),
                    perms::Default::All => PermissionDefault::Allow,
                },
                children: Vec::new(),
            })?;
        }
        commands::register(&context, &self.admin);
        handlers::register(&context, &self.admin)?;
        let admin = Arc::clone(&self.admin);
        // Off the tick: at start the plugin's first run costs a few hundred milliseconds.
        context.schedule_delayed_async_task(1, move |server| handlers::reapply(&admin, &server));
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        self.admin.flush();
        Ok(())
    }
}

register_plugin!(AdminLite);
