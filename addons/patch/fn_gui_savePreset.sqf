#include "script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

// Use the Name field as preset name
private _presetName = ctrlText (_display displayCtrl 201);
if (_presetName isEqualTo "") then {
    _presetName = ["A3SQL Patch", "Preset name:", ""] call CBA_fnc_inputBox;
    if (_presetName isEqualTo "") exitWith {
        systemChat "[A3SQL Patch] Save cancelled — no preset name provided";
    };
};

// Fetch all rules as array of hashmaps
private _rules = ["SELECT * FROM patch_rules ORDER BY priority DESC, id ASC"] call a3sql_fnc_selectMap;

if (_rules isEqualTo []) exitWith {
    systemChat "[A3SQL Patch] No rules to save";
};

// Strip the id / created_at fields — presets store definition only
// Convert to array-of-pairs for portable SQF serialization (str on hashmaps is unreliable)
private _stripped = _rules apply {
    private _rule = +_x;
    _rule deleteAt "id";
    _rule deleteAt "created_at";
    private _pairs = [];
    {
        _pairs pushBack [_x, _rule get _x];
    } forEach keys _rule;
    _pairs
};

private _data = str _stripped;

// Check if preset already exists
private _existing = [format ["SELECT id FROM patch_presets WHERE name = '%1'", _presetName]] call a3sql_fnc_selectMap;
if (_existing isNotEqualTo []) then {
    // Update existing preset
    private _sql = format [
        "UPDATE patch_presets SET data = '%1' WHERE name = '%2'",
        _data, _presetName
    ];
    [_sql] call a3sql_fnc_execute;
    systemChat format ["[A3SQL Patch] Preset '%1' updated (%2 rules)", _presetName, count _stripped];
} else {
    // Insert new preset
    private _sql = format [
        "INSERT INTO patch_presets (name, data) VALUES ('%1', '%2')",
        _presetName, _data
    ];
    [_sql] call a3sql_fnc_execute;
    systemChat format ["[A3SQL Patch] Preset '%1' saved (%2 rules)", _presetName, count _stripped];
};

true
