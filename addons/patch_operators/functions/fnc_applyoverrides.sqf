#include "../script_component.hpp"

// Apply all entity-class override rules at mission start
// Uses match_type='init' to identify config overrides (not runtime patches)
// Uses CBA_fnc_addClassEventHandler for event-driven application on future entities

private _overrides = ["SELECT * FROM patch_rules WHERE match_type='init' AND active=1 ORDER BY priority LIMIT 100"] call a3sql_database_fnc_selectMap;

// Register class event handlers so newly created entities get overrides applied
{
    private _class = _x get "match_value";
    private _property = _x get "property";
    private _value = _x get "value";

    [_class, "init", {
        params ["_entity", "_isJip", "_property", "_value"];
        _entity setVariable [_property, _value];
    }, true, [_property, _value], true] call CBA_fnc_addClassEventHandler;
} forEach _overrides;

// Apply to existing entities (fallback — class event handlers only apply to entities created after registration)
{
    private _matchValue = _x get "match_value";
    private _targetType = _x get "target_type";
    private _property = _x get "property";
    private _operator = _x get "operator";
    private _value = _x get "value";

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
} forEach _overrides;
