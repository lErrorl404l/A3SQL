#include "script_component.hpp"

params [
    ["_table", "", [""]],
    ["_data", "", ["", []]],
    ["_extension", "a3db"]
];

if (_table isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty table name"] };
if (_data isEqualTo "") exitWith { [0, "ERR_EXEC", "No data provided"] };

private _json = if (_data isEqualType []) then {
    _data
} else {
    private _raw = preprocessFile _data;
    if (_raw isEqualTo "") then {
        _raw = loadFile _data;
    };
    if (_raw isEqualTo "") exitWith { [0, "ERR_EXEC", "File not found"] };
    _raw
};

private _result = _extension callExtension ["import_json", [_table, _json]];
_result call CBA_fnc_parseJSON
