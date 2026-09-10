#include "../script_component.hpp"

// Apply a single rule to a target object with event context.
// The rule is a hashmap from the in-memory cache. The context hashmap
// carries event-specific fields (ammo, selection, projectile, shooter).
params [
    ["_rule", createHashMap, [createHashMap]],
    ["_target", objNull, [objNull]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _property = _rule getOrDefault ["target_property", ""];
private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];

// ── Special properties handled before the generic operator switch ──
switch (toLower _property) do {
    case "damage": {
        // Damage multiplier: apply the operator to the incoming damage
        // carried in the context, then add it to the target's damage.
        private _incoming = _context getOrDefault ["incomingDamage", 0];
        private _result = _incoming;
        switch (_operator) do {
            case "mul": { _result = _incoming * (parseNumber _value); };
            case "add": { _result = _incoming + (parseNumber _value); };
            case "div": { if ((parseNumber _value) != 0) then { _result = _incoming / (parseNumber _value); }; };
            case "set": { _result = parseNumber _value; };
            case "clamp": {
                private _max = parseNumber _value;
                _result = _incoming min _max;
            };
        };
        _target setDamage ((damage _target) + _result);
        [0, "OK", _result]
    };
    case "velocity": {
        // Set or scale the projectile velocity vector.
        private _vel = velocity _target;
        switch (_operator) do {
            case "set": {
                private _parts = _value splitString " ,";
                if (count _parts >= 3) then {
                    _target setVelocity [(parseNumber (_parts select 0)), (parseNumber (_parts select 1)), (parseNumber (_parts select 2))];
                };
            };
            case "mul": {
                private _scale = parseNumber _value;
                _target setVelocity (_vel vectorMultiply _scale);
            };
            case "add": {
                private _parts = _value splitString " ,";
                if (count _parts >= 3) then {
                    _target setVelocity (_vel vectorAdd [(parseNumber (_parts select 0)), (parseNumber (_parts select 1)), (parseNumber (_parts select 2))]);
                };
            };
        };
        [0, "OK", velocity _target]
    };
    default {
        // ── Generic variable operators (mirror patch_core) ──
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
    };
};
