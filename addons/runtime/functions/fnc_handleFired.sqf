#include "../script_component.hpp"

// Mission-wide Fired handler. Look up on_fire rules that match the
// weapon or ammo classname and apply them to the projectile or unit.
params [
    ["_unit", objNull, [objNull]],
    ["_weapon", "", [""]],
    ["_ammo", "", [""]],
    ["_projectile", objNull, [objNull]]
];

private _rules = missionNamespace getVariable [QGVAR(rules), createHashMap];
private _list = _rules getOrDefault ["on_fire", []];

{
    private _rule = _x;
    private _matchType = toLower (_rule getOrDefault ["match_type", "exact"]);
    private _matchValue = _rule getOrDefault ["match_value", ""];
    private _matched = false;

    switch (_matchType) do {
        case "all": { _matched = true; };
        case "exact": { _matched = (_weapon == _matchValue) || (_ammo == _matchValue); };
        case "type_of": { _matched = (_ammo isKindOf _matchValue); };
        case "wildcard": { _matched = [_ammo, _matchValue] call CBA_fnc_matchesWildcard; };
        case "regex": { _matched = [_ammo, _matchValue] call CBA_fnc_matchesRegex; };
    };

    if (_matched) then {
        private _context = createHashMap;
        _context set ["weapon", _weapon];
        _context set ["ammo", _ammo];
        _context set ["projectile", _projectile];

        // Apply the rule to the projectile (for velocity/speed changes)
        // and to the unit (for variable-based rules).
        if !(isNull _projectile) then {
            [_rule, _projectile, _context] call FUNC(apply);
        };
        [_rule, _unit, _context] call FUNC(apply);

        // ponytail: read CBA setting var directly. CBA_fnc_getSetting
        // can return nil during hot events (Fired) when CBA isn't fully init'd.
        private _logLevel = missionNamespace getVariable ["a3sql_runtime_log_level", 1];
        if (_logLevel >= 2) then {
            ["A3SQL Runtime", "Applied on_fire rule %1 to %2", _rule getOrDefault ["name", ""], _ammo] call CBA_fnc_info;
        };
    };
} forEach _list;
