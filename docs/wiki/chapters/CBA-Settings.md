# CBA Settings

Every A3SQL option for the person running the server lives in CBA Settings.
In game, open **Options → Addon Configuration → A3SQL**. The settings are
read on the server only, so configure them on the machine that hosts the
mission.

## The settings

### 1. TCP listener

| Setting | Type | Default | What it does |
|---|---|---|---|
| Enable TCP Listener | CHECKBOX | on | Start the query port when A3SQL loads on the server. No mission script needed. |
| Listener Port | EDIT | `33306` | Port the query listener listens on. |
| Listener Bind Address | EDIT | `127.0.0.1` | Registered for a future release. The in-game listener always binds to `127.0.0.1` (this machine only). To expose the database on the network, run the [standalone server](Standalone-Server) with `--bind 0.0.0.0`. |
| Listener Username | EDIT | (empty) | Username clients must send in `LOGIN <user> <pass>`. |
| Listener Password | EDIT | (empty) | Password clients must send in `LOGIN <user> <pass>`. |

Credentials are handed to the extension before the listener starts, but only
when **both** username and password are filled in. The listener is fail-closed:
with empty credentials it refuses every connection until you set them. See
[Security](Security).

### 2. Persistence

| Setting | Type | Default | What it does |
|---|---|---|---|
| Auto-Save | CHECKBOX | off | Save the whole database to disk when the mission ends. |
| Auto-Load | CHECKBOX | off | Load the saved database when a mission starts. |
| Auto-Save File | EDIT | `a3sql_autosave.bin` | File name, relative to the data directory (`a3sql_data/` next to the extension). |

Turn both on for persistence across restarts: Auto-Load restores the database
at mission start, Auto-Save writes it back when the mission ends. Saves are
crash-safe (temp file plus atomic rename, `.bak` fallback, checksum verified
on load). See [Security](Security) and
[Production Readiness](Production-Readiness).

### 3. Logging

| Setting | Type | Default | What it does |
|---|---|---|---|
| Log Level | LIST | INFO | How chatty A3SQL is in the server RPT. `ERROR`, `WARN`, `INFO`, or `DEBUG`. `DEBUG` is only for chasing problems. |

## Auto-start behavior

When Enable TCP Listener is on, the listener starts automatically on the
server as soon as the addon initializes. Credentials are applied before the
listener starts.

## Setting from SQF

A mission can override any setting with `CBA_fnc_setVar`, for example in
`init.sqf`:

```sqf
["a3sql_database_listener_port", 33307] call CBA_fnc_setVar;
["a3sql_listener_enabled", true] call CBA_fnc_setVar;
["a3sql_auto_save", true] call CBA_fnc_setVar;
["a3sql_database_auto_save_path", "my_mission_stats.bin"] call CBA_fnc_setVar;
```

The override must be in place before the addon reads the setting (server
init).
