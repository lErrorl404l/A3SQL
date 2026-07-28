#include "../script_component.hpp"

if (!isServer) exitWith {};

[objNull] call a3sql_progression_fnc_updateRank;

if (["a3sql_progression_log_verbose"] call CBA_fnc_getSetting) then {
    ["A3SQL Progression", "Mission-end progression update complete"] call CBA_fnc_info;
};
