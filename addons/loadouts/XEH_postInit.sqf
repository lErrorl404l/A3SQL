#include "script_component.hpp"

params [["_extension", "a3sql"]];

private _sql = "CREATE TABLE IF NOT EXISTS loadout_templates (id INTEGER PRIMARY KEY, faction TEXT, role TEXT, name TEXT, uniform TEXT, vest TEXT, helmet TEXT, backpack TEXT, primary_weapon TEXT, secondary_weapon TEXT, handgun_weapon TEXT, items_json TEXT, magazines_json TEXT, created_at TEXT)";
private _result = _extension callExtension _sql;

if (["a3sql_loadouts_debug"] call CBA_fnc_getSetting) then {
    diag_log text format ["[A3SQL Loadouts] Table init: %1", _result];
};

_result
