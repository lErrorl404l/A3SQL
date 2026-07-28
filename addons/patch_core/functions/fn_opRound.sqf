#include "..\script_component.hpp"

/*
    Rounds a numeric value to the specified decimal places.
    Params: [originalValue, value (precision as string), target, property]
    Returns: string
*/
params [
    ["_originalValue", "", [""]],
    ["_value", "0", [""]],
    ["_target", objNull, [objNull]],
    ["_property", "", [""]]
];

private _num = parseNumber _originalValue;
private _precision = parseNumber _value;

if (_precision <= 0) then {
    str (round _num)
} else {
    str (round (_num * (10 ^ _precision)) / (10 ^ _precision))
};
