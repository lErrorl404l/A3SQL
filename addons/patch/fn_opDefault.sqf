#include "script_component.hpp"

/*
    Returns the default parameter if the value is empty/zero/nil.
    Params: [value, default]
    Returns: any (default if value is empty, otherwise value)
*/
params [
    ["_value", "", ["", 0, true, false]],
    ["_default", "", ["", 0, true, false]]
];

if (isNil "_value" || {_value isEqualTo ""} || {_value isEqualTo 0} || {_value isEqualTo false}) then {
    _default
} else {
    _value
}
