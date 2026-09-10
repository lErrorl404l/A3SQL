# Getting Started: Your First A3SQL Mod

This guide builds a small stats mod end to end. It tracks how many times each player has been killed, keeps the scores across missions, and shows a leaderboard. When you are done you will have a working mod that creates a table, records an event, saves it to disk, and reads it back.

## What You Need

- Arma 3
- The latest release of [CBA](https://github.com/CBATeam/CBA_A3/releases)
- The latest release of A3SQL

Launch Arma with `-mod=@cba_a3;@a3sql` (or add both mods in your launcher). Everything in this guide runs server-side, so a dedicated server or a hosted multiplayer session is the natural target.

## Step 1: Depend on A3SQL and CBA

A mod that uses A3SQL lists both CBA and A3SQL as addon dependencies. If you build with HEMTT, set them up in `.hemtt/project.toml`:

```toml
name = "my-stats"
prefix = "mystats"
mainprefix = "z"

[files]
include = [
    "mod.cpp",
    "meta.cpp",
]
```

Then declare the addons in `addons/main/config.cpp`:

```cpp
class CfgPatches {
    class mystats_main {
        name = "My Stats";
        author = "You";
        requiredAddons[] = {"a3sql_main", "a3sql_database", "cba_main"};
        units[] = {};
        weapons[] = {};
    };
};
```

`a3sql_main` is A3SQL's core mod definition, and `a3sql_database` is the SQL engine itself. CBA must load first so A3SQL's functions and settings exist before your code runs.

## Step 2: Write the Mission init

Database work belongs on the server. In your mission's `init.sqf`, guard everything with `isServer`:

```sqf
if (isServer) then {
    // Step 3: create the table
    // Step 5: load the previous save
};
```

## Step 3: Create a Table

```sqf
if (isServer) then {
    ["CREATE TABLE IF NOT EXISTS player_stats (
        uid   STRING PRIMARY KEY,
        name  STRING,
        kills INT DEFAULT 0
    )"] call a3sql_fnc_execute;
};
```

`IF NOT EXISTS` keeps this safe to run on every mission start. The table has a STRING primary key for the player's Steam UID, plus a name and a kill counter. See the [SQL Dialect](SQL-Dialect) page for the full type list.

## Step 4: Record an Event

When a player dies, record the kill. This is the interesting part: a single UPSERT both inserts the row on a player's first kill and increments the counter on every kill after that. No separate count-then-insert dance needed.

```sqf
// initPlayerLocal.sqf, or any event handler that fires on the server
private _uid  = getPlayerUID _killer;
private _name = name _killer;

["INSERT INTO player_stats (uid, name, kills) VALUES ($1, $2, 1)
  ON CONFLICT (uid) DO UPDATE SET kills = kills + 1, name = $2",
 "a3sql", [_uid, _name]] call a3sql_fnc_execute;
```

The `$1` and `$2` placeholders are filled with your values as separate parameters, never by string interpolation. That makes the query injection-safe: a hostile player name cannot smuggle SQL into your statement. A3SQL also supports named prepared statements for the same purpose (`prepare <name> <sql>` and `execute_prepared <name> [args...]`).

## Step 5: Persist It

The engine is in-memory, so nothing survives a server restart unless you save it. Save the database when the mission ends and load it back at the next mission start:

```sqf
// init.sqf
if (isServer) then {
    // restore from the previous mission
    ["my_stats.bin"] call a3sql_fnc_load;

    // save when this mission ends
    addMissionEventHandler ["Ended", {
        ["my_stats.bin"] call a3sql_fnc_save;
    }];
};
```

`a3sql_fnc_load` and `a3sql_fnc_save` round-trip the whole database as one binary file. You can also let A3SQL handle this for you: the Auto-Save and Auto-Load [CBA settings](CBA-Settings) do the same thing with a path you configure, no code required.

Data volume: this is an in-memory engine. A few thousand rows is trivial, 10k to 100k rows is comfortable, and you should keep your tables under 100k rows. Mission-end saves hold the database mutex only briefly, so they will not stall gameplay.

## Step 6: Query It Back

Read the leaderboard whenever you need it, for example in a dialog or a hint:

```sqf
private _result = ["SELECT name, kills FROM player_stats ORDER BY kills DESC LIMIT 10"] call a3sql_fnc_execute;
// [0, "OK", [["name","kills"],["Scarface",42],["Ghost",37]]]
```

Every `a3sql_fnc_*` call returns the same envelope: `[code, status, data]`.

- `code == 0` with status `"OK"` means success, and `data` holds the payload. For a SELECT, `data` is a header row followed by data rows: `[["name","kills"],["Scarface",42]]`.
- Any other code is an error. `data` becomes a message you can log or show, for example `[-1,"ERR_TABLE","table 'player_stats' not found"]`.

One gotcha: a SELECT whose result would exceed the 30KB response buffer does not get silently truncated. It returns an error that tells you to page the result with cursors (`cursor create <name> <query>`, then `cursor fetch <name> [limit]`). For mission-scale data you will rarely hit this; if you do, the [SQL Dialect](SQL-Dialect) page covers it.

## The Complete Example

**init.sqf**

```sqf
if (isServer) then {
    ["CREATE TABLE IF NOT EXISTS player_stats (
        uid   STRING PRIMARY KEY,
        name  STRING,
        kills INT DEFAULT 0
    )"] call a3sql_fnc_execute;

    ["my_stats.bin"] call a3sql_fnc_load;

    addMissionEventHandler ["Ended", {
        ["my_stats.bin"] call a3sql_fnc_save;
    }];
};
```

**initPlayerLocal.sqf** (server's event handler; hook `Killed` wherever your mission tracks deaths)

```sqf
private _uid  = getPlayerUID _killer;
private _name = name _killer;

["INSERT INTO player_stats (uid, name, kills) VALUES ($1, $2, 1)
  ON CONFLICT (uid) DO UPDATE SET kills = kills + 1, name = $2",
 "a3sql", [_uid, _name]] call a3sql_fnc_execute;

private _leaderboard = ["SELECT name, kills FROM player_stats ORDER BY kills DESC LIMIT 10"] call a3sql_fnc_execute;
systemChat str _leaderboard;
```

That is the whole mod: create, record, persist, query.

## Next Steps

- **[SQL Dialect](SQL-Dialect)**: the full syntax, joins, window functions, transactions, and more
- **[CBA Settings](CBA-Settings)**: auto-save/load, log level, TCP listener configuration
- **[Security](Security)**: parameterized queries and TCP authentication in depth
- **[TCP Connector](TCP-Connector)**: query your database from Python or other external tools
- **[Module Guide](Module-Guide)**: patterns for analytics, loadouts, persistence, and progression
- **[Patch Framework](Patch-Framework)**: change weapon and vehicle values live, mid-mission, from SQL

Released under the [Arma Public License Share Alike](https://www.bohemia.net/community/licenses/arma-public-license-share-alike) (APL-SA).
