#include "..\script_component.hpp"

/*
    Multiplies a value by a numeric parameter.
    Params: [value, param]
    Returns: number
*/
params [
    ["_value", 0, [0, ""]],
    ["_param", 0, [0, ""]]
];

(parseNumber _value) * (parseNumber _param)
