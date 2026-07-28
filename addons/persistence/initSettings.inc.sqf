#include "script_component.hpp"

["a3sql_persistence_enabled", "CHECKBOX",
    [LSTRING(Enabled_DisplayName), LSTRING(Enabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_restore_on_jip", "CHECKBOX",
    [LSTRING(RestoreOnJIP_DisplayName), LSTRING(RestoreOnJIP_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_debug", "CHECKBOX",
    [LSTRING(Debug_DisplayName), LSTRING(Debug_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;
