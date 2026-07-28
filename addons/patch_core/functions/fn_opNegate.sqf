#include "..\script_component.hpp"

/*
    Negates a numeric value or inverts a boolean string.
    Params: [originalValue, value, target, property]
    Returns: string
*/
params [
    ["_originalValue", "", [""]],
    ["_value", "", [""]],
    ["_target", objNull, [objNull]],
    ["_property", "", [""]]
];

if (_originalValue == "true") exitWith { "false" };
if (_originalValue == "false") exitWith { "true" };

private _num = parseNumber _originalValue;
str (-_num)
