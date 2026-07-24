#include "script_component.hpp"

["a3db_listener_enabled", "CHECKBOX",
    ["Enable TCP Listener", "Start TCP listener on mission start for external queries."],
    "A3DB", true, false
] call CBA_fnc_addSetting;

["a3db_listener_port", "EDIT",
    ["Listener Port", "TCP port for external query listener."],
    "A3DB", "33306", false
] call CBA_fnc_addSetting;

["a3db_listener_bind", "EDIT",
    ["Listener Bind Address", "IP to bind to: 127.0.0.1 (localhost) or 0.0.0.0 (network)."],
    "A3DB", "127.0.0.1", false
] call CBA_fnc_addSetting;

["a3db_auto_save", "CHECKBOX",
    ["Auto-Save on Mission End", "Save database to file when mission ends."],
    "A3DB", false, false
] call CBA_fnc_addSetting;

["a3db_auto_load", "CHECKBOX",
    ["Auto-Load on Mission Start", "Restore database from file when mission starts."],
    "A3DB", false, false
] call CBA_fnc_addSetting;

["a3db_auto_save_path", "EDIT",
    ["Auto-Save File Path", "File path relative to Arma 3 directory, or absolute path."],
    "A3DB", "a3db_autosave.bin", false
] call CBA_fnc_addSetting;

["a3db_log_level", "LIST",
    ["Log Level", "Verbosity of .rpt diagnostic messages."],
    "A3DB",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3db_listener_user", "EDIT",
    ["Listener Username", "Username required for TCP login. Leave empty for anonymous access."],
    "A3DB", "", false
] call CBA_fnc_addSetting;

["a3db_listener_password", "EDIT",
    ["Listener Password", "Password required for TCP login. Leave empty for anonymous access."],
    "A3DB", "", false
] call CBA_fnc_addSetting;

// ── Auto-start listener at game startup ─────────────────────────────
// Extended_PreInit_EventHandlers ensures this runs before the main menu.
if (missionNamespace getVariable ["a3db_listener_enabled", true]) then {
    private _ext = "a3db";
    private _port_str = missionNamespace getVariable ["a3db_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };

    // Pass credentials to the extension before starting the listener
    private _user = missionNamespace getVariable ["a3db_listener_user", ""];
    private _pass = missionNamespace getVariable ["a3db_listener_password", ""];
    if (_user != "" && _pass != "") then {
        _ext callExtension ["set_credentials", [_user, _pass]];
    };

    private _result = _ext callExtension ["listen", [str _port]];
    if (missionNamespace getVariable ["a3db_log_level", 2] >= 2) then {
        diag_log text format ["[A3DB] Listener on port %1: %2", _port, _result];
    };
};
