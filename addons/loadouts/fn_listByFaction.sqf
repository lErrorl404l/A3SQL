#include "script_component.hpp"

params ["_faction"];

private _sql = format ["SELECT * FROM loadout_templates WHERE faction = '%1' ORDER BY role ASC", _faction];
private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
