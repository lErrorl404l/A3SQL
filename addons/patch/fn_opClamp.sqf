#include "script_component.hpp"

/*
    Clamps a numeric value between a minimum and maximum.
    Params: [originalValue, value ("min|max"), target, property]
    Returns: string
*/
params [
    ["_originalValue", "", [""]],
    ["_value", "0|100", [""]],
    ["_target", objNull, [objNull]],
    ["_property", "", [""]]
];

private _num = parseNumber _originalValue;
private _parts = _value splitString "|";
private _min = if (count _parts >= 1) then { parseNumber (_parts select 0) } else { 0 };
private _max = if (count _parts >= 2) then { parseNumber (_parts select 1) } else { _num };

str ((_num max _min) min _max)
