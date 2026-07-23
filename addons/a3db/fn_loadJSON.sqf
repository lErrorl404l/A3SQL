#include "script_component.hpp"

params [
    ["_table", "", [""]],
    ["_data", "", ["", []]],
    ["_extension", "a3db"]
];

if (_table isEqualTo "") exitWith { ["ERROR", "Empty table name"] };
if (_data isEqualTo "") exitWith { ["ERROR", "No data provided"] };

private _json = if (_data isEqualType []) then {
    _data
} else {
    private _raw = loadFile _data;
    if (_raw isEqualTo "") exitWith { ["ERROR", "File not found"] };
    _raw
};

private _result = _extension callExtension ["import_json", [_table, _json]];
_result
