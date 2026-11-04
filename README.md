# Admin Tools Lite

Tools for a Play Strawberry server's staff: a staff menu, inventory views, vanish, freeze,
teleport and watch, heal, feed, fly, game mode shortcuts, warnings, mutes, temporary bans and a
punishment history. The game's own `/kick`, `/ban`, `/pardon`, `/gamemode` and `/tp` stay as they
are; their kicks and bans are written into the history too.

Every action has its own permission and a line in the audit log. Staff with `admin-lite:notify` see
each other's actions in grey italics, for example `[Steve: Muted Alex for 1h: spam]`.

## Commands

| Command | What it does | Permission (default) |
| --- | --- | --- |
| `/staff [page]` | The staff menu: online players, 45 per page | `admin-lite:menu` (op 2) |
| `/invsee <player>`, `/endersee <player>` | A read-only copy of the inventory or ender chest | `admin-lite:invsee`, `admin-lite:endersee` (op 2) |
| `/vanish` | Hide yourself, or show yourself again | `admin-lite:vanish` (op 2) |
| `/freeze <player>`, `/unfreeze <player>` | Freeze or unfreeze, also offline players | `admin-lite:freeze` (op 2) |
| `/tpto <player>`, `/tphere <player>` | Teleport to a player; bring a player | `admin-lite:tp`, `admin-lite:bring` (op 2) |
| `/watch <player>`, `/watch` | Watch as a spectator; alone, go back | `admin-lite:watch` (op 2) |
| `/heal [player]`, `/feed [player]`, `/fly [player]` | Health, food, flight | `admin-lite:heal`, `feed`, `fly`; others with `.others` (op 2) |
| `/gmc`, `/gms`, `/gma`, `/gmsp` `[player]` | Creative, survival, adventure, spectator | `admin-lite:gamemode`, `.others` (op 2) |
| `/warn <player> <reason>` | A warning, shown at the next join if the player is offline | `admin-lite:warn` (op 2) |
| `/mute <player> <duration> [reason]`, `/unmute <player>` | A temporary mute | `admin-lite:mute` (op 2) |
| `/tempban <player> <duration> [reason]` | A ban with an end | `admin-lite:tempban` (op 3) |
| `/unban <player>` | Lifts a ban | `admin-lite:unban` (op 3) |
| `/history <player> [page]` | The punishment history, newest first | `admin-lite:history` (op 2) |

Durations: `30s`, `10m`, `2h`, `7d`, `2w` and combinations such as `1d12h`; at most ten years.

Other permissions: `admin-lite:kick` (op 3, the kick button in the menu), `admin-lite:vanish.see`
(op 2, sees vanished staff), `admin-lite:notify` (op 2, staff notices), `admin-lite:exempt` (op 4,
cannot be punished or frozen by other staff), `admin-lite:announcements` (everyone, reads public
punishment announcements).

Players are found by name: online players first, then every player seen since the plugin was
installed, so offline players can be warned, muted, banned and looked up.

## Public announcements

When a player is warned, muted, kicked or banned, every other player reads one line, for example
"Alex was muted for 1h. Reason: spam". It never shows the staff member's name or an IP address.
To turn announcements off, take `admin-lite:announcements` from everyone.

## Vanish and freeze

- A vanished staff member has no body, no tab-list entry, no join or leave line, and is not in
  `/list` or command suggestions, for everyone without `admin-lite:vanish.see`. Vanish carries over
  a rejoin and a restart.
- A frozen player is held in place, takes no damage and sees a title. A frozen player who leaves is
  still frozen when he comes back.

## Privacy

The plugin never reads a player's IP address. Anything in a reason that looks like an address is
stored and shown as `[IP]`. Command lines that may hold a password are never kept or logged.

## Files

In `plugins/data/admin-lite/`:

| File | What |
| --- | --- |
| `history.tsv` | the punishment history |
| `seen.tsv` | players seen: UUID, last name, last time |
| `state.tsv` | vanished, frozen and watching staff |
| `audit.log` | one line per staff action (`audit.log.1` is the previous one) |

There is no settings file.

## Languages

English, French, Spanish and Turkish, chosen by each reader's game language (`lang/`). Where the
game has its own message (game mode, teleport, kick, pardon), the game's own text is shown.

## Install on a Play Strawberry server

1. Add Admin Tools Lite to your server in the Play Strawberry panel.
2. Restart the server. The plugin creates `plugins/data/admin-lite/` on the first start.
3. Give the permissions to your staff. Give `admin-lite:vanish.see` to everyone who has the staff
   menu.

Play Strawberry servers load only plugins signed by Play Strawberry, so a file you build yourself
from this source loads only on your own Play Strawberry developer server.

## Build from source

You need Rust with the `wasm32-wasip2` target and the Strawberry plugin SDK
(`strawberry-plugin-api`), which Play Strawberry gives to its plugin developers.
`Cargo.toml` takes it from the SDK registry named `strawberry`; add that registry to
your Cargo configuration (for example `~/.cargo/config.toml`):

To get the SDK, accept its licence at https://panel.playstrawberry.net/strawberry/sdk; the page gives you a
token for `cargo login --registry strawberry`.

```toml
[registries.strawberry]
index = "sparse+https://sdk.playstrawberry.net/index/"
```

```text
cargo test
cargo build --release --target wasm32-wasip2
```

The plugin file is `target/wasm32-wasip2/release/admin_lite.wasm`.

## License

Source available under the Strawberry Source-Available License 1.0 ([LICENSE](LICENSE)). In short: you may read, build and
change this code for your own use, and run what you build only on Play Strawberry, without making money from it unless we
approve it in writing. The build Play Strawberry offers may be used on any of your Play Strawberry servers, also ones that
earn money. Selling the code or anything built from it is not allowed. Forks are welcome for sending us changes (pull
requests and issues).
