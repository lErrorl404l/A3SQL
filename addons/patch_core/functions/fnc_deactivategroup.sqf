#include "../script_component.hpp"

params [["_groupName", "", [""]]];

if (_groupName == "") exitWith {};

private _sql = format ["UPDATE patch_rules SET active=0 WHERE group_name = '%1'", _groupName];
_sql call a3sql_fnc_execute;

call a3sql_patch_core_fnc_setDirty;
