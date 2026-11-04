//! The commands. Vanilla's own commands stay vanilla (`/kick`, `/ban`, `/pardon`, `/gamemode`,
//! `/tp`, `/spectate`): these are shortcuts and the staff tools vanilla lacks, each behind its
//! own permission node.
//!
//! | Command | Node |
//! | --- | --- |
//! | `/staff [page]` | `admin-lite:menu` |
//! | `/invsee <player>`, `/endersee <player>` | `admin-lite:invsee`, `admin-lite:endersee` |
//! | `/vanish` | `admin-lite:vanish` |
//! | `/freeze <player>` (again to unfreeze), `/unfreeze <player>` | `admin-lite:freeze` |
//! | `/tpto <player>`, `/tphere <player>` | `admin-lite:tp`, `admin-lite:bring` |
//! | `/watch <player>`, `/watch` | `admin-lite:watch` |
//! | `/heal`, `/feed`, `/fly` `[player]` | `admin-lite:heal` (`.others`), `feed`, `fly` |
//! | `/gmc`, `/gms`, `/gma`, `/gmsp` `[player]` | `admin-lite:gamemode` (`.others`) |
//! | `/warn <player> <reason>` | `admin-lite:warn` |
//! | `/mute <player> <duration> [reason]`, `/unmute <player>` | `admin-lite:mute` |
//! | `/tempban <player> <duration> [reason]` | `admin-lite:tempban` |
//! | `/unban <player>` | `admin-lite:unban` |
//! | `/history <player> [page]` | `admin-lite:history` |

use std::sync::Arc;

use strawberry_plugin_api::{
    Context, Player, Server,
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    command_wit::Number,
    commands::{CommandHandler, CommandSuggestionHandler},
    common::GameMode,
};

use crate::{
    actions::{self, Outcome},
    admin::{Actor, Admin, Line, failure, id_of},
    duration,
    lang::Msg,
    menus, perms, text,
};

#[derive(Clone, Copy)]
pub enum Cmd {
    Staff,
    Invsee,
    Endersee,
    Vanish,
    Freeze,
    Unfreeze,
    TpTo,
    TpHere,
    Watch,
    Heal,
    Feed,
    Fly,
    Gm(GameMode),
    Warn,
    Mute,
    Unmute,
    TempBan,
    Unban,
    History,
}

struct Run {
    admin: Arc<Admin>,
    cmd: Cmd,
}

impl CommandHandler for Run {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let actor = Actor::of_sender(&sender);
        match run(&self.admin, &server, &actor, self.cmd, &args) {
            Ok(Ok(())) => Ok(1),
            Ok(Err(line)) => Err(failure(&actor, &line)),
            Err(denied) => Err(denied),
        }
    }
}

