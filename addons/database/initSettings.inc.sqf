
["a3sql_listener_enabled", "CHECKBOX",
    [LSTRING(ListenerEnabled_DisplayName), LSTRING(ListenerEnabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_listener_port", "EDITBOX",
    [LSTRING(ListenerPort_DisplayName), LSTRING(ListenerPort_Description)],
    LSTRING(Category), "33306", false
] call CBA_fnc_addSetting;

["a3sql_listener_bind", "EDITBOX",
    [LSTRING(ListenerBind_DisplayName), LSTRING(ListenerBind_Description)],
    LSTRING(Category), "127.0.0.1", false
] call CBA_fnc_addSetting;

["a3sql_auto_save", "CHECKBOX",
    [LSTRING(AutoSave_DisplayName), LSTRING(AutoSave_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;

["a3sql_auto_load", "CHECKBOX",
    [LSTRING(AutoLoad_DisplayName), LSTRING(AutoLoad_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;

["a3sql_auto_save_path", "EDITBOX",
    [LSTRING(AutoSavePath_DisplayName), LSTRING(AutoSavePath_Description)],
    LSTRING(Category), "a3sql_autosave.bin", false
] call CBA_fnc_addSetting;

["a3sql_log_level", "LIST",
    [LSTRING(LogLevel_DisplayName), LSTRING(LogLevel_Description)],
    LSTRING(Category),
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_listener_user", "EDITBOX",
    [LSTRING(ListenerUser_DisplayName), LSTRING(ListenerUser_Description)],
    LSTRING(Category), "", false
] call CBA_fnc_addSetting;

["a3sql_listener_password", "EDITBOX",
    [LSTRING(ListenerPassword_DisplayName), LSTRING(ListenerPassword_Description)],
    LSTRING(Category), "", false
] call CBA_fnc_addSetting;
