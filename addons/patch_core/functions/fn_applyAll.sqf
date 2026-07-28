#include "..\script_component.hpp"

params [
    ["_extension", "a3sql", [""]]
];

private _log_level = missionNamespace getVariable ["a3sql_patch_log_level", 2];
private _applied = 0;
private _errors  = 0;
private _offset  = 0;
private _pageSize = 50;

while {true} do {
    private _sql = format [
        "SELECT * FROM patch_rules WHERE active = 1 ORDER BY priority DESC, id ASC LIMIT %1 OFFSET %2",
        _pageSize, _offset
    ];
    private _response = _extension callExtension _sql;
    if (_response isEqualTo "") exitWith {};

    private _parsed = parseSimpleArray _response;
    if ((_parsed select 0) != 0) exitWith {};

    private _data = _parsed select 2;
    if !(_data isEqualType []) exitWith {};
    if (count _data < 2) exitWith {};

    private _headers = _data select 0;
    private _rows = _data select [1];
    if (_rows isEqualTo []) exitWith {};

    {
        private _row = _x;
        private _rule = createHashMap;
        {
            _rule set [_x, _row select _forEachIndex];
        } forEach _headers;

        // Parse match_value if it looks like a JSON / SQF array
        private _mv = _rule getOrDefault ["match_value", ""];
        if (_mv isEqualType "" && {count _mv > 1 && {_mv select [0, 1] == "["} && {_mv select [count _mv - 1, 1] == "]"}}) then {
            _rule set ["match_value", parseSimpleArray _mv];
        };

        private _ruleId = _rule getOrDefault ["id", "?"];

        // ── Stream output (real-time) ───────────────────────────────
        if (missionNamespace getVariable ["a3sql_patch_stream_output", false]) then {
            systemChat format ["[A3SQL Patch] Applying rule %1 (%2 — %3 %4 %5)",
                _ruleId,
                _rule getOrDefault ["name", ""],
                _rule getOrDefault ["target_type", ""],
                _rule getOrDefault ["property", ""],
                _rule getOrDefault ["operator", "set"]
            ];
        };

        try {
            private _result = [_rule, _extension] call FUNC(applyRule);
            if ((_result select 0) == 0) then {
                _applied = _applied + 1;
            } else {
                _errors = _errors + 1;
                if (_log_level >= 1) then {
                    diag_log text format ["[A3SQL Patch] Rule %1 error: %2", _ruleId, _result select 2];
                };
            };
        } catch {
            _errors = _errors + 1;
            if (_log_level >= 1) then {
                diag_log text format ["[A3SQL Patch] Rule %1 exception: %2", _ruleId, _exception];
            };
        };
    } forEach _rows;

    _offset = _offset + _pageSize;
};

if (_log_level >= 2) then {
    diag_log text format ["[A3SQL Patch] applyAll: %1 applied, %2 errors", _applied, _errors];
};

[0, "OK", [_applied, _errors]]
