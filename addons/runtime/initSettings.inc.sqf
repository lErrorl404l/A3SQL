["a3sql_runtime_enabled", "CHECKBOX",
    [LSTRING(Enabled_DisplayName), LSTRING(Enabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_runtime_log_level", "LIST",
    [LSTRING(LogLevel_DisplayName), LSTRING(LogLevel_Description)],
    LSTRING(Category),
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 1],
    false
] call CBA_fnc_addSetting;

["a3sql_runtime_poll_hz", "SLIDER",
    [LSTRING(PollHz_DisplayName), LSTRING(PollHz_Description)],
    LSTRING(Category),
    [0, 20, 5, 0],
    false
] call CBA_fnc_addSetting;
