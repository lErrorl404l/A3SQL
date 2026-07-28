#include "script_component.hpp"

if (!isServer) exitWith {};

[objNull] call a3sql_progression_fnc_updateRank;

if (missionNamespace getVariable ["a3sql_progression_log_verbose", false]) then {
    diag_log text "[A3SQL Progression] Mission-end progression update complete";
};
