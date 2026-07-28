#include "script_component.hpp"

ADDON = false;

#include "XEH_PREP.hpp"
#include "initSettings.inc.sqf"

// Public API aliases — stable names other addons use
missionNamespace setVariable ["a3sql_fnc_execute", missionNamespace getVariable "a3sql_database_fnc_execute"];
missionNamespace setVariable ["a3sql_fnc_selectAll", missionNamespace getVariable "a3sql_database_fnc_selectAll"];
missionNamespace setVariable ["a3sql_fnc_selectArray", missionNamespace getVariable "a3sql_database_fnc_selectArray"];
missionNamespace setVariable ["a3sql_fnc_selectMap", missionNamespace getVariable "a3sql_database_fnc_selectMap"];
missionNamespace setVariable ["a3sql_fnc_exportCSV", missionNamespace getVariable "a3sql_database_fnc_exportCSV"];

ADDON = true;
