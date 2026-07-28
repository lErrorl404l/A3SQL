#include "..\script_component.hpp"

params [
    ["_target", cursorObject, [objNull]]
];

if (isNull _target) exitWith {
    systemChat "[A3SQL Patch] No target object under crosshair";
};

// Fire event so all matching rules apply to this target type
["a3sql_patch_applied_from_menu", [typeOf _target]] call CBA_fnc_serverEvent;

systemChat format ["[A3SQL Patch] Rules applied to %1", typeOf _target];

[0, "OK", typeOf _target]
