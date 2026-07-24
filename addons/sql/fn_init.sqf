#include "script_component.hpp"

params [["_extension", "a3db"]];

private _version = _extension callExtension "version";
diag_log text format ["[A3DB] Loading extension: %1", _version];

_version
