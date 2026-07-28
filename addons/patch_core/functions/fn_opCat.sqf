#include "../script_component.hpp"

/*
    Concatenates a parameter string to a value.
    Params: [value, param]
    Returns: string
*/
params [
    ["_value", "", ["", 0]],
    ["_param", "", ["", 0]]
];

str _value + str _param
