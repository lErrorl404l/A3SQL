#include "script_component.hpp"

/*
    Subtracts a numeric parameter from a value.
    Params: [value, param]
    Returns: number
*/
params [
    ["_value", 0, [0, ""]],
    ["_param", 0, [0, ""]]
];

(parseNumber _value) - (parseNumber _param)
