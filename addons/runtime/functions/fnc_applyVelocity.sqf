#include "../script_component.hpp"

// Built-in apply function: modify projectile velocity.
// Called by fnc_apply when apply_function = "a3sql_runtime_fnc_applyVelocity".
// Reads current velocity, applies the operator with the rule value.
params [
    ["_target", objNull, [objNull]],
    ["_rule", createHashMap, [createHashMap]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];
private _vel      = velocity _target;

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
