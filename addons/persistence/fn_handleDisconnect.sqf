#include "script_component.hpp"

params ["_unit", "_id", "_uid", "_name"];

if (isNull _unit) exitWith {};
if (!missionNamespace getVariable ["a3sql_persistence_enabled", true]) exitWith {};

[_uid, _unit] call a3sql_persistence_fnc_savePlayer;
