#include "script_component.hpp"

/*
    Applies modulo to a value by a numeric parameter. Returns 0 on modulo by zero.
    Params: [value, param]
    Returns: number
*/
params [
    ["_value", 0, [0, ""]],
    ["_param", 0, [0, ""]]
];

private _p = parseNumber _param;
if (_p == 0) then { 0 } else { (parseNumber _value) % _p }
