# a3sql Module Integration Guide

Documentation for modders and scripters who want to store, share, and query data using a3sql.

- [1. Adding a3sql as a dependency](#1-adding-a3sql-as-a-dependency)
- [2. Function reference](#2-function-reference)
- [3. Schema registry convention](#3-schema-registry-convention)
- [4. Dependency injection pattern](#4-dependency-injection-pattern)
- [5. Security model](#5-security-model)
- [6. Best practices](#6-best-practices)
- [7. Example: simple tracking mod](#7-example-simple-tracking-mod)

---

## 1. Adding a3sql as a dependency

Add `a3sql_main` and `a3sql_sql` to your mod's `requiredAddons[]`:

```cpp
class CfgPatches {
    class MyMod {
        requiredAddons[] = {"a3sql_main", "a3sql_sql", "cba_xeh"};
    };
};
```

| Dependency | What it provides |
|---|---|
| `a3sql_main` | Core extension load, CBA settings, version check |
| `a3sql_sql` | SQF wrapper functions (`a3sql_fnc_*`) |
| `cba_xeh` | Extended Event Handlers (init, preInit, postInit) |

If you use the patch framework, add `a3sql_patch` too:

```cpp
requiredAddons[] = {"a3sql_sql", "a3sql_patch", "cba_xeh"};
```

---

## 2. Function reference

All functions take an optional extension name as the last parameter (defaults to `"a3sql"`). They return a parsed result array `[code, "status", data]` where `code == 0` means success.

### Core SQL

| Function | Description |
|---|---|
| `a3sql_fnc_execute` | Run SQL. Supports parameterized queries via `$1`, `$2` |
| `a3sql_fnc_executePrepared` | Run a prepared statement by name with params |
| `a3sql_fnc_executeTimed` | Same as execute, logs to RPT if query takes >10ms |
| `a3sql_fnc_prepare` | Prepare a named statement for repeated use |
| `a3sql_fnc_selectAll` | SELECT with auto-pagination for large result sets |
| `a3sql_fnc_selectArray` | SELECT returning rows as arrays (skips column headers) |
| `a3sql_fnc_selectMap` | SELECT returning rows as hash maps (column name -> value) |

### Persistence

| Function | Description |
|---|---|
| `a3sql_fnc_save` | Save full database to binary file |
| `a3sql_fnc_load` | Restore full database from binary file |
| `a3sql_fnc_exportJSON` | Export a table as JSON |
| `a3sql_fnc_exportCSV` | Export a table as CSV |
| `a3sql_fnc_exportSQL` | Export the full database as SQL dump |
| `a3sql_fnc_loadJSON` | Load JSON data into a table |
| `a3sql_fnc_dumpSQL` | Alias for `exportSQL` |

### Initialize and settings

| Function | Description |
|---|---|
| `a3sql_fnc_init` | Initialize extension, print version to RPT |
| `a3sql_fnc_settings` | Register CBA settings (called automatically via PreInit) |
| `a3sql_fnc_postInit` | Set up auto-save/load hooks (called automatically) |

### Basic usage

```sqf
// Execute a statement
_result = ["CREATE TABLE IF NOT EXISTS players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;

// Query with results as hash maps
_result = ["SELECT name, score FROM players ORDER BY score DESC"] call a3sql_fnc_selectMap;
// Returns: [{name: "Scarface", score: 1500}, {name: "Stitch", score: 1200}]

// Parameterized query (safe)
_result = ["SELECT * FROM players WHERE uid = $1", "a3sql", ["76561198000000001"]] call a3sql_fnc_execute;

// Prepared statement
["get_player", "SELECT name, score FROM players WHERE uid = $1"] call a3sql_fnc_prepare;
_result = ["get_player", ["76561198000000001"]] call a3sql_fnc_executePrepared;

// Persistence
["mydata.bin"] call a3sql_fnc_save;
["mydata.bin"] call a3sql_fnc_load;
```

---

## 3. Schema registry convention

a3sql uses an **in-process, in-memory database**. Every mod that writes to the
extension shares the same database namespace. There are no separate databases
per mod, so tables must be **namespaced** to avoid collisions.

### Table naming convention

Prefix your tables with a short mod identifier:

```sqf
["CREATE TABLE IF NOT EXISTS mytracker_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT,
    pos_x FLOAT,
    pos_y FLOAT,
    mission_name TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"] call a3sql_fnc_execute;
```

### Why this matters

Two mods both creating `CREATE TABLE events` will collide. The second `CREATE
TABLE` fails. If they use `CREATE TABLE IF NOT EXISTS`, the second silently
skips and the first mod's schema wins, which might not match what the second
mod expects.

### Recommendation

- Choose a short prefix: `mythical_`, `my_`, `tm_`, `abc_`
- Keep it lowercase, no spaces
- Document your tables so other mods can read them

---

## 4. Dependency injection pattern

a3sql's shared database enables a powerful pattern: **Mod A writes data, Mod B
reads it without either mod depending on the other.**

**Example:**
- Mod A (Telemetry) writes to `mod_telemetry_events`
- Mod B (Live Map) reads from `mod_telemetry_events` and displays on a web dashboard
- Both mods only depend on `a3sql_sql`
- Neither mod knows the other exists

This decouples mod dependencies at the config level while still allowing data
sharing at runtime.

```sqf
// Mod A writes
["INSERT INTO mod_telemetry_events (event_type, pos_x, pos_y) VALUES ('shot', 100, 200)"] call a3sql_fnc_execute;

// Mod B reads
_result = ["SELECT * FROM mod_telemetry_events WHERE event_type = 'shot'"] call a3sql_fnc_selectMap;
```

### Table as API

When your mod writes data that others consume, treat your table schema as a
public API:

- Document the table columns and types
- Version your table names if you expect schema changes: `mod_events_v2`
- Add a `_schema` table or comment convention for discovery

### Adapter pattern

If you want to provide a friendlier API on top of raw SQL, wrap queries in
your own functions:

```sqf
// mymod_fnc_getTopPlayers.sqf
params ["_limit"];
_result = [format ["SELECT name, score FROM mymod_stats ORDER BY score DESC LIMIT %1", _limit]] call a3sql_fnc_selectMap;
_result
```

---

## 5. Security model

### Parameterized queries

Always use `$1`, `$2` placeholders for user-supplied values:

```sqf
// Safe
_result = ["SELECT * FROM players WHERE name = $1", "a3sql", [_playerInput]] call a3sql_fnc_execute;

// Unsafe - do not use string interpolation for user input
_result = [format ["SELECT * FROM players WHERE name = '%1'", _playerInput]] call a3sql_fnc_execute;
```

### Listener security

The TCP listener (enabled by default, port 33306) accepts SQL queries from
external tools:

- Binds to `127.0.0.1` by default (localhost only)
- Set a username and password in CBA settings to require `LOGIN` before queries
- Change the bind address to `0.0.0.0` only if you need remote access across a network
- The extension runs in-process with the game and has access to the game's
  in-memory database

### Threat model

- **Localhost listener**: any process on the same machine can connect
- **Remote listener**: any process that can reach the port can connect
- **Amend/credentialed access**: the listener authenticates before accepting SQL
- **SQL injection**: prevented by using `$1` parameterized syntax
- **Mod collisions**: no isolation between mod databases (all in one process
  space)
- **Mission SQF**: any mod loaded on the server can write to any table (no
  per-mod access control in a3sql's in-memory database)

### Recommendations

- Keep the listener bound to `127.0.0.1` unless you need remote queries
- Always set credentials for the TCP listener in production
- Use parameterized queries for any user-supplied input
- Do not expose the TCP port to the public internet
- Sync traffic should use SSH tunnels if crossing untrusted networks

---

## 6. Best practices

- Always use `CREATE TABLE IF NOT EXISTS` to handle re-insertion
- Use `INTEGER PRIMARY KEY` for auto-incrementing IDs
- Use prepared statements for user input: `SELECT * FROM table WHERE uid = $1`
- Use `IF EXISTS` before `DROP TABLE`
- Clean up old data: `DELETE FROM events_shots WHERE timestamp < datetime('now', '-30 days')`
- Batch INSERTs when importing large datasets: use multi-VALUES format
- Use `LIMIT` + `OFFSET` for pagination when SELECTing many rows
- Run analytics/report queries during mission end, not mid-game
- Use `call a3sql_fnc_selectMap` instead of `call a3sql_fnc_execute` for easier data access

---

## 7. Example: simple tracking mod

### mod.cpp

```cpp
class CfgPatches {
    class MyTracker {
        name = "My Tracker";
        author = "Me";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_sql", "cba_xeh"};
        units[] = {};
        weapons[] = {};
    };
};
```

### fn_postInit.sqf

```sqf
#include "script_component.hpp"

if (!isServer) exitWith {};

// Create the events table
private _sql = "CREATE TABLE IF NOT EXISTS mytracker_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT NOT NULL,
    pos_x FLOAT DEFAULT 0.0,
    pos_y FLOAT DEFAULT 0.0,
    mission_name TEXT DEFAULT '',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";
_sql call a3sql_fnc_execute;

// Record player connections
addMissionEventHandler ["PlayerConnected", {
    params ["_id", "_uid", "_name", "_jip", "_owner"];
    ["INSERT INTO mytracker_events (event_type, pos_x, pos_y, mission_name) VALUES ('connect', 0, 0, $1)", "a3sql", [missionName]] call a3sql_fnc_execute;
}];

// Clean up old events every 10 minutes
[{
    ["DELETE FROM mytracker_events WHERE created_at < datetime('now', '-7 days')"] call a3sql_fnc_execute;
}, [], 600] call CBA_fnc_addPerFrameHandler;
```

### fn_getTopPlayers.sqf

```sqf
#include "script_component.hpp"

// Query top 5 players by score
_result = ["SELECT uid, name, score FROM player_progression ORDER BY score DESC LIMIT 5"] call a3sql_fnc_selectMap;

{
    diag_log text format ["[MyTracker] Player %1 (%2): %3", _x get "name", _x get "uid", _x get "score"];
} forEach _result;

_result
```
