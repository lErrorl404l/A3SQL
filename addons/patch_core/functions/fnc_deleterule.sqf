#include "../script_component.hpp"

params [
    ["_ruleId", 0, [0]],
    ["_extension", "a3sql", [""]]
];

if (_ruleId <= 0) exitWith { [1, "ERR_PARAM", "Invalid rule ID"] };

private _sql = format ["DELETE FROM patch_rules WHERE id = %1", _ruleId];
private _response = _extension callExtension _sql;
private _parsed = parseSimpleArray _response;

if ((_parsed select 0) == 0) then {
    // Mark dirty so applyAll picks up the change
    if (isNil QGVAR(namespace)) then { GVAR(namespace) = [] call CBA_fnc_createNamespace; };
    GVAR(namespace) setVariable ["dirty", true];
    if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
        INFO_1("Rule %1 deleted",_ruleId);
    };
};

_parsed

