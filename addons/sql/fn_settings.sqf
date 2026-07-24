#include "script_component.hpp"

["a3db_listener_enabled", "CHECKBOX",
    ["Enable TCP Listener", "Start TCP listener on mission start for external queries."],
    "A3DB", false, true
] call CBA_fnc_addSetting;

["a3db_listener_port", "SLIDER",
    ["Listener Port", "TCP port for external query listener."],
    "A3DB", [1024, 65535, 33306, 0], true
] call CBA_fnc_addSetting;

["a3db_auto_save", "CHECKBOX",
    ["Auto-Save on Mission End", "Save database to file when mission ends."],
    "A3DB", false, true
] call CBA_fnc_addSetting;

["a3db_auto_save_path", "STRING",
    ["Auto-Save File", "File path for auto-save binary dump."],
    "A3DB", "a3db_autosave.bin", true
] call CBA_fnc_addSetting;
