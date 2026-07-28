#include "script_component.hpp"

["a3sql_listener_enabled", "CHECKBOX",
    [["STR_a3sql_database_setting_listener_enabled", "STR_a3sql_database_setting_listener_enabled_desc"]],
    "A3SQL", true, false
] call CBA_fnc_addSetting;

["a3sql_listener_port", "EDIT",
    [["STR_a3sql_database_setting_listener_port", "STR_a3sql_database_setting_listener_port_desc"]],
    "A3SQL", "33306", false
] call CBA_fnc_addSetting;

["a3sql_listener_bind", "EDIT",
    [["STR_a3sql_database_setting_listener_bind", "STR_a3sql_database_setting_listener_bind_desc"]],
    "A3SQL", "127.0.0.1", false
] call CBA_fnc_addSetting;

["a3sql_auto_save", "CHECKBOX",
    [["STR_a3sql_database_setting_auto_save", "STR_a3sql_database_setting_auto_save_desc"]],
    "A3SQL", false, false
] call CBA_fnc_addSetting;

["a3sql_auto_load", "CHECKBOX",
    [["STR_a3sql_database_setting_auto_load", "STR_a3sql_database_setting_auto_load_desc"]],
    "A3SQL", false, false
] call CBA_fnc_addSetting;

["a3sql_auto_save_path", "EDIT",
    [["STR_a3sql_database_setting_auto_save_path", "STR_a3sql_database_setting_auto_save_path_desc"]],
    "A3SQL", "a3sql_autosave.bin", false
] call CBA_fnc_addSetting;

["a3sql_log_level", "LIST",
    [["STR_a3sql_database_setting_log_level", "STR_a3sql_database_setting_log_level_desc"]],
    "A3SQL",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_listener_user", "EDIT",
    [["STR_a3sql_database_setting_listener_user", "STR_a3sql_database_setting_listener_user_desc"]],
    "A3SQL", "", false
] call CBA_fnc_addSetting;

["a3sql_listener_password", "EDIT",
    [["STR_a3sql_database_setting_listener_password", "STR_a3sql_database_setting_listener_password_desc"]],
    "A3SQL", "", false
] call CBA_fnc_addSetting;

// ── Auto-start listener at game startup ─────────────────────────────
// Extended_PreInit_EventHandlers ensures this runs before the main menu.
if (missionNamespace getVariable ["a3sql_listener_enabled", true]) then {
    private _ext = "a3sql";
    private _port_str = missionNamespace getVariable ["a3sql_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };

    // Pass credentials to the extension before starting the listener
    private _user = missionNamespace getVariable ["a3sql_listener_user", ""];
    private _pass = missionNamespace getVariable ["a3sql_listener_password", ""];
    if (_user != "" && _pass != "") then {
        _ext callExtension ["set_credentials", [_user, _pass]];
    };

    private _result = _ext callExtension ["listen", [str _port]];
    if (missionNamespace getVariable ["a3sql_log_level", 2] >= 2) then {
        diag_log text format ["[A3SQL] Listener on port %1: %2", _port, _result];
    };
};
