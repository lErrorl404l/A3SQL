#include "script_component.hpp"

private _display = findDisplay 12300;
if (isNull _display) exitWith {};

// ── Populate combo boxes (once) ──────────────────────────────
private _targetCombo = _display displayCtrl 204;
if (lbSize _targetCombo == 0) then {
    {
        _targetCombo lbAdd _x;
    } forEach ["weapon", "vehicle", "magazine", "unit", "texture", "material", "entity"];
    _targetCombo lbSetCurSel 0;
};

private _opCombo = _display displayCtrl 206;
if (lbSize _opCombo == 0) then {
    {
        _opCombo lbAdd _x;
    } forEach ["set", "inc", "sub", "mul", "div", "mod", "cat", "default", "round", "clamp", "negate", "replace", "format", "sqf_exec"];
    _opCombo lbSetCurSel 0;
};

// ── Initialize slider range ─────────────────────────────────
private _slider = _display displayCtrl 203;
_slider sliderSetRange [0, 100];
_slider sliderSetSpeed [1, 10];
_slider sliderSetPosition 0;

// ── Populate rule list ───────────────────────────────────────
private _list = _display displayCtrl 100;
lbClear _list;

private _rules = ["SELECT * FROM patch_rules ORDER BY priority DESC, id ASC"] call a3sql_fnc_selectMap;

{
    private _id = _x getOrDefault ["id", 0];
    private _name = _x getOrDefault ["name", ""];
    private _targetType = _x getOrDefault ["target_type", ""];
    private _property = _x getOrDefault ["property", ""];
    private _operator = _x getOrDefault ["operator", "set"];
    private _value = _x getOrDefault ["value", ""];

    private _label = format ["[%1] %2 | %3 | %4 %5 %6",
        _id, _name, _targetType, _property, _operator, _value
    ];
    private _idx = _list lbAdd _label;
    _list lbSetData [_idx, str _id];
    _list lbSetTooltip [_idx, _label];
} forEach _rules;

true
