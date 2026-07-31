#include "../script_component.hpp"

params [
    ["_filter", "", ["", createHashMap, []]],
    ["_extension", "a3sql", [""]]
];

private _sql = "SELECT * FROM patch_rules ORDER BY priority DESC, id ASC";

if (_filter isEqualType "") then {
    if (_filter != "") then {
        _sql = format [
            "SELECT * FROM patch_rules WHERE name LIKE '%%%1%%' ORDER BY priority DESC, id ASC",
            [_filter] call a3sql_database_fnc_sqlEscape
        ];
    };
};

if (_filter isEqualType createHashMap) then {
    private _conditions = [];
    {
        private _key = _x;
        private _val = _filter get _key;
        if (_val isEqualType "") then {
            _conditions pushBack format ["%1 = '%2'", _key, [_val] call a3sql_database_fnc_sqlEscape];
        } else {
            _conditions pushBack format ["%1 = %2", _key, _val];
        };
    } forEach (keys _filter);
    if (_conditions isNotEqualTo []) then {
        _sql = format [
            "SELECT * FROM patch_rules WHERE %1 ORDER BY priority DESC, id ASC",
            _conditions joinString " AND "
        ];
    };
};

private _response = _extension callExtension _sql;
if (_response isEqualTo "") exitWith { [1, "ERR_CONN", "No response from extension"] };

parseSimpleArray _response
