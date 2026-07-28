#include "../script_component.hpp"

params [["_matchValue", "", [""]], ["_property", "", [""]], ["_value", "", [""]]];
private _targets = [];
{
    if (_matchValue in (magazines _x)) then {
        _x removeMagazines _matchValue;
        _x addMagazine _value;
        _targets pushBack _x;
    };
} forEach (allUnits + vehicles);
_targets
