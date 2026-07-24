#include "script_component.hpp"

params [["_extension", "a3db"]];

private _version = _extension callExtension "version";
diag_log text format ["[A3DB] Loading extension: %1", _version];

// Start TCP listener if enabled in CBA settings
if (isServer && {CBA_settings_loaded}) then {
    if (missionNamespace getVariable ["a3db_listener_enabled", false]) then {
        private _port = missionNamespace getVariable ["a3db_listener_port", 33306];
        private _result = _extension callExtension ["listen", [str _port]];
        diag_log text format ["[A3DB] Listener: %1", _result];
    };
};

_version
