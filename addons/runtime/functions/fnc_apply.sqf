#include "../script_component.hpp"

// Apply a single rule to a target object with event context.
// The rule is a hashmap from the in-memory cache. The context hashmap
// carries event-specific fields (ammo, selection, projectile, shooter).
//
// If apply_function is set, call that function instead of the generic
// operator switch. This makes the engine general-purpose: any SQF
// function can be registered as an apply function via the DB.
params [
    ["_rule", createHashMap, [createHashMap]],
    ["_target", objNull, [objNull]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _applyFunction = toLower (_rule getOrDefault ["apply_function", ""]);

// ── Custom apply function (general-purpose path) ───────────────────
// When apply_function is set, call it directly. The function receives
// [target, rule, context] and handles its own logic. This replaces
// the old hardcoded damage/velocity cases.
if (_applyFunction isNotEqualTo "") then {
    private _fnc = missionNamespace getVariable [_applyFunction, {}];
    if (_fnc isEqualTo {}) exitWith {
        [1, "ERR_FUNC", format ["Apply function '%1' not found", _applyFunction]]
    };
    [_target, _rule, _context] call _fnc
} else {
    // ── Generic variable operators (default path) ──────────────────────
    // For rules without apply_function, use set/add/mul/div/clamp/call
    // on object variables. This is the general-purpose fallback.
    private _property = _rule getOrDefault ["target_property", ""];
    private _operator = toLower (_rule getOrDefault ["operator", "set"]);
    private _value    = _rule getOrDefault ["value", ""];

    private _current = _target getVariable [_property, nil];
    private _result = _current;
    switch (_operator) do {
        case "set":   { _result = _value; };
        case "add":   { _result = (parseNumber _current) + (parseNumber _value); };
        case "mul":   { _result = (parseNumber _current) * (parseNumber _value); };
        case "div":   { if ((parseNumber _value) != 0) then { _result = (parseNumber _current) / (parseNumber _value); }; };
        case "clamp": { _result = (parseNumber _current) min (parseNumber _value); };
        case "call": {
            private _fnc = missionNamespace getVariable [_property, {}];
            [_target, _value, _context] call _fnc;
            _result = nil;
        };
    };
    if !(isNil "_result") then {
        _target setVariable [_property, _result];
    };
    [0, "OK", _result]
}
