#include "script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";

diag_log text format ["[A3SQL Loadouts] %1", _version];

_version
