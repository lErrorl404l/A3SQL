#include "../script_component.hpp"

params [["_matchValue", "", [""]], ["_property", "", [""]], ["_value", "", [""]]];
private _targets = vehicles select { typeOf _x == _matchValue };
{
    switch (toLower _property) do {
        case "fuel": { _x setFuel (parseNumber _value); };
        case "damage": { _x setDamage (parseNumber _value); };
        case "ammo": { _x setVehicleAmmo 1; };
        default { _x setVariable [_property, _value]; };
    };
} forEach _targets;
_targets
