#include "script_component.hpp"

["a3sql_loadouts_debug", "CHECKBOX",
    [LSTRING(Debug_DisplayName), LSTRING(Debug_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;