/// An argument's text, or `None`. The host gives an empty string for a missing argument.
fn arg(args: &ConsumedArgs, key: &str) -> Option<String> {
    match args.get_value(key) {
        Arg::Simple(s) | Arg::Msg(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

fn page(args: &ConsumedArgs) -> usize {
    match args.get_value("page") {
        Arg::Num(Ok(Number::Int32(value))) => usize::try_from(value).unwrap_or(1).max(1),
        _ => 1,
    }
}

fn need(args: &ConsumedArgs, key: &str) -> Result<String, Line> {
    arg(args, key).ok_or_else(|| Msg::new("not_online").plain("?").into())
}

fn only_player(actor: &Actor) -> Result<&Player, Line> {
    actor
        .player
        .as_ref()
        .ok_or_else(|| Msg::new("only_players").into())
}

fn seconds(args: &ConsumedArgs) -> Result<u64, Line> {
    let typed = arg(args, "duration").unwrap_or_default();
    duration::parse(&typed).ok_or_else(|| {
        Msg::new("bad_duration")
            .plain(text::clean_reason(&typed))
            .into()
    })
}

/// `[player]`: the actor himself, or another player when he holds the `.others` node.
fn self_or_named(
    server: &Server,
    actor: &Actor,
    args: &ConsumedArgs,
    others: &str,
) -> Result<Result<Player, Line>, CommandError> {
    let Some(name) = arg(args, "player") else {
        return Ok(match &actor.player {
            Some(player) => server
                .get_player_by_uuid(player.get_id())
                .ok_or_else(|| Msg::new("only_players").into()),
            None => Err(Msg::new("only_players").into()),
        });
    };
    let target = match actions::online(server, &name) {
        Ok(target) => target,
        Err(line) => return Ok(Err(line)),
    };
    if id_of(&target) != actor.id && !actor.has(others) {
        return Err(CommandError::PermissionDenied);
    }
    Ok(Ok(target))
}

fn run(
    admin: &Arc<Admin>,
    server: &Server,
    actor: &Actor,
    cmd: Cmd,
    args: &ConsumedArgs,
) -> Result<Outcome, CommandError> {
    let named_online = || need(args, "player").and_then(|name| actions::online(server, &name));
    let named_known = || need(args, "player").and_then(|name| actions::known(admin, server, &name));
    let reason = || arg(args, "reason").unwrap_or_default();
    Ok(match cmd {
        Cmd::Staff => {
            only_player(actor).map(|staff| menus::open_staff(admin, server, staff, page(args) - 1))
        }
        Cmd::Invsee => only_player(actor)
            .and_then(|staff| named_online().map(|t| menus::open_inventory(admin, staff, &t))),
        Cmd::Endersee => only_player(actor)
            .and_then(|staff| named_online().map(|t| menus::open_ender(admin, staff, &t))),
        Cmd::Vanish => actions::vanish(admin, server, actor),
        Cmd::Freeze => named_online().and_then(|t| actions::freeze(admin, server, actor, &t)),
        Cmd::Unfreeze => {
            need(args, "player").and_then(|name| actions::unfreeze(admin, server, actor, &name))
        }
        Cmd::TpTo => named_online().and_then(|t| actions::tp_to(admin, server, actor, &t)),
        Cmd::TpHere => named_online().and_then(|t| actions::bring(admin, server, actor, &t)),
        Cmd::Watch => match arg(args, "player") {
            Some(name) => actions::online(server, &name)
                .and_then(|t| actions::watch(admin, server, actor, &t)),
            None => actions::unwatch(admin, server, actor),
        },
        Cmd::Heal => self_or_named(server, actor, args, perms::HEAL_OTHERS)?
            .and_then(|t| actions::heal(admin, server, actor, &t)),
        Cmd::Feed => self_or_named(server, actor, args, perms::FEED_OTHERS)?
            .and_then(|t| actions::feed(admin, server, actor, &t)),
        Cmd::Fly => self_or_named(server, actor, args, perms::FLY_OTHERS)?
            .and_then(|t| actions::fly(admin, server, actor, &t)),
        Cmd::Gm(mode) => self_or_named(server, actor, args, perms::GAMEMODE_OTHERS)?
            .and_then(|t| actions::gamemode(admin, server, actor, &t, mode)),
        Cmd::Warn => named_known().and_then(|t| actions::warn(admin, server, actor, &t, &reason())),
        Cmd::Mute => named_known().and_then(|t| {
            seconds(args).and_then(|s| actions::mute(admin, server, actor, &t, s, &reason()))
        }),
        Cmd::Unmute => named_known().and_then(|t| actions::unmute(admin, server, actor, &t)),
        Cmd::TempBan => named_known().and_then(|t| {
            seconds(args).and_then(|s| actions::tempban(admin, server, actor, &t, s, &reason()))
        }),
        Cmd::Unban => {
            need(args, "player").and_then(|name| actions::unban(admin, server, actor, &name))
        }
        Cmd::History => need(args, "player")
            .and_then(|name| actions::history(admin, server, actor, &name, page(args))),
    })
}

/// Online names for a player argument.
struct Names;

impl CommandSuggestionHandler for Names {
    fn suggest(
        &self,
        _sender: CommandSender,
        server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let start = request.start as usize;
        let prefix = request
            .input
            .get(start..)
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut values: Vec<CommandSuggestion> = server
            .get_all_players()
            .iter()
            .map(Player::get_name)
            .filter(|name| name.to_ascii_lowercase().starts_with(&prefix))
            .map(|value| CommandSuggestion {
                value,
                tooltip: None,
            })
            .collect();
        values.truncate(100);
        CommandSuggestions {
            start: request.start,
            length: u32::try_from(request.input.len().saturating_sub(start)).unwrap_or(0),
            values,
        }
    }
}

fn word() -> ArgumentType {
    ArgumentType::String(StringType::SingleWord)
}

fn greedy() -> ArgumentType {
    ArgumentType::String(StringType::Greedy)
}

/// Registers every command.
pub fn register(context: &Context, admin: &Arc<Admin>) {
    let run = |cmd: Cmd| Run {
        admin: Arc::clone(admin),
        cmd,
    };
    let names = |name: &str| vec![name.to_owned()];
    let player = || CommandNode::argument("player", &word()).suggest(Names);
    let page_arg =
        || CommandNode::argument("page", &ArgumentType::Integer((Some(1), Some(10_000))));

    let staff = Command::new(&names("staff"), "Open the staff menu.")
        .execute(run(Cmd::Staff))
        .then(page_arg().execute(run(Cmd::Staff)));
    context.register_command(staff, perms::MENU);

    let one_player = [
        (
            "invsee",
            "View a player's inventory.",
            Cmd::Invsee,
            perms::INVSEE,
        ),
        (
            "endersee",
            "View a player's ender chest.",
            Cmd::Endersee,
            perms::ENDERSEE,
        ),
        (
            "freeze",
            "Freeze a player, or unfreeze him.",
            Cmd::Freeze,
            perms::FREEZE,
        ),
        (
            "unfreeze",
            "Unfreeze a player.",
            Cmd::Unfreeze,
            perms::FREEZE,
        ),
        ("tpto", "Teleport to a player.", Cmd::TpTo, perms::TP),
        (
            "tphere",
            "Bring a player to you.",
            Cmd::TpHere,
            perms::BRING,
        ),
        (
            "unmute",
            "Let a muted player chat again.",
            Cmd::Unmute,
            perms::MUTE,
        ),
        ("unban", "Lift a player's ban.", Cmd::Unban, perms::UNBAN),
    ];
    for (name, about, cmd, node) in one_player {
        let command = Command::new(&names(name), about).then(player().execute(run(cmd)));
        context.register_command(command, node);
    }

    let vanish =
        Command::new(&names("vanish"), "Vanish, or show yourself again.").execute(run(Cmd::Vanish));
    context.register_command(vanish, perms::VANISH);

    let watch = Command::new(
        &names("watch"),
        "Watch a player as a spectator; alone, go back.",
    )
    .execute(run(Cmd::Watch))
    .then(player().execute(run(Cmd::Watch)));
    context.register_command(watch, perms::WATCH);

    let self_or_other = [
        ("heal", "Restore health.", Cmd::Heal, perms::HEAL),
        ("feed", "Fill the food bar.", Cmd::Feed, perms::FEED),
        ("fly", "Toggle flight.", Cmd::Fly, perms::FLY),
        (
            "gmc",
            "Creative mode.",
            Cmd::Gm(GameMode::Creative),
            perms::GAMEMODE,
        ),
        (
            "gms",
            "Survival mode.",
            Cmd::Gm(GameMode::Survival),
            perms::GAMEMODE,
        ),
        (
            "gma",
            "Adventure mode.",
            Cmd::Gm(GameMode::Adventure),
            perms::GAMEMODE,
        ),
        (
            "gmsp",
            "Spectator mode.",
            Cmd::Gm(GameMode::Spectator),
            perms::GAMEMODE,
        ),
    ];
    for (name, about, cmd, node) in self_or_other {
        let command = Command::new(&names(name), about)
            .execute(run(cmd))
            .then(player().execute(run(cmd)));
        context.register_command(command, node);
    }

    // A reason is free text and may hold an address a staff member typed: the commands that take
    // one are sensitive, so the console's "issued server command" line shows the command word
    // only (no IP on the console). The audit log keeps the detail, cleaned.
    let warn = Command::new(&names("warn"), "Warn a player.")
        .sensitive()
        .then(player().then(CommandNode::argument("reason", &greedy()).execute(run(Cmd::Warn))));
    context.register_command(warn, perms::WARN);

    for (name, about, cmd, node) in [
        ("mute", "Mute a player for a time.", Cmd::Mute, perms::MUTE),
        (
            "tempban",
            "Ban a player for a time.",
            Cmd::TempBan,
            perms::TEMPBAN,
        ),
    ] {
        let command = Command::new(&names(name), about).sensitive().then(
            player().then(
                CommandNode::argument("duration", &word())
                    .execute(run(cmd))
                    .then(CommandNode::argument("reason", &greedy()).execute(run(cmd))),
            ),
        );
        context.register_command(command, node);
    }

    let history = Command::new(&names("history"), "Read a player's punishment history.").then(
        player()
            .execute(run(Cmd::History))
            .then(page_arg().execute(run(Cmd::History))),
    );
    context.register_command(history, perms::HISTORY);
}
