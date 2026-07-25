# CBA Settings

All settings are under the **A3SQL** category in CBA Settings (in-game → Options → Addon Options).

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `a3sql_listener_enabled` | CHECKBOX | true | Start TCP listener at game boot for external queries |
| `a3sql_listener_port` | EDIT | `33306` | TCP port for external query listener |
| `a3sql_listener_bind` | EDIT | `127.0.0.1` | IP to bind to: `127.0.0.1` (localhost) or `0.0.0.0` (network) |
| `a3sql_listener_user` | EDIT | (empty) | Username required for TCP login. Empty = anonymous access |
| `a3sql_listener_password` | EDIT | (empty) | Password required for TCP login. Empty = anonymous access |
| `a3sql_auto_save` | CHECKBOX | false | Auto-save database to file when mission ends |
| `a3sql_auto_load` | CHECKBOX | false | Restore database from file when mission starts |
| `a3sql_auto_save_path` | EDIT | `a3sql_autosave.bin` | File path relative to Arma 3 directory, or absolute path |
| `a3sql_log_level` | LIST | INFO (2) | Verbosity: ERROR(0), WARN(1), INFO(2), DEBUG(3) |

**Auto-start behavior**: When `a3sql_listener_enabled` is true, the TCP listener starts at game boot (main menu) — no mission required. Credentials are applied before the listener starts.

## Setting from SQF

Override settings in a mission's `init.sqf` or `description.ext`:

```sqf
// In init.sqf
["a3sql_listener_port", 33307] call CBA_fnc_setVar;
["a3sql_listener_enabled", true] call CBA_fnc_setVar;
["a3sql_auto_save", true] call CBA_fnc_setVar;
["a3sql_auto_save_path", "my_mission_stats.bin"] call CBA_fnc_setVar;
```
