#include "script_component.hpp"

/*
    Divides a value by a numeric parameter. Returns 0 on division by zero.
    Params: [value, param]
    Returns: number
*/
params [
    ["_value", 0, [0, ""]],
    ["_param", 0, [0, ""]]
];

private _p = parseNumber _param;
if (_p == 0) then { 0 } else { (parseNumber _value) / _p }
