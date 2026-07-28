
["a3sql_progression_enabled", "CHECKBOX",
    [LSTRING(Enabled_DisplayName), LSTRING(Enabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_progression_log_verbose", "CHECKBOX",
    [LSTRING(VerboseLogging_DisplayName), LSTRING(VerboseLogging_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;
