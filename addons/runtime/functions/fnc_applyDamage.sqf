#include "../script_component.hpp"

// Built-in apply function: modify target damage.
// Called by fnc_apply when apply_function = "a3sql_runtime_fnc_applyDamage".
// Reads incomingDamage from context, applies the operator, adds to target damage.
params [
    ["_target", objNull, [objNull]],
    ["_rule", createHashMap, [createHashMap]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];
private _incoming = _context getOrDefault ["incomingDamage", 0];

private _result = _incoming;
switch (_operator) do {
    case "mul":   { _result = _incoming * (parseNumber _value); };
    case "add":   { _result = _incoming + (parseNumber _value); };
    case "div":   { if ((parseNumber _value) != 0) then { _result = _incoming / (parseNumber _value); }; };
    case "set":   { _result = parseNumber _value; };
    case "clamp": { _result = _incoming min (parseNumber _value); };
};

_target setDamage ((damage _target) + _result);
[0, "OK", _result]
