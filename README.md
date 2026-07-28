# A3SQL — Live Database Engine for Arma 3

[![CI](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions)
[![Wiki](https://img.shields.io/badge/docs-wiki-green)](https://github.com/lErrorl404l/a3sql/wiki)
[![License](https://img.shields.io/badge/license-APL--SA-red)](LICENSE)

**A3SQL** is an embeddable SQL database engine for Arma 3. It runs as a `callExtension` in-process on your server, storing and querying mission data without external tools. Think of it as **SQLite for Arma**.

---

## What you can do with it

| Feature | What it means for your server |
|---------|-------------------------------|
| **Live patching** | Change weapon stats, vehicle values, textures mid-mission — no PBO repacking or restart needed. Edit rules from the in-game dialog or remotely over TCP. |
| **Player loadouts** | Store faction/role loadout templates in SQL. Apply them on spawn — no more massive `init.sqf` blocks. |
| **Cross-session progression** | Track player rank, score, kills, and playtime across missions using Arma's built-in rank system. |
| **Mission state persistence** | Auto-save player loadout and position on disconnect, restore on JIP reconnect. |
| **Performance monitoring** | Log server FPS, entity counts, and player counts to SQL every 60s. Identify which missions cause lag. |
| **Kill tracking** | Log every kill event with weapon, distance, headshot status. Export as CSV for after-action review. |
| **Admin commands via TCP** | Kick, ban, change missions, lock the server — all from an external tool or Discord webhook. |
| **Replay data** | Record position snapshots every 30s. Export for external mission analysis. |
| **Mod-to-mod data sharing** | Mod A writes to SQL, Mod B reads it — no cross-dependency needed. |

---

## Getting Started

### 1. Install

Download from the [Releases page](https://github.com/lErrorl404l/a3sql/releases) or build from source. Place `@a3sql` in your Arma 3 directory.

**Required**: CBA_A3 (any version).

### 2. Configure via CBA Settings

Open CBA Settings in-game or in your server config:

| Setting | What it controls |
|---------|-----------------|
| `a3sql_database_enabled` | Enable/disable the database engine |
| `a3sql_database_host` | TCP listener bind address (default: `0.0.0.0`) |
| `a3sql_database_port` | TCP listener port (default: `33306`) |
| `a3sql_database_tcp_enabled` | Enable remote TCP access |
| `a3sql_database_password` | TCP listener password (leave blank for no auth) |
| `a3sql_patch_core_enabled` | Enable dynamic patching |

### 3. Deploy to your server

```
-mod=@cba_a3;@a3sql
```

### 4. Manage from in-game

Press the editor keybind (default: Ctrl+Shift+E) to open the Patch Editor dialog — add, edit, or delete patch rules visually.

### 5. Manage remotely

```bash
# List all patch rules
python tools/a3sql-patch.py list

# Add a rule
python tools/a3sql-patch.py add weapon reloadTime 2.5 --name "M4 buff"

# Kick a player
python tools/a3sql-patch.py rcon kick 76561198000000001

# See who's online
python tools/a3sql-patch.py rcon players

# Live-updating admin dashboard
python tools/a3sql-webhook.py --webhook-url "https://discord.com/api/webhooks/..."
```

---

## Modules

A3SQL is modular — remove any PBO you don't need:

| Addon | Function prefix | What it does | Remove if... |
|-------|----------------|-------------|--------------|
| `a3sql_main` | — | Core mod definition, version, macros | — |
| `a3sql_database` | `a3sql_fnc_*` | SQL engine, query execution, persistence | You don't need SQL |
| `a3sql_patch_core` | `a3sql_patch_core_fnc_*` | Dynamic patching engine, PerFrame handler | You don't use live patching |
| `a3sql_patch_editor` | `a3sql_patch_editor_fnc_*` | In-game rule editor dialog | You manage rules via SQL |
| `a3sql_patch_operators` | `a3sql_patch_operators_fnc_*` | Value transformer functions | You don't use custom operators |
| `a3sql_admin` | `a3sql_admin_fnc_*` | Player tracking, admin command execution | You use built-in admin tools |
| `a3sql_analytics` | `a3sql_analytics_fnc_*` | Perf monitoring, kill tracking, replay snapshots | You don't need analytics |
| `a3sql_loadouts` | `a3sql_loadouts_fnc_*` | Loadout templates with faction/role CRUD | You use a different loadout system |
| `a3sql_persistence` | `a3sql_persistence_fnc_*` | Player state save/restore on DC/JIP | Your mission handles respawn |
| `a3sql_progression` | `a3sql_progression_fnc_*` | Rank/score tracking across sessions | You don't need persistence |

---

## Server Commands (RCON)

Integrates with Arma 3's built-in `serverCommand` system. Commands are queued via SQL and executed by the PerFrame handler:

```bash
# Kick a player
python tools/a3sql-patch.py rcon kick 76561198000000001 --reason "Team killing"

# List available missions
python tools/a3sql-patch.py rcon missions

# Lock the server
python tools/a3sql-patch.py rcon lock

# Say something in chat
python tools/a3sql-patch.py rcon say "Server restart in 5 minutes"
```

See the [Admin Commands](https://github.com/lErrorl404l/a3sql/wiki/Admin-Commands) wiki page for all available commands.

---

## For Modders

### Add a3sql as a dependency

```cpp
requiredAddons[] = {"a3sql_main", "a3sql_database"};
```

### Execute SQL from SQF

```sqf
_result = ["SELECT * FROM players WHERE score > 1000"] call a3sql_fnc_selectMap;
```

### Example: Loading a loadout template on spawn

```sqf
if (!isNil "a3sql_loadouts_fnc_applyLoadout") then {
    [player, "NATO", "rifleman"] call a3sql_loadouts_fnc_applyLoadout;
};
```

See the [Module Integration Guide](https://github.com/lErrorl404l/a3sql/wiki/Module-Guide) for full documentation.

---

## Tools

A3SQL ships with Python CLI tools (stdlib only, no dependencies):

| Tool | Purpose |
|------|---------|
| `tools/a3sql-patch.py` | Manage patch rules, players, and admin commands over TCP |
| `tools/a3sql-webhook.py` | Forward admin commands to Discord/Slack webhooks |
| `tools/a3sql-sync.py` | Sync tables between a3sql servers (hub-and-spoke) |
| `tools/sqf_validator.py` | Validate SQF file syntax |
| `tools/sqfvmChecker.py` | Full SQF parse check with SQF-VM |

---

## Building from Source

```bash
cargo build --release -p a3sql          # Extension binary
hemtt build                              # Addon PBOs (10 addons)
cargo test -p a3sql                       # 420+ tests
```

See the [Building wiki page](https://github.com/lErrorl404l/a3sql/wiki/Building) for cross-compilation, code signing, and CI details.

---

## License

[Arma Public License Share Alike (APL-SA)](LICENSE) — as required by Bohemia Interactive for Arma 3 mods.
