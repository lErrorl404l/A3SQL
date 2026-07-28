#include "../script_component.hpp"

params [
    ["_id", 0, [0]]
];

if (_id <= 0) exitWith { [1, "ERR_PARAM", "Invalid template ID"] };

private _sql = format ["DELETE FROM loadout_templates WHERE id = %1", _id];
private _response = "a3sql" callExtension _sql;
private _parsed = parseSimpleArray _response;

if (["a3sql_loadouts_debug"] call CBA_fnc_getSetting && {(count _parsed) >= 1 && {(_parsed select 0) == 0}}) then {
    ["A3SQL Loadouts", "Template %1 deleted", _id] call CBA_fnc_info;
};

_parsed
