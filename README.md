# A3SQL

[![Version](https://img.shields.io/github/release/lErrorl404l/a3sql.svg?style=flat-square&label=Version)](https://github.com/lErrorl404l/a3sql/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/lErrorl404l/a3sql/ci.yml?style=flat-square&label=CI)](https://github.com/lErrorl404l/a3sql/actions)
[![Downloads](https://img.shields.io/github/downloads/lErrorl404l/a3sql/total.svg?style=flat-square&label=Downloads)](https://github.com/lErrorl404l/a3sql/releases)
[![License](https://img.shields.io/badge/License-APL--SA-red.svg?style=flat-square)](LICENSE)

**A3SQL** is a live database engine for Arma 3. It stores and processes mission data at runtime — managing loadouts, tracking player stats, patching in-game values, logging events — all through a persistent SQL database embedded in your server.

Requires the latest version of [CBA A3](https://github.com/CBATeam/CBA_A3/releases).

---

## Features

### Dynamic live patching
Modify weapon stats, vehicle properties, textures, and any runtime-settable value mid-mission. Rules are stored in SQL and applied automatically. No PBO repacking or mission restart.

### Player loadout management
Store faction/role-based loadout templates in SQL. Apply them on spawn with a single function call.

### Persistence & progression
Track player rank, score, kills, and playtime across sessions using Arma's built-in rank system. Save player loadouts and positions on disconnect, restore on JIP reconnect.

### Analytics & logging
Log server performance (FPS, entity counts), kill events (weapon, distance, headshot), and player connections to SQL. Export as CSV for after-action review.

### Remote administration
Manage your server from outside the game — kick/ban players, change missions, execute admin commands over TCP. Forward notifications to Discord via webhooks.

## Installation

Download the [latest release](https://github.com/lErrorl404l/a3sql/releases/latest) and unpack `@a3sql` into your Arma 3 directory.

Launch with:
```
-mod=@cba_a3;@a3sql
```

Configure via **CBA Settings** → **A3SQL** categories.

## Documentation

- [Wiki Home](https://github.com/lErrorl404l/a3sql/wiki)
- [Patch Framework](https://github.com/lErrorl404l/a3sql/wiki/Patch-Framework)
- [Module Integration Guide](https://github.com/lErrorl404l/a3sql/wiki/Module-Guide)
- [Admin Commands](https://github.com/lErrorl404l/a3sql/wiki/Admin-Commands)

## Modules

A3SQL is modular. Remove any PBO you don't need:

| Addon | Purpose |
|-------|---------|
| `a3sql_main` | Core mod definition (required) |
| `a3sql_database` | SQL engine and query execution |
| `a3sql_patch_core` | Dynamic patching engine |
| `a3sql_patch_editor` | In-game rule editor dialog |
| `a3sql_patch_operators` | Value transformation operators |
| `a3sql_admin` | Player tracking and admin commands |
| `a3sql_analytics` | Performance monitoring and event logging |
| `a3sql_loadouts` | Faction/role loadout templates |
| `a3sql_persistence` | Player state save/restore |
| `a3sql_progression` | Rank/score tracking |

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
python tools/a3sql-patch.py rcon kick 76561198000000001
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

[Arma Public License Share Alike (APL-SA)](LICENSE)
