---
layout: wiki
wiki: A3SQL
title: Use Cases
group: wiki
order: 4
parent: Home
---

# Use Cases

Practical scenarios for A3SQL. Each section shows what you build, how to
build it, and common pitfalls to avoid.

## Server admins

### Kill/death tracking with persistence

Track kills, deaths, playtime, and score across every mission on your server.
JIP players get their stats restored automatically.

Simple kill counter:

```sqf
// Inside a Killed event handler on the server:
// params ["_unit", "_killer"];
private _uid  = getPlayerUID _killer;
private _name = name _killer;

["INSERT OR REPLACE INTO stats (uid, name, kills)
 VALUES ($1, $2,
   COALESCE((SELECT kills FROM stats WHERE uid = $1), 0) + 1
 )", "a3sql", [_uid, _name]] call a3sql_fnc_execute;
```

Top 10 leaderboard:

```sqf
_result = ["SELECT name, kills FROM stats ORDER BY kills DESC LIMIT 10"] call a3sql_fnc_selectArray;
```

Export to CSV for a web panel:

```sqf
private _csv = ["stats"] call a3sql_fnc_exportCSV;
```

**How:** [Getting Started](Getting-Started), [SQL Dialect](SQL-Dialect)

### Remote administration from Discord

The TCP listener lets external tools query the database while the mission
runs. A Discord bot can kick, ban, or show stats without anyone being in-game.

```python
import socket

def query_a3sql(sql):
    s = socket.socket()
    s.connect(("127.0.0.1", 33306))
    s.sendall(b"LOGIN admin yourpassword\n")
    s.recv(65536)  # [0,"OK","Authenticated"]
    s.sendall((sql + "\n").encode())
    result = s.recv(65536).decode()
    s.close()
    return result
```

Warning: The in-game TCP listener binds to `127.0.0.1`. It only accepts
connections from the same machine. Use the
[Standalone Server](Standalone-Server) for remote access.

**How:** [Standalone Server](Standalone-Server), [TCP Connector](TCP-Connector)

### Ban system with expiry

Store bans in SQL with optional expiry dates. Check on player connect.

```sql
CREATE TABLE bans (
  uid STRING PRIMARY KEY,
  reason STRING,
  banned_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  expires_at TIMESTAMP
);
```

Check on player connect (in `initPlayerServer.sqf`):

```sqf
// initPlayerServer.sqf receives the player as _this select 0
private _player = _this select 0;
private _uid = getPlayerUID _player;
private _result = ["SELECT 1 FROM bans WHERE uid = $1
  AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
  "a3sql", [_uid]] call a3sql_fnc_execute;

if (_result select 0 == 0 && {count (_result select 2) > 0}) then {
    // Player is banned, kick them
    kick _player;
    [format ["Banned player %1 attempted to join", name _player]] remoteExec ["systemChat", 0];
};
```

**How:** [SQL Dialect](SQL-Dialect), [CBA Settings](CBA-Settings)

### Performance impact

A3SQL runs in-memory inside the Arma process. For a typical 64-slot server:

| Data size | RAM cost | Save time | Impact on gameplay |
|---|---|---|---|
| 100 MB | ~100 MB | < 1 s | None |
| 1 GB | ~1 GB | 2-5 s | Brief hitch on save |
| 5 GB | ~5 GB | 5-15 s | Noticeable hitch |

The database saves to `profile_name/a3sql/` as `.bin` files. Each save is
atomic: a new file is written, then renamed over the old one. A crash during
save loses at most the current in-progress write.

**How:** [Production Readiness](Production-Readiness)

## Mission makers

### Player loadout persistence

Save and restore loadouts across sessions. Players keep their gear when the
mission changes.

```sqf
// Save loadout on disconnect
addMissionEventHandler ["HandleDisconnect", {
    params ["_unit"];
    private _uid  = getPlayerUID _unit;
    private _loadout = getUnitLoadout _unit;

    ["INSERT INTO loadouts (uid, loadout) VALUES ($1, $2)
      ON CONFLICT (uid) DO UPDATE SET loadout = $2",
     "a3sql", [_uid, _loadout]] call a3sql_fnc_execute;
    false  // allow disconnect
}];

// Restore loadout on join
addMissionEventHandler ["EntityCreated", {
    params ["_entity"];
    if (_entity isKindOf "Man" && isPlayer _entity) then {
        private _uid = getPlayerUID _entity;
        private _result = ["SELECT loadout FROM loadouts WHERE uid = $1",
          "a3sql", [_uid]] call a3sql_fnc_execute;

        if (_result select 0 == 0 && {count (_result select 2) > 0}) then {
            private _loadout = (_result select 2) select 0 select 0;
            _entity setUnitLoadout _loadout;
        };
    };
}];
```

