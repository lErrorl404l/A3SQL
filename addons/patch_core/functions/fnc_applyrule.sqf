#include "../script_component.hpp"

params [
    ["_rule", [], [createHashMap, []]],
    ["_extension", "a3sql", [""]]
];

// ── Extract rule fields (supports hashmap or array from getRule) ──
private _matchType  = "";
private _matchValue = "";
private _targetType = "";
private _property   = "";
private _operator   = "set";
private _value      = "";

if (_rule isEqualType []) then {
    // Raw DB row array — use ordinal indexes matching patch_rules column order
    _matchType  = _rule param [5, "exact", [""]];
    _matchValue = _rule param [6, "", ["", []]];
    _targetType = _rule param [7, "", [""]];
    _property   = _rule param [8, "", [""]];
    _operator   = _rule param [9, "set", [""]];
    _value      = _rule param [10, "", [""]];
} else {
    if (_rule isEqualType createHashMap) then {
        _matchType  = _rule getOrDefault ["match_type", "exact"];
        _matchValue = _rule getOrDefault ["match_value", ""];
        _targetType = _rule getOrDefault ["target_type", ""];
        _property   = _rule getOrDefault ["property", ""];
        _operator   = _rule getOrDefault ["operator", "set"];
        _value      = _rule getOrDefault ["value", ""];
    };
};

if (_property isEqualTo "") exitWith { [1, "ERR_PARAM", "No property specified"] };

// ── Gather candidate targets ──
private _targets = [];
private _handlerApplied = false;

switch (toLower _targetType) do {
    case "weapon": {
        try {
            _targets = [_matchValue, _property, _value] call FUNC(handleWeapon);
            _handlerApplied = true;
        } catch {};
    };
    case "magazine": {
        try {
            _targets = [_matchValue, _property, _value] call FUNC(handleMagazine);
            _handlerApplied = true;
        } catch {};
    };
    case "texture": {
        try {
            _targets = [_matchValue, _property, _value] call FUNC(handleTexture);
            _handlerApplied = true;
        } catch {};
    };
    case "material": {
        try {
            _targets = [_matchValue, _property, _value] call FUNC(handleMaterial);
            _handlerApplied = true;
        } catch {};
    };
    case "entity": {
        try {
            _targets = [_matchValue, _property, _value] call FUNC(handleEntity);
            _handlerApplied = true;
        } catch {};
    };
    default {
        private _customHandler = missionNamespace getVariable [format [QGVAR(handler_%1), toLower _targetType], nil];
        if (isNil {_customHandler}) then {
            // ── Generic target collection ──
            switch (toLower _targetType) do {
                case "all":     { _targets = allMissionObjects "All"; };
                case "object":  { _targets = allMissionObjects "All"; };
                case "vehicle": { _targets = vehicles; };
                case "man":       { _targets = allUnits; };
                case "unit":      { _targets = allUnits; };
                case "group":   { _targets = allGroups apply { _x }; };
                default         { _targets = allMissionObjects "All"; };
            };
        } else {
            try {
                _targets = [_matchValue, _property, _value] call _customHandler;
                _handlerApplied = true;
            } catch {};
        };
    };
};

// ── Handler-routed types skip generic operator logic ──
if (_handlerApplied) exitWith { [0, "OK", count _targets] };

if (_targets isEqualTo []) exitWith { [0, "OK", "No targets found for type"] };

// ── Filter by match_type / match_value ──
private _matched = [];

switch (toLower _matchType) do {
    case "all": {
        _matched = _targets;
    };
    case "exact": {
        if (_matchValue isEqualType []) then {
            _matched = _targets select { typeOf _x in _matchValue };
        } else {
            _matched = _targets select { typeOf _x == _matchValue };
        };
    };
    case "type_of": {
        if (_matchValue isEqualType []) then {
            _matched = _targets select {
                private _t = _x;
                private _hit = false;
                { if (_t isKindOf _x) exitWith { _hit = true }; } forEach _matchValue;  // ponytail: O(n×m) fine for typical <100 rules
                _hit
            };
        } else {
            _matched = _targets select { _x isKindOf _matchValue };
        };
    };
    case "wildcard": {
        private _pattern = if (_matchValue isEqualType []) then { _matchValue select 0 } else { _matchValue };
        _matched = _targets select { [typeOf _x, _pattern] call CBA_fnc_matchesWildcard };
    };
    case "regex": {
        private _re = if (_matchValue isEqualType []) then { _matchValue select 0 } else { _matchValue };
        _matched = _targets select { [typeOf _x, _re] call CBA_fnc_matchesRegex };
    };
    default {
        _matched = _targets;
    };
};

