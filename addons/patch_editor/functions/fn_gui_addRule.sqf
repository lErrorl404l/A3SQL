#include "..\script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

private _name = ctrlText (_display displayCtrl 201);
private _active = cbChecked (_display displayCtrl 202);
private _priority = sliderPosition (_display displayCtrl 203);
private _targetType = lbText (lbCurSel (_display displayCtrl 204));
private _property = ctrlText (_display displayCtrl 205);
private _operator = lbText (lbCurSel (_display displayCtrl 206));
private _value = ctrlText (_display displayCtrl 207);

if (_name isEqualTo "") exitWith {
    systemChat "[A3SQL Patch] Rule name is required";
};

// ── Validation ─────────────────────────────────────────────────────
private _ruleHash = createHashMap;
_ruleHash set ["target_type", _targetType];
_ruleHash set ["property", _property];
_ruleHash set ["operator", _operator];
_ruleHash set ["value", _value];

private _validation = [_ruleHash] call FUNC(validateRule);
if (!(_validation select 0)) exitWith {
    systemChat format ["[A3SQL Patch] Validation failed: %1", _validation select 1];
};

private _activeInt = if (_active) then { 1 } else { 0 };
private _priorityInt = round _priority;

// Check if updating an existing rule or adding a new one
private _list = _display displayCtrl 100;
private _sel = lbCurSel _list;
private _isUpdate = _sel >= 0;

private _result = [];

if (_isUpdate) then {
    private _ruleId = parseNumber (_list lbData _sel);
    private _sql = format [
        "UPDATE patch_rules SET name = '%1', active = %2, priority = %3, target_type = '%4', property = '%5', operator = '%6', value = '%7' WHERE id = %8",
        _name, _activeInt, _priorityInt, _targetType, _property, _operator, _value, _ruleId
    ];
    _result = [_sql] call a3sql_fnc_execute;

    if ((_result select 0) == 0) then {
        systemChat format ["[A3SQL Patch] Rule %1 updated", _ruleId];
        call FUNC(gui_listRules);
        [true] call a3sql_patch_core_fnc_setDirty;
    } else {
        systemChat format ["[A3SQL Patch] Failed to update rule: %1", _result select 2];
    };
} else {
    private _sql = format [
        "INSERT INTO patch_rules (name, active, priority, target_type, property, operator, value) VALUES ('%1', %2, %3, '%4', '%5', '%6', '%7')",
        _name, _activeInt, _priorityInt, _targetType, _property, _operator, _value
    ];
    _result = [_sql] call a3sql_fnc_execute;

    if ((_result select 0) == 0) then {
        systemChat format ["[A3SQL Patch] Rule '%1' added", _name];
        call FUNC(gui_listRules);

        // Clear fields for next entry
        (_display displayCtrl 201) ctrlSetText "";
        (_display displayCtrl 207) ctrlSetText "";
        (_display displayCtrl 202) cbSetChecked true;
        (_display displayCtrl 203) sliderSetPosition 0;

        [true] call a3sql_patch_core_fnc_setDirty;
    } else {
        systemChat format ["[A3SQL Patch] Failed to add rule: %1", _result select 2];
    };
};
