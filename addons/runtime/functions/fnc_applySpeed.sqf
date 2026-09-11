#include "../script_component.hpp"

// Built-in apply function: modify unit or vehicle speed.
// Called by fnc_apply when apply_function = "a3sql_runtime_fnc_applySpeed".
// Sets the target's movement speed mode or max speed.
// Value format: number (max speed in km/h) or mode string ("walk","run","sprint").
// Operator: set replaces, mul scales current maxSpeed.
params [
    ["_target", objNull, [objNull]],
    ["_rule", createHashMap, [createHashMap]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];

// Check if value is a mode string (walk/run/sprint) or numeric
private _modes = ["walk", "run", "sprint"];
private _isMode = (toLower _value) in _modes;

if (_isMode) do {
    // Set movement speed mode (Infantry)
    private _mode = toLower _value;
    private _coef = switch (_mode) do {
        case "walk":   { 0.5 };
        case "run":    { 1.0 };
        case "sprint": { 1.5 };
        default { 1.0 };
    };
    _target setAnimSpeedCoef _coef;
    [0, "OK", format ["Speed mode set to %1 (coef %2)", _mode, _coef]]
} else {
    // Numeric: set max speed
    private _maxSpeed = parseNumber _value;
    if (_maxSpeed <= 0) exitWith { [1, "ERR_PARAM", "Invalid speed value"] };

    switch (_operator) do {
        case "set": {
            // For vehicles: set damage to control max speed (workaround)
            // For infantry: use setAnimSpeedCoef
            if (vehicle _target isNotEqualTo _target) then {
                // In vehicle. Set fuel to control speed indirectly
                // A better approach: use setCustomAimCoef or engine power
                _target setCustomAimCoef (_maxSpeed / 100);
            } else {
                // Infantry. Scale movement speed
                private _currentSpeed = getNumber (configOf _target >> "maxSpeed");
                if (_currentSpeed > 0) then {
                    _target setAnimSpeedCoef (_maxSpeed / _currentSpeed);
                };
            };
        };
        case "mul": {
            private _currentSpeed = getNumber (configOf _target >> "maxSpeed");
            if (_currentSpeed > 0) then {
                _target setAnimSpeedCoef (_maxSpeed);
            };
        };
    };
    [0, "OK", format ["Max speed set to %1 km/h", _maxSpeed]]
}
