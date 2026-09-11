#include "../script_component.hpp"

params [
    ["_extension", "a3sql", [""]]
];

// ── Load active rules into an in-memory cache keyed by event ───────
private _rows = ["SELECT id, name, event, match_type, match_value, target_property, operator, value, priority FROM runtime_overrides WHERE active = 1 ORDER BY priority DESC", _extension] call a3sql_database_fnc_selectMap;

private _cache = createHashMap;
{
    private _event = toLower (_x getOrDefault ["event", ""]);
    if (_event isNotEqualTo "") then {
        private _list = _cache getOrDefault [_event, []];
        _list pushBack _x;
        _cache set [_event, _list];
    };
} forEach _rows;

missionNamespace setVariable [QGVAR(rules), _cache];

private _logLevel = missionNamespace getVariable ["a3sql_runtime_log_level", 1];
if (_logLevel >= 2) then {
    ["A3SQL Runtime", "Loaded %1 active rules into cache", count _rows] call CBA_fnc_info;
};

[0, "OK", count _rows]
