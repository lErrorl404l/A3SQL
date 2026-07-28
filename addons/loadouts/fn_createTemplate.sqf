#include "script_component.hpp"

params ["_faction", "_role", "_name", "_uniform", "_vest", "_helmet", "_backpack", "_primaryWeapon", "_secondaryWeapon", "_handgunWeapon", "_itemsJson", "_magazinesJson"];

private _sql = format ["INSERT INTO loadout_templates (faction, role, name, uniform, vest, helmet, backpack, primary_weapon, secondary_weapon, handgun_weapon, items_json, magazines_json, created_at) VALUES ('%1', '%2', '%3', '%4', '%5', '%6', '%7', '%8', '%9', '%10', '%11', '%12', datetime('now')) RETURNING id",
    _faction, _role, _name, _uniform, _vest, _helmet, _backpack, _primaryWeapon, _secondaryWeapon, _handgunWeapon, _itemsJson, _magazinesJson];

private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
