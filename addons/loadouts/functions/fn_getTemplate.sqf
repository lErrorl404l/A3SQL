#include "..\script_component.hpp"

params ["_id", "_faction", "_role"];

private _sql = "";
if (_id > 0) then {
    _sql = format ["SELECT * FROM loadout_templates WHERE id = %1", _id];
} else {
    _sql = format ["SELECT * FROM loadout_templates WHERE faction = '%1' AND role = '%2'", _faction, _role];
};

private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
