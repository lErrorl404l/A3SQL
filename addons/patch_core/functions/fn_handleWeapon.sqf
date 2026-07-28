#include "../script_component.hpp"

params [["_matchValue", "", [""]], ["_property", "", [""]], ["_value", "", [""]]];
private _targets = [];
{
    private _weapon = primaryWeapon _x;
    if (_weapon == _matchValue) then {
        _x setVehicleAmmo parseNumber _value;
        _targets pushBack _x;
    };
    _weapon = secondaryWeapon _x;
    if (_weapon == _matchValue) then {
        _x setVehicleAmmo parseNumber _value;
        _targets pushBack _x;
    };
    _weapon = handgunWeapon _x;
    if (_weapon == _matchValue) then {
        _x setVehicleAmmo parseNumber _value;
        _targets pushBack _x;
    };
} forEach (allUnits + vehicles);
_targets
