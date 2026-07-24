#include "script_component.hpp"

["a3db_listener_enabled", "CHECKBOX",
    ["Enable TCP Listener", "Start TCP listener on mission start for external queries."],
    "A3DB", true, false
] call CBA_fnc_addSetting;

["a3db_listener_port", "STRING",
    ["Listener Port", "TCP port for external query listener."],
    "A3DB", "33306", false
] call CBA_fnc_addSetting;

["a3db_listener_bind", "STRING",
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

["a3db_auto_save_path", "STRING",
    ["Auto-Save File Path", "File path relative to Arma 3 directory, or absolute path."],
    "A3DB", "a3db_autosave.bin", false
] call CBA_fnc_addSetting;

["a3db_log_level", "LIST",
    ["Log Level", "Verbosity of .rpt diagnostic messages."],
    "A3DB",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;
