#include "../script_component.hpp"

params [
    ["_control", controlNull, [controlNull]],
    ["_selectedIndex", -1, [0]]
];

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

private _list = _display displayCtrl 100;

// Use provided index (from onLBSelChanged) or find current selection
private _sel = _selectedIndex;
if (_sel < 0) then {
    _sel = lbCurSel _list;
};
if (_sel < 0) exitWith {};

private _ruleId = parseNumber (_list lbData _sel);
if (_ruleId <= 0) exitWith {};

private _sql = format ["SELECT * FROM patch_rules WHERE id = %1", _ruleId];
private _rows = [_sql] call a3sql_database_fnc_selectMap;
if (_rows isEqualTo []) exitWith {};

private _rule = _rows select 0;

private _name = _rule getOrDefault ["name", ""];
private _active = (_rule getOrDefault ["active", 0]) == 1;
private _priority = _rule getOrDefault ["priority", 0];
private _targetType = _rule getOrDefault ["target_type", "weapon"];
private _property = _rule getOrDefault ["property", ""];
private _operator = _rule getOrDefault ["operator", "set"];
private _value = _rule getOrDefault ["value", ""];

(_display displayCtrl 201) ctrlSetText _name;
(_display displayCtrl 202) cbSetChecked _active;
(_display displayCtrl 203) sliderSetPosition _priority;

private _targetCombo = _display displayCtrl 204;
private _targetIdx = 0;
for "_i" from 0 to (lbSize _targetCombo - 1) do {
    if ((_targetCombo lbText _i) == _targetType) exitWith { _targetIdx = _i; };
};
_targetCombo lbSetCurSel _targetIdx;

(_display displayCtrl 205) ctrlSetText _property;

private _opCombo = _display displayCtrl 206;
private _opIdx = 0;
for "_i" from 0 to (lbSize _opCombo - 1) do {
    if ((_opCombo lbText _i) == _operator) exitWith { _opIdx = _i; };
};
_opCombo lbSetCurSel _opIdx;

(_display displayCtrl 207) ctrlSetText _value;
