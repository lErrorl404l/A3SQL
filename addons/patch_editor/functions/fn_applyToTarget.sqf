#include "..\script_component.hpp"

params [
    ["_target", cursorObject, [objNull]]
];

if (isNull _target) exitWith {
    ["A3SQL Patch", "No target object under crosshair"] call CBA_fnc_notify;
};

// Fire event so all matching rules apply to this target type
["a3sql_patch_applied_from_menu", [typeOf _target]] call CBA_fnc_serverEvent;

["A3SQL Patch", format ["Rules applied to %1", typeOf _target]] call CBA_fnc_notify;

[0, "OK", typeOf _target]
