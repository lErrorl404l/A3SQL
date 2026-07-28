#include "script_component.hpp"

// Apply all entity-class override rules at mission start
// Uses match_type='init' to identify config overrides (not runtime patches)

private _rows = ["SELECT * FROM patch_rules WHERE match_type='init' AND active=1 ORDER BY priority LIMIT 100"] call a3sql_fnc_selectMap;

{
    private _matchValue = _x get "match_value";
    private _targetType = _x get "target_type";
    private _property = _x get "property";
    private _operator = _x get "operator";
    private _value = _x get "value";

    // Find matching entities by target type
    private _targets = [];
    switch (toLower _targetType) do {
        case "vehicle": {
            _targets = vehicles select { typeOf _x == _matchValue };
        };
        case "unit": {
            _targets = allUnits select { typeOf _x == _matchValue };
        };
        default {
            _targets = allMissionObjects _matchValue;
        };
    };

    // Apply operator to each target
    {
        switch (toLower _operator) do {
            case "set": {
                _x setVariable [_property, _value];
            };
            case "mul": {
                _x setVariable [_property, parseNumber (_x getVariable [_property, 0]) * parseNumber _value];
            };
            default {
                _x setVariable [_property, _value];
            };
        };
    } forEach _targets;
} forEach _rows;
