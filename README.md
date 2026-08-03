# A3SQL

[![Version](https://img.shields.io/github/release/lErrorl404l/a3sql.svg?style=flat-square&label=Version)](https://github.com/lErrorl404l/a3sql/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/lErrorl404l/a3sql/ci.yml?style=flat-square&label=CI)](https://github.com/lErrorl404l/a3sql/actions)
[![Downloads](https://img.shields.io/github/downloads/lErrorl404l/a3sql/total.svg?style=flat-square&label=Downloads)](https://github.com/lErrorl404l/a3sql/releases)
[![License](https://img.shields.io/badge/License-APL--SA-red.svg?style=flat-square)](LICENSE)

**A3SQL** is a live SQL database engine for Arma 3. It runs inside your server and lets your mission store, query, and update data with plain SQL: player stats that survive restarts, faction loadouts, event logs, admin commands, and mid-mission value patching. No external database, no PBO repacking.

Requires the latest version of [CBA A3](https://github.com/CBATeam/CBA_A3/releases).

---

## What you get

- **Player stats and progression.** Rank, score, kills, and playtime across sessions. Saved on disconnect, restored on JIP reconnect.
- **Loadout management.** Faction and role loadout templates in SQL, applied on spawn with a single call.
- **Dynamic live patching.** Change weapon stats, vehicle properties, and other runtime values mid-mission from SQL rules. No mission restart.
- **Analytics and logging.** Performance snapshots, kill events, and player connections logged to SQL, exportable as CSV for after-action review.
- **Remote administration.** Manage the server from outside the game over the TCP listener: kick/ban players, change missions, run admin commands. Forward notifications to Discord via webhooks.

## Installation

Install from the Steam Workshop, or download the [latest release](https://github.com/lErrorl404l/a3sql/releases/latest) and unpack `@a3sql` into your Arma 3 directory.

Launch the game or server with:

```
-mod=@cba_a3;@a3sql
```

Configure via **CBA Settings** → **A3SQL** categories.

## Quick start

```sqf
// Server-side, once, at mission start:
["CREATE TABLE IF NOT EXISTS stats (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;

// Record a result:
["INSERT INTO stats VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;

// Query the top scores:
_result = ["SELECT name, score FROM stats ORDER BY score DESC LIMIT 10"] call a3sql_fnc_execute;
```

Every command returns `[returnCode, status, data]`, and user input should go through parameterized queries (`$1`, `$2`). The full SQF API, SQL dialect, and server-admin guide live in [docs/README.md](docs/README.md).

## Modules

A3SQL is modular. Remove any PBO you don't need:

| Addon | Purpose |
|-------|---------|
| `a3sql_main` | Core mod definition (required) |
| `a3sql_database` | SQL engine and query API |
| `a3sql_patch_core` | Dynamic patching engine |
| `a3sql_patch_editor` | In-game rule editor |
| `a3sql_patch_operators` | Value transformation operators |
| `a3sql_admin` | Player tracking and admin commands |
| `a3sql_analytics` | Performance monitoring and event logging |
| `a3sql_loadouts` | Faction/role loadout templates |
| `a3sql_persistence` | Player state save/restore |
| `a3sql_progression` | Rank/score tracking |

## Documentation

- [docs/README.md](docs/README.md): quick start, SQF API, SQL dialect, admin guide
- [Getting Started](docs/wiki/Getting-Started.md)
- [SQL Dialect](docs/wiki/SQL-Dialect.md)
- [Patch Framework](docs/wiki/Patch-Framework.md)
- [Module Guide](docs/wiki/Module-Guide.md)
- [CBA Settings](docs/wiki/CBA-Settings.md)
- [Security](docs/wiki/Security.md)
- [TCP Connector](docs/wiki/TCP-Connector.md)

## For modders

Add a3sql as a dependency:

```cpp
requiredAddons[] = {"a3sql_main", "a3sql_database"};
```

Execute SQL from SQF:

```sqf
_result = ["SELECT * FROM loadout_templates WHERE faction = 'NATO'"] call a3sql_fnc_selectMap;
```

Python CLI tools (stdlib only):

```bash
python tools/a3sql-patch.py list
python tools/a3sql-patch.py rcon kick 76561198000000001 --reason "Team killing"
python tools/a3sql-webhook.py --webhook-url "https://discord.com/api/webhooks/..."
```

## Building

```bash
cargo build --release --manifest-path extension/Cargo.toml
hemtt build
cargo test --manifest-path extension/Cargo.toml
```

## Contributing

Pull requests welcome. For bugs and feature requests, open an [issue](https://github.com/lErrorl404l/a3sql/issues).

## License

Licensed under the [Arma Public License Share Alike (APL-SA)](LICENSE), Copyright 2026 ABE Team.
