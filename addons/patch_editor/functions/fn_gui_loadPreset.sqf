#include "../script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

// Get all available presets
private _presets = ["SELECT name, id FROM patch_presets ORDER BY id ASC"] call a3sql_fnc_selectMap;

if (_presets isEqualTo []) exitWith {
    ["A3SQL Patch", "No saved presets found"] call CBA_fnc_notify;
};

// Use the name field as preset name input, or show list if empty
private _presetName = ctrlText (_display displayCtrl 201);

if (_presetName isEqualTo "") then {
    // Show available presets
    private _names = _presets apply { _x getOrDefault ["name", "?"] };
    private _msg = "Available presets:\n\n" + (_names joinString "\n") + "\n\nType a preset name in the Name field and click Load Preset again.";
    ["A3SQL Patch", "Type a preset name in the Name field, then click Load Preset"] call CBA_fnc_notify;
    hint _msg;
} else {
    // Find the preset
    private _match = _presets select { (_x getOrDefault ["name", ""]) == _presetName };
    if (_match isEqualTo []) exitWith {
        ["A3SQL Patch", format ["Preset '%1' not found", _presetName]] call CBA_fnc_notify;
    };

    private _row = [format ["SELECT data FROM patch_presets WHERE name = '%1'", _presetName]] call a3sql_fnc_selectMap;
    if (_row isEqualTo []) exitWith {
        ["A3SQL Patch", format ["Could not load preset '%1'", _presetName]] call CBA_fnc_notify;
    };

    private _data = (_row select 0) getOrDefault ["data", ""];
    if (_data isEqualTo "") exitWith {
        ["A3SQL Patch", format ["Preset '%1' is empty", _presetName]] call CBA_fnc_notify;
    };

    // Parse the preset data (stored as SQF array serialization)
    private _rules = call compile _data;
    if !(_rules isEqualType []) exitWith {
        ["A3SQL Patch", "Invalid preset data format"] call CBA_fnc_notify;
    };

    // Delete all existing rules
    ["DELETE FROM patch_rules"] call a3sql_fnc_execute;

    // Insert preset rules
    private _inserted = 0;
    {
        if (_x isEqualType []) then {
            _x = createHashMapFromArray _x;
        };
        private _name = _x getOrDefault ["name", ""];
        private _active = _x getOrDefault ["active", 1];
        private _priority = _x getOrDefault ["priority", 0];
        private _targetType = _x getOrDefault ["target_type", ""];
        private _property = _x getOrDefault ["property", ""];
        private _operator = _x getOrDefault ["operator", "set"];
        private _value = _x getOrDefault ["value", ""];

        private _insertSQL = format [
            "INSERT INTO patch_rules (name, active, priority, target_type, property, operator, value) VALUES ('%1', %2, %3, '%4', '%5', '%6', '%7')",
            _name, _active, _priority, _targetType, _property, _operator, _value
        ];
        private _res = [_insertSQL] call a3sql_fnc_execute;
        if ((_res select 0) == 0) then {
            _inserted = _inserted + 1;
        };
    } forEach _rules;

    // Refresh list and mark dirty
    call FUNC(gui_listRules);
    [true] call a3sql_patch_core_fnc_setDirty;

    ["A3SQL Patch", format ["Preset '%1' loaded (%2 rules)", _presetName, _inserted]] call CBA_fnc_notify;
};

true
