#include "../script_component.hpp"

/*
    Formats a string with the original value and target object.
    Params: [originalValue, value (format string with %1/%2), target, property]
    Returns: string
*/
params [
    ["_originalValue", "", [""]],
    ["_value", "", [""]],
    ["_target", objNull, [objNull]],
    ["_property", "", [""]]
];

format [_value, _originalValue, _target]
