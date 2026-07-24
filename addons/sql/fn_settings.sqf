#include "script_component.hpp"

// ── TCP Listener ───────────────────────────────────────────────────────

["a3db_header_listener", "HEADER",
    ["TCP Listener", "External TCP socket for querying the database from outside Arma."],
    "A3DB", false
] call CBA_fnc_addSetting;

["a3db_listener_enabled", "CHECKBOX",
    ["Enable TCP Listener", "Start TCP listener on mission start so external tools can query the database."],
    "A3DB", false, false
] call CBA_fnc_addSetting;

["a3db_listener_port", "STRING",
    ["Listener Port", "TCP port for the external query listener."],
    "A3DB", "33306", false
] call CBA_fnc_addSetting;

["a3db_listener_bind", "STRING",
    ["Listener Bind Address", "IP address to bind to. Default 127.0.0.1 (localhost only). Set to 0.0.0.0 for network access."],
    "A3DB", "127.0.0.1", false
] call CBA_fnc_addSetting;

// ── Auto-Persistence ───────────────────────────────────────────────────

["a3db_header_persistence", "HEADER",
    ["Auto-Persistence", "Automatically save and restore the database between missions."],
    "A3DB", false
] call CBA_fnc_addSetting;

["a3db_auto_save", "CHECKBOX",
    ["Auto-Save on Mission End", "Save the database to file when a mission ends."],
    "A3DB", false, false
] call CBA_fnc_addSetting;

["a3db_auto_load", "CHECKBOX",
    ["Auto-Load on Mission Start", "Restore the database from file when a mission starts."],
    "A3DB", false, false
] call CBA_fnc_addSetting;

["a3db_auto_save_path", "STRING",
    ["Auto-Save File Path", "File path for auto-save and auto-load. Relative to Arma 3 directory or absolute."],
    "A3DB", "a3db_autosave.bin", false
] call CBA_fnc_addSetting;

// ── Diagnostics ────────────────────────────────────────────────────────

["a3db_header_diagnostics", "HEADER",
    ["Diagnostics", "Control verbosity of a3db log output for troubleshooting."],
    "A3DB", false
] call CBA_fnc_addSetting;

["a3db_log_level", "LIST",
    ["Log Level", "Verbosity of a3db diagnostic messages in the .rpt file."],
    "A3DB",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;
