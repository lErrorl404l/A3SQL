# A3DB — Worked Example

This guide walks through building a simple Arma 3 mod that uses A3DB to
persist player stats between missions.

---

## 1. Project Setup

Your mod needs A3DB and CBA_A3 as dependencies.

```toml
# .hemtt/project.toml (your addon)
name = "my-stats"
prefix = "mystats"
mainprefix = "z"

[files]
include = [
    "mod.cpp",
    "meta.cpp",
]
```

## 2. CfgPatches

```cpp
// addons/main/config.cpp
class CfgPatches {
    class mystats_main {
        name = "My Stats";
        author = "Me";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3db_main", "a3db_sql", "cba_main"};
        units[] = {};
        weapons[] = {};
    };
};

class CfgFunctions {
    class mystats {
        class main {
            file = QPATHTO_FOLDER(main);
            class postInit {};
        };
    };
};
```

## 3. Initialize Database

```sqf
// addons/main/fn_postInit.sqf
#include "script_component.hpp"

// Create the stats table once
["mystats", "CREATE TABLE IF NOT EXISTS player_stats (
    uid      STRING PRIMARY KEY,
    kills    INT DEFAULT 0,
    deaths   INT DEFAULT 0,
    captures INT DEFAULT 0
)"] call a3db_fnc_execute;
```

## 4. Record Events

```sqf
// addons/main/fn_onKill.sqf
#include "script_component.hpp"

params ["_killer", "_victim"];

private _uid = getPlayerUID _killer;
// Use a SELECT to check before inserting
_sql = format ["SELECT COUNT(*) FROM player_stats WHERE uid = '%1'", _uid];
private _exists = ([_sql] call a3db_fnc_execute) select 2 select 0;
if (_exists == "0") then {
    _sql = format [
        "INSERT INTO player_stats (uid, kills) VALUES ('%1', 1)",
        _uid
    ];
    [_sql] call a3db_fnc_execute;
};

// Insert if first kill
_sql = format [
    "INSERT OR IGNORE INTO player_stats (uid, kills) VALUES ('%1', 1)",
    _uid
];
[_sql] call a3db_fnc_execute;
```

## 5. Query Stats

```sqf
// addons/main/fn_getStats.sqf
#include "script_component.hpp"

params ["_uid"];

private _sql = format [
    "SELECT kills, deaths, captures FROM player_stats WHERE uid = '%1'",
    _uid
];
_result = [_sql] call a3db_fnc_execute;
// Returns [code, "OK", [["kills","deaths","captures"],[42,7,3]]]
_result
```

## 6. Fuzzy Search

Search player names or items with typo tolerance:

```sqf
_results = ["
    SELECT name, uid FROM players WHERE name %% 'joh'
"] call a3db_fnc_execute;
// Matches "John", "Johnson", "Johansson" via trigram similarity
```

## 7. Save & Load

Persist the database between game sessions:

```sqf
// Server-side persistence
if (isServer) then {
    // Auto-load on mission start
    ["my_stats.bin"] call a3db_fnc_load;

    // Auto-save on mission end
    addMissionEventHandler ["Ended", {
        ["my_stats.bin"] call a3db_fnc_save;
    }];
};
```

## 8. Transactions

Group multiple writes atomically:

```sqf
["BEGIN"] call a3db_fnc_execute;

["INSERT INTO log (event, time) VALUES ('mission_start', '2026-07-24')"] call a3db_fnc_execute;
["UPDATE server_stats SET missions = missions + 1"] call a3db_fnc_execute;

["COMMIT"] call a3db_fnc_execute; // All or nothing
```

## 9. Export Data

```sqf
// Export to CSV for spreadsheets
_result = ["player_stats"] call a3db_fnc_exportCSV;

// Full SQL dump for backup
_backup = [] call a3db_fnc_exportSQL;
```

## 10. Full Example Mission

```
my_mission.Altis/
├── initServer.sqf
└── initPlayerLocal.sqf
```

**initServer.sqf:**
```sqf
// Create tables at mission start
["stats", "CREATE TABLE IF NOT EXISTS session_stats (
    uid STRING PRIMARY KEY,
    score INT
)"] call a3db_fnc_execute;

// Restore previous session data if available
["session_data.bin"] call a3db_fnc_load;

// Save when mission ends
addMissionEventHandler ["Ended", {
    ["session_data.bin"] call a3db_fnc_save;
}];
```

**initPlayerLocal.sqf:**
```sqf
// Insert player on join
private _sql = format [
    "INSERT INTO session_stats (uid, score) VALUES ('%1', 0)",
    getPlayerUID player
];
// Ignore PK conflict if player already exists
_sql call a3db_fnc_execute;

// Query current scores
_result = ["SELECT uid, score FROM session_stats ORDER BY score DESC LIMIT 10"] call a3db_fnc_execute;
systemChat format ["Scores: %1", _result];
```

---

## A3DB API Reference

All functions accept an optional extension name as the last parameter
(defaults to `"a3db"`). All return a parsed JSON array:

```
[code, "OK|ERR_*", data]
```

| Function | Purpose | Example |
|----------|---------|---------|
| `a3db_fnc_execute(sql)` | Run SQL statements | `["SELECT * FROM t"] call a3db_fnc_execute` |
| `a3db_fnc_loadJSON(table, data)` | Import JSON data or file | `["items", loadFile "data.json"] call a3db_fnc_loadJSON` |
| `a3db_fnc_exportJSON(table)` | Export table as JSON | `["items"] call a3db_fnc_exportJSON` |
| `a3db_fnc_exportCSV(table)` | Export table as CSV | `["items"] call a3db_fnc_exportCSV` |
| `a3db_fnc_exportSQL()` | Full DB SQL dump | `[] call a3db_fnc_exportSQL` |
| `a3db_fnc_dumpSQL()` | Same as exportSQL | `[] call a3db_fnc_dumpSQL` |
| `a3db_fnc_save(path)` | Save DB to binary file | `["data.bin"] call a3db_fnc_save` |
| `a3db_fnc_load(path)` | Restore DB from file | `["data.bin"] call a3db_fnc_load` |
