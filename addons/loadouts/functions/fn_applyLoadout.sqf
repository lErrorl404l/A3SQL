#include "../script_component.hpp"

params [
    ["_unit", objNull, [objNull]],
    ["_faction", "", [""]],
    ["_role", "", [""]]
];

if (isNull _unit) exitWith { [1, "ERR_PARAM", "Invalid unit"] };

// Fetch template by faction + role
private _result = [0, _faction, _role] call a3sql_loadouts_fnc_getTemplate;
if ((_result select 0) != 0) exitWith { _result };

private _template = _result select 2;

// Parse JSON arrays stored as SQF-format strings
private _items = parseSimpleArray (_template getOrDefault ["items_json", "[]"]);
private _mags = parseSimpleArray (_template getOrDefault ["magazines_json", "[]"]);

// Build full setUnitLoadout array from typed columns + parsed arrays
private _loadout = [
    [_template getOrDefault ["primary_weapon", ""], [], (_mags param [0, []])],
    [_template getOrDefault ["secondary_weapon", ""], [], (_mags param [1, []])],
    [_template getOrDefault ["handgun_weapon", ""], [], (_mags param [2, []])],
    _template getOrDefault ["uniform", ""],
    _template getOrDefault ["vest", ""],
    _template getOrDefault ["backpack", ""],
    _template getOrDefault ["helmet", ""],
    [],
    [],
    (_items param [0, []]),
    (_items param [1, []]),
    (_items param [2, []]),
    (_items param [3, []])
];

_unit setUnitLoadout _loadout;

if (["a3sql_loadouts_debug"] call CBA_fnc_getSetting) then {
    ["A3SQL Loadouts", format ["Applied %1 / %2 to %3", _faction, _role, name _unit]] call CBA_fnc_notify;
};

[0, "OK", _loadout]
