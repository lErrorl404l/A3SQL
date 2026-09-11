#include "../script_component.hpp"

params [["_extension", "a3sql", [""]]];

private _version = _extension callExtension "version";

INFO_1("%1",_version);

_version

