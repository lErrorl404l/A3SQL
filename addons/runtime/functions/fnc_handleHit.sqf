#include "../script_component.hpp"

// Per-object HitPart handler. Look up on_hit rules that match the ammo
// or selection and apply them to the target. The incoming damage is
// derived from the projectile velocity so the damage multiplier is
// deterministic and server-authoritative.
params [
    ["_target", objNull, [objNull]],
    ["_shooter", objNull, [objNull]],
    ["_ammo", "", [""]],
    ["_selection", "", [""]],
    ["_projectile", objNull, [objNull]]
];

private _rules = missionNamespace getVariable [QGVAR(rules), createHashMap];
private _list = _rules getOrDefault ["on_hit", []];

// Estimate incoming damage from projectile speed for the multiplier.
private _incoming = if (isNull _projectile) then { 0 } else { vectorMagnitude (velocity _projectile) / 100 };

{
    private _rule = _x;
    private _matchType = toLower (_rule getOrDefault ["match_type", "exact"]);
    private _matchValue = _rule getOrDefault ["match_value", ""];
    private _matched = false;

    switch (_matchType) do {
        case "all": { _matched = true; };
        case "exact": { _matched = (_ammo == _matchValue) || (_selection == _matchValue); };
        case "type_of": { _matched = (_ammo isKindOf _matchValue); };
        case "wildcard": { _matched = [_ammo, _matchValue] call CBA_fnc_matchesWildcard; };
        case "regex": { _matched = [_ammo, _matchValue] call CBA_fnc_matchesRegex; };
    };

    if (_matched) then {
        private _context = createHashMap;
        _context set ["ammo", _ammo];
        _context set ["selection", _selection];
        _context set ["shooter", _shooter];
        _context set ["projectile", _projectile];
        _context set ["incomingDamage", _incoming];

        [_rule, _target, _context] call FUNC(apply);

        if (["a3sql_runtime_log_level"] call CBA_fnc_getSetting >= 2) then {
            ["A3SQL Runtime", "Applied on_hit rule %1 to %2 (incoming %3)", _rule getOrDefault ["name", ""], typeOf _target, _incoming] call CBA_fnc_info;
        };
    };
} forEach _list;
