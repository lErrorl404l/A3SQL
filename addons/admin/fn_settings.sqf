#include "script_component.hpp"

["a3sql_admin_enabled", "CHECKBOX",
    ["Enabled", "Enable server command execution via SQL queue."],
    "A3SQL Admin", true, false
] call CBA_fnc_addSetting;

["a3sql_admin_log_level", "LIST",
    ["Log Level", "Verbosity of .rpt diagnostic messages for admin commands."],
    "A3SQL Admin",
    [[0, 1, 2], ["ERROR", "INFO", "DEBUG"], 1],
    false
] call CBA_fnc_addSetting;