**How:** [Getting Started](Getting-Started), [SQL Dialect](SQL-Dialect)

### Dynamic balance/economy system

Track player currency, vehicle ownership, or base building resources.

```sql
CREATE TABLE economy (
  uid STRING PRIMARY KEY,
  balance INTEGER DEFAULT 0,
  last_transaction TIMESTAMP
);
```

```sqf
// Deduct currency
["UPDATE economy SET balance = balance - $1,
  last_transaction = CURRENT_TIMESTAMP
  WHERE uid = $2 AND balance >= $1",
 "a3sql", [500, getPlayerUID player]] call a3sql_fnc_execute;

// Check balance
_result = ["SELECT balance FROM economy WHERE uid = $1",
  "a3sql", [getPlayerUID player]] call a3sql_fnc_execute;
```

**How:** [SQL Dialect](SQL-Dialect), [CBA Settings](CBA-Settings)

### Event logging for admin review

Log admin actions, player reports, or suspicious activity for review.

```sql
CREATE TABLE admin_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  action STRING,
  player_uid STRING,
  target_uid STRING,
  timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  details STRING
);
```

```sqf
["INSERT INTO admin_log (action, player_uid, target_uid, details)
  VALUES ($1, $2, $3, $4)",
 "a3sql", ["kick", "12345678", "87654321", "Team killing"]] call a3sql_fnc_execute;
```

**How:** [Module Guide](Module-Guide)

## Mod developers

### FFI extension for custom SQL

Call your own SQL functions from SQF through the FFI extension. Use this for
high-performance operations that bypass the interpreter.

```cpp
// a3sql_ffi_example.cpp
#include <a3sql/ffi.h>

extern "C" int32_t my_custom_query(
    const char* sql, int32_t sql_len,
    char* result, int32_t* result_len
) {
    // Your custom SQL logic here
    // Write result to the output buffer
    return 0;  // 0 = success
}
```

Warning: The FFI extension must compile against the A3SQL SDK headers.
See [Development Setup](Development-Setup) for build instructions.

**How:** [Development Setup](Development-Setup), [Plugins](Plugins)

### Plugin system for custom functions

Register custom SQF functions through the plugin system. Each plugin is a
separate PBO that registers its functions with A3SQL.

```cpp
// plugin_init.cpp
#include <a3sql/plugin.h>

void a3sql_plugin_init() {
    a3sql_register_function("my_custom_fn", my_custom_fn_impl);
}
```

**How:** [Plugins](Plugins), [Development Setup](Development-Setup)

### Compatibility testing

Test your mod's SQL against the A3SQL engine before release. Use the
compatibility corpus to catch dialect issues early.

```bash
cd tools
uv run sql_corpus/compatibility_test.py --mod my_mod.sql
```

**How:** [SQL Compatibility Report](SQL-Compatibility-Report), [SQL Dialect](SQL-Dialect)

## FAQ

### Does A3SQL work with other database mods?

No. A3SQL uses its own in-memory SQLite engine. It does not connect to
external databases. The TCP listener and FFI extension provide integration
points for external tools.

### Can I migrate data between A3SQL versions?

Yes. The database files are in `profile_name/a3sql/`. Copy the `.bin` files
to the new server. Backward compatibility is maintained within major versions.

### What happens if the server crashes?

A3SQL uses atomic writes. A crash during save loses at most the current
in-progress write. Previous saves are intact. Enable periodic auto-save
through CBA Settings for additional safety.

### How do I debug SQL issues?

Enable debug logging in CBA Settings. Check the Arma log file for SQL
errors. Use the TCP connector to test queries interactively from a Python
or Node.js script.

**How:** [CBA Settings](CBA-Settings), [TCP Connector](TCP-Connector)
