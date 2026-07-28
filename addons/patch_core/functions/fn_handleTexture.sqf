#include "..\script_component.hpp"

params [["_matchValue", "", [""]], ["_property", "", [""]], ["_value", "", [""]]];
private _targets = allMissionObjects "All" select { typeOf _x == _matchValue };
{
    _x setObjectTexture [0, _value];
} forEach _targets;
_targets
