#include "script_component.hpp"

["a3sql_listener_enabled", "CHECKBOX",
    ["STR_A3SQL_Database_ListenerEnabled_DisplayName", "STR_A3SQL_Database_ListenerEnabled_Description"],
    "STR_A3SQL_Database_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_listener_port", "EDIT",
    ["STR_A3SQL_Database_ListenerPort_DisplayName", "STR_A3SQL_Database_ListenerPort_Description"],
    "STR_A3SQL_Database_Category", "33306", false
] call CBA_fnc_addSetting;

["a3sql_listener_bind", "EDIT",
    ["STR_A3SQL_Database_ListenerBind_DisplayName", "STR_A3SQL_Database_ListenerBind_Description"],
    "STR_A3SQL_Database_Category", "127.0.0.1", false
] call CBA_fnc_addSetting;

["a3sql_auto_save", "CHECKBOX",
    ["STR_A3SQL_Database_AutoSave_DisplayName", "STR_A3SQL_Database_AutoSave_Description"],
    "STR_A3SQL_Database_Category", false, false
] call CBA_fnc_addSetting;

["a3sql_auto_load", "CHECKBOX",
    ["STR_A3SQL_Database_AutoLoad_DisplayName", "STR_A3SQL_Database_AutoLoad_Description"],
    "STR_A3SQL_Database_Category", false, false
] call CBA_fnc_addSetting;

["a3sql_auto_save_path", "EDIT",
    ["STR_A3SQL_Database_AutoSavePath_DisplayName", "STR_A3SQL_Database_AutoSavePath_Description"],
    "STR_A3SQL_Database_Category", "a3sql_autosave.bin", false
] call CBA_fnc_addSetting;

["a3sql_log_level", "LIST",
    ["STR_A3SQL_Database_LogLevel_DisplayName", "STR_A3SQL_Database_LogLevel_Description"],
    "STR_A3SQL_Database_Category",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_listener_user", "EDIT",
    ["STR_A3SQL_Database_ListenerUser_DisplayName", "STR_A3SQL_Database_ListenerUser_Description"],
    "STR_A3SQL_Database_Category", "", false
] call CBA_fnc_addSetting;

["a3sql_listener_password", "EDIT",
    ["STR_A3SQL_Database_ListenerPassword_DisplayName", "STR_A3SQL_Database_ListenerPassword_Description"],
    "STR_A3SQL_Database_Category", "", false
] call CBA_fnc_addSetting;
