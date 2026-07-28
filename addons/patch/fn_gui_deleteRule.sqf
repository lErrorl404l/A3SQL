#include "script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

private _list = _display displayCtrl 100;
private _sel = lbCurSel _list;
if (_sel < 0) exitWith {
    systemChat "[A3SQL Patch] No rule selected";
};

private _ruleId = parseNumber (_list lbData _sel);
if (_ruleId <= 0) exitWith {};

private _result = [_ruleId] call a3sql_patch_fnc_deleteRule;

if ((_result select 0) == 0) then {
    systemChat format ["[A3SQL Patch] Rule %1 deleted", _ruleId];
    call a3sql_patch_fnc_gui_listRules;

    // Clear fields
    (_display displayCtrl 201) ctrlSetText "";
    (_display displayCtrl 205) ctrlSetText "";
    (_display displayCtrl 207) ctrlSetText "";
    (_display displayCtrl 202) cbSetChecked true;
    (_display displayCtrl 203) sliderSetPosition 0;
} else {
    systemChat format ["[A3SQL Patch] Delete failed: %1", _result select 2];
};
