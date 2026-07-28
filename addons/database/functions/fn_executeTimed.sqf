#include "..\script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3sql"]
];

if (_sql isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty SQL"] };

private _start = diag_tickTime;
private _result = [_sql, _extension] call FUNC(execute);
private _elapsed = diag_tickTime - _start;

if (_elapsed > 0.01) then {
    diag_log text format ["[A3SQL] SLOW QUERY (%1 ms): %2", round (_elapsed * 1000), _sql];
};

_result
