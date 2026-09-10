#include "../script_component.hpp"

// Mission-wide EntityKilled handler. Look up on_killed rules and apply
// them to the killed unit or the killer. Typical use: reward points or
// log the kill to the database.
params [
    ["_killed", objNull, [objNull]],
    ["_killer", objNull, [objNull]]
];

private _rules = missionNamespace getVariable [QGVAR(rules), createHashMap];
private _list = _rules getOrDefault ["on_killed", []];

{
    private _rule = _x;
    private _matchType = toLower (_rule getOrDefault ["match_type", "exact"]);
    private _matchValue = _rule getOrDefault ["match_value", ""];
    private _matched = false;

    switch (_matchType) do {
        case "all": { _matched = true; };
        case "exact": { _matched = (typeOf _killed == _matchValue) || (typeOf _killer == _matchValue); };
        case "type_of": { _matched = (_killed isKindOf _matchValue) || (_killer isKindOf _matchValue); };
        case "wildcard": { _matched = [typeOf _killed, _matchValue] call CBA_fnc_matchesWildcard; };
        case "regex": { _matched = [typeOf _killed, _matchValue] call CBA_fnc_matchesRegex; };
    };

    if (_matched) then {
        private _context = createHashMap;
        _context set ["killed", _killed];
        _context set ["killer", _killer];

        [_rule, _killed, _context] call FUNC(apply);
        if !(isNull _killer) then {
            [_rule, _killer, _context] call FUNC(apply);
        };

        if (["a3sql_runtime_log_level"] call CBA_fnc_getSetting >= 2) then {
            ["A3SQL Runtime", "Applied on_killed rule %1 to %2", _rule getOrDefault ["name", ""], typeOf _killed] call CBA_fnc_info;
        };
    };
} forEach _list;
