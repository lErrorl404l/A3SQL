#include "../script_component.hpp"

params ["_faction", "_role", "_name", "_uniform", "_vest", "_helmet", "_backpack", "_primaryWeapon", "_secondaryWeapon", "_handgunWeapon", "_itemsJson", "_magazinesJson"];

private _sql = format ["INSERT INTO loadout_templates (faction, role, name, uniform, vest, helmet, backpack, primary_weapon, secondary_weapon, handgun_weapon, items_json, magazines_json, created_at) VALUES ('%1', '%2', '%3', '%4', '%5', '%6', '%7', '%8', '%9', '%10', '%11', '%12', datetime('now')) RETURNING id",
    [_faction] call a3sql_database_fnc_sqlEscape, [_role] call a3sql_database_fnc_sqlEscape, [_name] call a3sql_database_fnc_sqlEscape, [_uniform] call a3sql_database_fnc_sqlEscape, [_vest] call a3sql_database_fnc_sqlEscape, [_helmet] call a3sql_database_fnc_sqlEscape, [_backpack] call a3sql_database_fnc_sqlEscape, [_primaryWeapon] call a3sql_database_fnc_sqlEscape, [_secondaryWeapon] call a3sql_database_fnc_sqlEscape, [_handgunWeapon] call a3sql_database_fnc_sqlEscape, [_itemsJson] call a3sql_database_fnc_sqlEscape, [_magazinesJson] call a3sql_database_fnc_sqlEscape];

private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
