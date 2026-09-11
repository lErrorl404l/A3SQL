#include "../script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";
private _log_level = ["a3sql_patch_log_level"] call CBA_fnc_getSetting;

if (_log_level >= 3) then {
    INFO_1("%1",_version);
};

_version

