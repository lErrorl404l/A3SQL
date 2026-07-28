#include "../script_component.hpp"

params [["_unit", objNull, [objNull]]];

if (!isServer) exitWith {};
if !(["a3sql_progression_enabled"] call CBA_fnc_getSetting) exitWith {};

private _units = if (isNull _unit) then {
    allUnits select {isPlayer _x}
} else {
    [_unit]
};

private _now = str date;
private _rankOrder = ["PRIVATE", "CORPORAL", "SERGEANT", "LIEUTENANT", "CAPTAIN", "MAJOR", "COLONEL"];

{
    private _uid = getPlayerUID _x;
    if (_uid == "") then { continue };

    private _rank = toUpper (rank _x);
    private _score = score _x;
    private _name = name _x;

    // ── Analytics reconciliation ────────────────────────────────
    private _kills = 0;
    private _deaths = 0;

    private _killResult = [format ["SELECT COUNT(*) as cnt FROM events_kills WHERE killer_uid = '%1'", _uid]] call a3sql_database_fnc_selectMap;
    if !(_killResult isEqualTo []) then { _kills = (_killResult select 0) get "cnt"; };

    private _deathResult = [format ["SELECT COUNT(*) as cnt FROM events_kills WHERE victim_uid = '%1'", _uid]] call a3sql_database_fnc_selectMap;
    if !(_deathResult isEqualTo []) then { _deaths = (_deathResult select 0) get "cnt"; };

    // ── Escape strings for SQL ──────────────────────────────────
    _name = _name regexReplace ["'", "''"];
    private _mission = missionName regexReplace ["'", "''"];

    // ── UPSERT core fields ──────────────────────────────────────
    private _sql = format [
        "INSERT OR REPLACE INTO player_progression (uid, name, current_rank, score, total_kills, total_deaths, missions_played, playtime_seconds, last_seen, last_mission) VALUES ('%1', '%2', '%3', %4, %5, %6, COALESCE((SELECT missions_played FROM player_progression WHERE uid = '%1'), 0) + 1, COALESCE((SELECT playtime_seconds FROM player_progression WHERE uid = '%1'), 0), '%7', '%8')",
        _uid, _name, _rank, _score, _kills, _deaths, _now, _mission
    ];
    _sql call a3sql_database_fnc_execute;

    // ── Update highest_rank if current rank is higher ───────────
    private _highestResult = [format ["SELECT highest_rank FROM player_progression WHERE uid = '%1'", _uid]] call a3sql_database_fnc_selectMap;
    if !(_highestResult isEqualTo []) then {
        private _storedHighest = (_highestResult select 0) getOrDefault ["highest_rank", ""];
        if (_storedHighest == "") then {
            [format ["UPDATE player_progression SET highest_rank = '%1' WHERE uid = '%2'", _rank, _uid]] call a3sql_database_fnc_execute;
        } else {
            private _currentIdx = _rankOrder find _rank;
            private _storedIdx = _rankOrder find (toUpper _storedHighest);
            if (_currentIdx > _storedIdx) then {
                [format ["UPDATE player_progression SET highest_rank = '%1' WHERE uid = '%2'", _rank, _uid]] call a3sql_database_fnc_execute;
            };
        };
    };

    if (["a3sql_progression_log_verbose"] call CBA_fnc_getSetting) then {
        ["A3SQL Progression", "Updated %1 (%2): rank=%3 score=%4 kills=%5 deaths=%6", _name, _uid, _rank, _score, _kills, _deaths] call CBA_fnc_info;
    };
} forEach _units;
