#include "script_component.hpp"

/*
    Replaces substrings in the original value using regex.
    Params: [originalValue, value ("search|replacement"), target, property]
    Returns: string
*/
params [
    ["_originalValue", "", [""]],
    ["_value", "", [""]],
    ["_target", objNull, [objNull]],
    ["_property", "", [""]]
];

private _parts = _value splitString "|";
private _search = if (count _parts >= 1) then { _parts select 0 } else { "" };
private _replacement = if (count _parts >= 2) then { _parts select 1 } else { "" };

_originalValue regexReplace [_search, _replacement]
