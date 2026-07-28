#include "..\script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3sql"]
];

if (_sql isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty SQL"] };

// Run query, auto-paginate if result is large
private _response = _extension callExtension _sql;
private _parsed = _response call CBA_fnc_parseJSON;

if ((_parsed select 0) == 0) then {
    _parsed
} else {
    // Large result — use cursor
    private _cursorName = "q_" + (call BIS_fnc_randomNum);
    _extension callExtension ["cursor create", [_cursorName, _sql]];

    private _allRows = [];
    private _cursorResult = [];

    while { true } do {
        _cursorResult = (_extension callExtension ["cursor fetch", [_cursorName, "200"]]) call CBA_fnc_parseJSON;
        if ((_cursorResult select 0) != 0) exitWith {};
        private _data = _cursorResult select 2;
        if (_data isEqualType []) then {
            if (count _data <= 1) exitWith {};  // header only = no more rows
            _allRows append (_data select [1]);
        };
    };

    _extension callExtension ["cursor drop", [_cursorName]];

    // Return as [0, "OK", [[headers], ...allRows]]
    private _header = ((_parsed select 2) select [0, 1]);
    [0, "OK", _header + _allRows]
};
