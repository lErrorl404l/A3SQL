#include "..\script_component.hpp"

if (!hasInterface) exitWith {};

if (!isNull (findDisplay 12300)) exitWith {
    (findDisplay 12300) closeDisplay 2;
};

private _ok = createDialog "a3sql_patch_editor";
if (!_ok) exitWith {
    ["A3SQL Patch", "Failed to create patch editor dialog"] call CBA_fnc_error;
};

true
