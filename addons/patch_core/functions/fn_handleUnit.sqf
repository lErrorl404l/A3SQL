#include "../script_component.hpp"

params [["_matchValue", "", [""]], ["_property", "", [""]], ["_value", "", [""]]];
private _targets = allUnits select { typeOf _x == _matchValue };
{
    _x setSkill [_property, parseNumber _value];
} forEach _targets;
_targets
