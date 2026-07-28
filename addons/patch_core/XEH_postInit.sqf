#include "script_component.hpp"

private _extension = "a3sql";
private _enabled = ["a3sql_patch_enabled"] call CBA_fnc_getSetting;
private _log_level = ["a3sql_patch_log_level"] call CBA_fnc_getSetting;

// ── Auto-create patch_rules table ──────────────────────────────────
private _createTable = "CREATE TABLE IF NOT EXISTS patch_rules (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER DEFAULT 1, priority INTEGER DEFAULT 0, match_type TEXT NOT NULL DEFAULT 'exact', match_value TEXT DEFAULT '', target_type TEXT NOT NULL, property TEXT NOT NULL, operator TEXT DEFAULT 'set', value TEXT NOT NULL, created_at TEXT DEFAULT '')";
private _result = _extension callExtension _createTable;
if (_log_level >= 2) then {
    ["A3SQL Patch", "Table init: %1", _result] call CBA_fnc_info;
};

// ── Auto-create patch_presets table ───────────────────────────────
private _createPresets = "CREATE TABLE IF NOT EXISTS patch_presets (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL, data TEXT NOT NULL, created_at TEXT DEFAULT '')";
private _presetResult = _extension callExtension _createPresets;
if (_log_level >= 2) then {
    ["A3SQL Patch", "Presets table init: %1", _presetResult] call CBA_fnc_info;
};

// ── Column migration (group_name + notes) ─────────────────────────
"ALTER TABLE patch_rules ADD COLUMN group_name TEXT DEFAULT ''" call a3sql_database_fnc_execute;
"ALTER TABLE patch_rules ADD COLUMN notes TEXT DEFAULT ''" call a3sql_database_fnc_execute;
if (_log_level >= 2) then {
    ["A3SQL Patch", "Column migration applied (group_name, notes)"] call CBA_fnc_info;
};

// ── Auto-load saved rules ──────────────────────────────────────────
private _loadResult = _extension callExtension "load patch_rules";
if ('[0,"OK"' in _loadResult) then {
    if (_log_level >= 2) then {
        ["A3SQL Patch", "Loaded saved patch rules"] call CBA_fnc_info;
    };
    [] call a3sql_patch_core_fnc_reload;
};

// ── PerFrame handler ───────────────────────────────────────────────
if (_enabled) then {
    private _interval = ["a3sql_patch_check_interval_hz"] call CBA_fnc_getSetting;
    if (_interval <= 0) then { _interval = 0; };
    private _hz = if (_interval > 0) then { 1 / _interval } else { 0 };
    private _maxTicks = round(60 / ([0.05, _hz] select (_hz > 0)));

    [_hz, [0, _maxTicks], {
        params ["_args"];
        _args params ["_tick", "_maxTicks"];
        if !(["a3sql_patch_enabled"] call CBA_fnc_getSetting) exitWith {};
        private _dirty = if (isNil QGVAR(namespace)) then { true } else { GVAR(namespace) getVariable ["dirty", true] };
        private _timeout = (_tick >= _maxTicks);
        if (_dirty || _timeout) then {
            [] call a3sql_patch_core_fnc_applyAll;
            _args set [0, 0];
        } else {
            _args set [0, _tick + 1];
        };
    }] call CBA_fnc_addPerFrameHandler;
};

// ── JIP handler ────────────────────────────────────────────────────
addMissionEventHandler ["PlayerConnected", {
    params ["_id", "_uid", "_name", "_jip", "_owner", "_idStr"];
    if (_jip) then {
        if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 2) then {
            ["A3SQL Patch", "JIP player %1 (%2) — applying patches", _name, _uid] call CBA_fnc_info;
        };
        [] call a3sql_patch_core_fnc_applyAll;
    };
}];

// ── Apply config overrides (if patch_operators is loaded) ──────────
if (!isNil "a3sql_patch_operators_fnc_applyOverrides") then {
    [] call a3sql_patch_operators_fnc_applyOverrides;
};

// ── Mission end cleanup ────────────────────────────────────────────
addMissionEventHandler ["Ended", {
    private _log_level = ["a3sql_patch_log_level"] call CBA_fnc_getSetting;
    if (_log_level >= 2) then {
        ["A3SQL Patch", "Saving patch rules..."] call CBA_fnc_info;
    };
    "a3sql" callExtension "save patch_rules";
    if (_log_level >= 2) then {
        ["A3SQL Patch", "Saved OK — patch system cleanup complete"] call CBA_fnc_info;
    };
}];
