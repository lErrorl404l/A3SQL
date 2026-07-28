#include "../script_component.hpp"

params [["_groupName", "", [""]]];

if (_groupName == "") exitWith {};

private _sql = format ["SELECT * FROM patch_rules WHERE group_name = '%1' AND active=1 ORDER BY priority", _groupName];
private _rows = [_sql] call a3sql_fnc_selectMap;

{ [_x] call a3sql_patch_core_fnc_applyRule; } forEach _rows;
