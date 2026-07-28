#include "../script_component.hpp"

params ["_unit", "_id", "_uid", "_name"];

if (isNull _unit) exitWith {};
if !(["a3sql_persistence_enabled"] call CBA_fnc_getSetting) exitWith {};

[_uid, _unit] call a3sql_persistence_fnc_savePlayer;
