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

        // Projectile-scoped properties (velocity) apply only to the round;
        // unit-scoped properties apply to the shooter.
        private _property = toLower (_rule getOrDefault ["target_property", ""]);
        if !(isNull _projectile) then {
            [_rule, _projectile, _context] call FUNC(apply);
        };
        if !(_property in ["velocity", "damage"]) then {
            [_rule, _unit, _context] call FUNC(apply);
        };

        if (["a3sql_runtime_log_level"] call CBA_fnc_getSetting >= 2) then {
            ["A3SQL Runtime", "Applied on_fire rule %1 to %2", _rule getOrDefault ["name", ""], _ammo] call CBA_fnc_info;
        };
    };
} forEach _list;
