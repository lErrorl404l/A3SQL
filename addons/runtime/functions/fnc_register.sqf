#include "../script_component.hpp"

params [
    ["_extension", "a3sql", [""]]
];

// ── Create the runtime_overrides table if it does not exist ────────
private _createTable = "CREATE TABLE IF NOT EXISTS runtime_overrides (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER DEFAULT 1, event TEXT NOT NULL, match_type TEXT DEFAULT 'exact', match_value TEXT DEFAULT '', target_property TEXT DEFAULT '', operator TEXT DEFAULT 'set', value TEXT NOT NULL, priority INTEGER DEFAULT 0, apply_function TEXT DEFAULT '')";
private _result = _extension callExtension _createTable;

// Add apply_function column if upgrading from older schema
private _addColumn = "ALTER TABLE runtime_overrides ADD COLUMN apply_function TEXT DEFAULT ''";
_extension callExtension _addColumn;

private _logLevel = missionNamespace getVariable ["a3sql_runtime_log_level", 1];
if (_logLevel >= 2) then {
    ["A3SQL Runtime", "Table init: %1", _result] call CBA_fnc_info;
};

// ── Seed the demo rules only when the table is empty ────────────────
private _count = ["SELECT COUNT(*) FROM runtime_overrides", _extension] call a3sql_database_fnc_selectArray;
private _rowCount = if (_count isEqualTo []) then { 0 } else { (_count select 0) select 0 };

if (_rowCount == 0) then {
    // on_hit damage multiplier. Requires a player-present server (HitPart
    // is camera-scoped on dedicated servers, so not verifiable headless).
    private _seedHit = "INSERT INTO runtime_overrides (name, event, match_type, match_value, target_property, operator, value, priority, apply_function) VALUES ('demo_50cal_heavy_damage', 'on_hit', 'type_of', 'MSS_50_M33_Ball', '', 'mul', '1.5', 10, 'a3sql_runtime_fnc_applyDamage')";
    private _seedHitResult = _extension callExtension _seedHit;
    if (_logLevel >= 2) then {
        ["A3SQL Runtime", "Seeded demo on_hit rule: %1", _seedHitResult] call CBA_fnc_info;
    };

    // on_fire projectile-velocity multiplier. The runtime ballistics hook.
    // Mission-wide Fired fires on a dedicated server, so this is verifiable
    // headless: firing the M107A1 gives the round 1.2x velocity.
    private _seedFire = "INSERT INTO runtime_overrides (name, event, match_type, match_value, target_property, operator, value, priority, apply_function) VALUES ('demo_50cal_hot_load', 'on_fire', 'type_of', 'MSS_50_M33_Ball', '', 'mul', '1.2', 20, 'a3sql_runtime_fnc_applyVelocity')";
    private _seedFireResult = _extension callExtension _seedFire;
    if (_logLevel >= 2) then {
        ["A3SQL Runtime", "Seeded demo on_fire rule: %1", _seedFireResult] call CBA_fnc_info;
    };
};

[0, "OK", "Table ready"]
