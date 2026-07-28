#include "../script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

private _list = _display displayCtrl 100;
private _sel = lbCurSel _list;
if (_sel < 0) exitWith {
    ["A3SQL Patch", "No rule selected"] call CBA_fnc_notify;
};

private _ruleId = parseNumber (_list lbData _sel);
if (_ruleId <= 0) exitWith {};

private _result = [_ruleId] call a3sql_patch_core_fnc_deleteRule;

if ((_result select 0) == 0) then {
    ["A3SQL Patch", format ["Rule %1 deleted", _ruleId]] call CBA_fnc_notify;
    call FUNC(gui_listRules);

    // Clear fields
    (_display displayCtrl 201) ctrlSetText "";
    (_display displayCtrl 205) ctrlSetText "";
    (_display displayCtrl 207) ctrlSetText "";
    (_display displayCtrl 202) cbSetChecked true;
    (_display displayCtrl 203) sliderSetPosition 0;
} else {
    ["A3SQL Patch", format ["Delete failed: %1", _result select 2]] call CBA_fnc_notify;
};