if (_matched isEqualTo []) exitWith { [0, "OK", "No targets matched"] };

// ── Apply operator ──
private _applied = 0;
private _failed  = 0;

{
    private _target = _x;
    private _success = false;

    switch (toLower _operator) do {
        case "set": {
            try {
                _target setVariable [_property, _value];
                _success = true;
            } catch {};
        };
        case "toggle": {
            try {
                private _current = _target getVariable [_property, false];
                _target setVariable [_property, !_current];
                _success = true;
            } catch {};
        };
        case "call": {
            try {
                private _fnc = missionNamespace getVariable [_property, {}];
                [_target, _value] call _fnc;
                _success = true;
            } catch {};
        };
        case "sqf_exec": {
            if (["a3sql_patch_allow_sqf_exec"] call CBA_fnc_getSetting) then {
                try {
                    private _code = compile _value;
                    [_target] call _code;
                    _success = true;
                } catch {};
            } else {
                if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
                    ["A3SQL Patch", "sqf_exec blocked — a3sql_patch_allow_sqf_exec is disabled"] call CBA_fnc_error;
                };
            };
        };
        case "add": {
            if (["a3sql_patch_allow_sqf_exec"] call CBA_fnc_getSetting) then {
                try {
                    private _code = compile _value;
                    private _handlerId = _target addEventHandler [_property, _code];
                    // Track handler ID so "remove" can find it
                    private _tracked = _target getVariable [QGVAR(ehRegistry), createHashMap];
                    _tracked set [_property, _handlerId];
                    _target setVariable [QGVAR(ehRegistry), _tracked];
                    _success = true;
                } catch {};
            } else {
                if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
                    ["A3SQL Patch", "add blocked — a3sql_patch_allow_sqf_exec is disabled"] call CBA_fnc_error;
                };
            };
        };
        case "remove": {
            try {
                private _tracked = _target getVariable [QGVAR(ehRegistry), createHashMap];
                private _handlerId = _tracked getOrDefault [_property, -1];
                if (_handlerId >= 0) then {
                    _target removeEventHandler [_property, _handlerId];
                    _tracked deleteAt _property;
                    _success = true;
                };
            } catch {};
        };
        // ── Value-transformer operators ──
        case "inc": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opAdd)];
                _success = true;
            } catch {};
        };
        case "sub": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opSub)];
                _success = true;
            } catch {};
        };
        case "mul": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opMul)];
                _success = true;
            } catch {};
        };
        case "div": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opDiv)];
                _success = true;
            } catch {};
        };
        case "mod": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opMod)];
                _success = true;
            } catch {};
        };
        case "cat": {
            try {
                private _current = _target getVariable [_property, ""];
                _target setVariable [_property, [_current, _value] call FUNC(opCat)];
                _success = true;
            } catch {};
        };
        case "default": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value] call FUNC(opDefault)];
                _success = true;
            } catch {};
        };
        // ── Value-transformer operators (5 new) ──
        case "round": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value, _target, _property] call FUNC(opRound)];
                _success = true;
            } catch {};
        };
        case "clamp": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value, _target, _property] call FUNC(opClamp)];
                _success = true;
            } catch {};
        };
        case "negate": {
            try {
                private _current = _target getVariable [_property, 0];
                _target setVariable [_property, [_current, _value, _target, _property] call FUNC(opNegate)];
                _success = true;
            } catch {};
        };
        case "replace": {
            try {
                private _current = _target getVariable [_property, ""];
                _target setVariable [_property, [_current, _value, _target, _property] call FUNC(opReplace)];
                _success = true;
            } catch {};
        };
        case "format": {
            try {
                private _current = _target getVariable [_property, ""];
                _target setVariable [_property, [_current, _value, _target, _property] call FUNC(opFormat)];
                _success = true;
            } catch {};
        };
    };

    if (_success) then {
        _applied = _applied + 1;
    } else {
        _failed = _failed + 1;
    };
} forEach _matched;

[0, "OK", [_applied, _failed]]
