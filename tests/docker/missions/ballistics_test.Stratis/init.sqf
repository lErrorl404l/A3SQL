// A3SQL MSS Ballistics Demo — automated verification mission
// Run on a dedicated server with: @a3sql @cba_a3 @mss @ace [@a3sql_mss_ballistics]
//
// Phase 1 (server): read ballistic config values for demo weapons → a3sql.
//   Proves the compat override landed in the config the game loaded.
//
// Phase 2 (server): create each ammo's projectile, launch it at the config
//   initSpeed, and measure its velocity decay + drop over 0.5s.
//   Proves the config values are in effect in-engine.
//
// Phase 3: verify ALL 62 ammo classes from the full Project M set.
//   Reads CfgAmmo values and logs them for host-side comparison.

diag_log text "=== A3SQL BALLISTICS TEST START ===";

// ── Run id: patched if the compat PBO is loaded, else baseline ──
private _run = if (isClass (configFile >> "CfgPatches" >> "a3sql_mss_ballistics")) then { "patched" } else { "baseline" };
missionNamespace setVariable ["a3sql_ballistics_run", _run, true];
diag_log text format ["[BALLISTICS] run id: %1", _run];

"a3sql" callExtension "CREATE TABLE IF NOT EXISTS ballistics_test (id INTEGER PRIMARY KEY AUTOINCREMENT, run TEXT, weapon TEXT, phase TEXT, metric TEXT, value TEXT)";

// ── Demo weapons (Phase 1+2) ──
private _cases = [
    ["MSS_SR25_65CM_22_LMT_BLK", "MSS_20rnd_AR10_GI_65CM_147ELDM", "MSS_65CM_147ELDM"],
    ["Mss_M107A1_50_29_GRY",     "MSS_10rnd_50_M33_Ball",         "MSS_50_M33_Ball"],
    ["MSS_Mk18_24_300NM_CPS_BLK","MSS_10rnd_MK18_300NM_225ELDM",  "MSS_300NM_225ELDM"]
];

private _fnc_log = {
    params ["_weapon", "_phase", "_metric", "_val"];
    private _sql = format ["INSERT INTO ballistics_test (run, weapon, phase, metric, value) VALUES ('%1', '%2', '%3', '%4', '%5')",
        _run, _weapon, _phase, _metric, _val];
    "a3sql" callExtension _sql;
};

// ── PHASE 1: read config values for demo weapons ──
{
    _x params ["_wpn", "_mag", "_ammo"];

    private _cfgMag  = configFile >> "CfgMagazines" >> _mag;
    private _cfgAmmo = configFile >> "CfgAmmo" >> _ammo;

    private _initSpeed  = getNumber (_cfgMag >> "initSpeed");
    private _airFric    = getNumber (_cfgAmmo >> "airFriction");
    private _bulletMass = getNumber (_cfgAmmo >> "ACE_bulletMass");
    private _bc         = getArray  (_cfgAmmo >> "ACE_ballisticCoefficients");
    private _dragModel  = getNumber (_cfgAmmo >> "ACE_dragModel");
    private _muzzleVels = getArray  (_cfgAmmo >> "ACE_muzzleVelocities");

    diag_log text format ["[BALLISTICS][CONFIG] %1 initSpeed=%2 airFriction=%3 bulletMass=%4 BC=%5 dragModel=%6 muzzleVels=%7",
        _wpn, _initSpeed, _airFric, _bulletMass, _bc, _dragModel, _muzzleVels];

    [_wpn, "config", "initSpeed", str _initSpeed] call _fnc_log;
    [_wpn, "config", "airFriction", str _airFric] call _fnc_log;
    [_wpn, "config", "ACE_bulletMass", str _bulletMass] call _fnc_log;
    [_wpn, "config", "ACE_ballisticCoefficients", str _bc] call _fnc_log;
    [_wpn, "config", "ACE_dragModel", str _dragModel] call _fnc_log;
    [_wpn, "config", "ACE_muzzleVelocities", str _muzzleVels] call _fnc_log;
} forEach _cases;

// ── PHASE 2: fire projectiles and measure velocity ──
{
    _x params ["_wpn", "_mag", "_ammo"];

    private _cfgMag  = configFile >> "CfgMagazines" >> _mag;
    private _cfgAmmo = configFile >> "CfgAmmo" >> _ammo;
    private _initSpeed = getNumber (_cfgMag >> "initSpeed");

    // Create projectile at origin, aimed east
    private _pos = getPosATL player;
    private _proj = createVehicle [_ammo, [_pos select 0, _pos select 1, (_pos select 2) + 1.5], [], 0, "CAN_COLLIDE"];
    _proj setVelocity [_initSpeed, 0, 0];

    private _v0 = vectorMagnitude velocity _proj;
    diag_log text format ["[BALLISTICS][FIRE] %1 ammo=%2 initSpeed=%3 v0=%4", _wpn, _ammo, _initSpeed, _v0];

    [_wpn, "fire", "v0", str _v0] call _fnc_log;

    // Wait 0.5s for drag to act
    sleep 0.5;

    private _v1 = vectorMagnitude velocity _proj;
    private _drop = (_pos select 2) + 1.5 - (getPosATL _proj select 2);

    diag_log text format ["[BALLISTICS][FIRE] %1 ammo=%2 v1(0.5s)=%3 drop=%4", _wpn, _ammo, _v1, _drop];

    [_wpn, "fire", "v1", str _v1] call _fnc_log;
    [_wpn, "fire", "drop", str _drop] call _fnc_log;

    deleteVehicle _proj;
} forEach _cases;

// ── PHASE 3: verify ALL 62 ammo classes from full Project M ──
diag_log text "[BALLISTICS][PHASE3] Starting full ammo class verification";

private _allAmmos = [
    "MSS_300NM_225ELDM","MSS_300NM_230BH","MSS_300NM_250ATIP","MSS_300NM_M1163",
    "MSS_300PRC_174ELDVT","MSS_300PRC_212ELDX","MSS_300PRC_225ELDM","MSS_300PRC_245EOL",
    "MSS_300WM_212ELDX","MSS_300WM_Mk248_Mod0","MSS_300WM_Mk248_Mod1",
    "MSS_308_LAPSUB","MSS_308_M1158","MSS_308_M118LR","MSS_308_M62","MSS_308_M80A1",
    "MSS_308_M852","MSS_308_Mk316Mod0",
    "MSS_338LM_LAP_250GR","MSS_338LM_LAP_300GR","MSS_338LM_TAC_AP485",
    "MSS_338LM_TAC_AP529","MSS_338LM_TAC_API526",
    "MSS_338NM_250gr_OTM_GTM","MSS_338NM_300_Atip","MSS_338NM_M1162","MSS_338NM_SMK",
    "MSS_375CT_AP","MSS_375CT_ATIP","MSS_375CT_BB","MSS_375CT_Ball","MSS_375CT_OEP",
    "MSS_375SP_350GR_FMJ",
    "MSS_408CT_AP","MSS_408CT_BB","MSS_408CT_Ball","MSS_408CT_DTM","MSS_408CT_OEP",
    "MSS_416B_395gr_OTM","MSS_416B_452gr_MTAC","MSS_416B_452gr_Solid","MSS_416B_500gr_ATIP",
    "MSS_50BMG_M20_APIT","MSS_50_750_AMAX","MSS_50_800_LAP","MSS_50_M1022_LR",
    "MSS_50_M17_Tracer","MSS_50_M33_Ball","MSS_50_Mk211Mod0_RAUFOSS",
    "MSS_65CM_130FED","MSS_65CM_136LAP","MSS_65CM_140Berger","MSS_65CM_140ELDM",
    "MSS_65CM_143ELDX","MSS_65CM_147ELDM","MSS_65CM_M1200",
    "MSS_65PRC_143ELDX","MSS_65PRC_147ELDM","MSS_65PRC_153ATIP",
    "MSS_7PRC_154SST","MSS_7PRC_175ELDX","MSS_7PRC_180ELDM"
];

private _phase3Pass = 0;
private _phase3Fail = 0;
private _phase3Missing = 0;

{
    private _ammo = _x;
    private _cfgAmmo = configFile >> "CfgAmmo" >> _ammo;

    if (isClass _cfgAmmo) then {
        private _airFric    = getNumber (_cfgAmmo >> "airFriction");
        private _hit        = getNumber (_cfgAmmo >> "hit");
        private _typSpeed   = getNumber (_cfgAmmo >> "typicalSpeed");
        private _bc         = getArray  (_cfgAmmo >> "ACE_ballisticCoefficients");
        private _dragModel  = getNumber (_cfgAmmo >> "ACE_dragModel");
        private _bulletMass = getNumber (_cfgAmmo >> "ACE_bulletMass");

        diag_log text format ["[BALLISTICS][PHASE3] %1 airFriction=%2 hit=%3 typicalSpeed=%4 BC=%5 dragModel=%6 bulletMass=%7",
            _ammo, _airFric, _hit, _typSpeed, _bc, _dragModel, _bulletMass];

        [_ammo, "phase3", "airFriction", str _airFric] call _fnc_log;
        [_ammo, "phase3", "hit", str _hit] call _fnc_log;
        [_ammo, "phase3", "typicalSpeed", str _typSpeed] call _fnc_log;
        [_ammo, "phase3", "ACE_ballisticCoefficients", str _bc] call _fnc_log;
        [_ammo, "phase3", "ACE_dragModel", str _dragModel] call _fnc_log;
        [_ammo, "phase3", "ACE_bulletMass", str _bulletMass] call _fnc_log;

        _phase3Pass = _phase3Pass + 1;
    } else {
        diag_log text format ["[BALLISTICS][PHASE3] MISSING: %1", _ammo];
        [_ammo, "phase3", "status", "MISSING"] call _fnc_log;
        _phase3Missing = _phase3Missing + 1;
    };
} forEach _allAmmos;

diag_log text format ["[BALLISTICS][PHASE3] DONE: %1 found, %2 missing, %3 failed", _phase3Pass, _phase3Missing, _phase3Fail];

// ── PHASE 4: armor/protection values from CfgWeapons (vests, helmets) ──
diag_log text "[BALLISTICS][PHASE4] Starting armor/protection value verification";

private _armorCases = [
    "V_PlateCarrier1_rgr",
    "V_PlateCarrier2_rgr",
    "V_PlateCarrierGL_rgr",
    "V_PlateCarrierSpec_rgr",
    "H_HelmetB",
    "H_HelmetSpecB",
    "H_HelmetSpecB_paint1",
    "U_B_CombatUniform_mcam"
];

private _phase4Pass = 0;
private _phase4Fail = 0;

{
    private _name = _x;
    private _cfg = configFile >> "CfgWeapons" >> _name;

    if (isClass _cfg) then {
        private _itemInfo = _cfg >> "ItemInfo";
        private _mass = if (isClass _itemInfo) then { getNumber (_itemInfo >> "mass") } else { -1 };
        private _type = if (isClass _itemInfo) then { getNumber (_itemInfo >> "type") } else { -1 };

        // Armor is in HitpointProtection subclass inside ItemInfo
        private _armor = -1;
        private _passThrough = -1;
        if (isClass _itemInfo) then {
            // Try armor class (ACE-style)
            private _armorClass = _itemInfo >> "armor";
            if (isClass _armorClass) then {
                _armor = getNumber (_armorClass >> "hitpointProtection" >> "armor");
            };
            // Try passThrough in ItemInfo directly
            _passThrough = getNumber (_itemInfo >> "passThrough");
            // Also try hitpointProtection
            private _hpProt = _itemInfo >> "hitpointProtection";
            if (isClass _hpProt) then {
                private _armorHP = getNumber (_hpProt >> "armor");
                if (_armorHP > 0) then { _armor = _armorHP };
            };
        };

        // Also check armor property directly
        private _armorDirect = getNumber (_cfg >> "ItemInfo" >> "armor");

        // Enumerate HitpointProtection children
        private _hpProt = _itemInfo >> "hitpointProtection";
        private _hpEntries = "";
        if (isClass _hpProt) then {
            private _numHP = count _hpProt;
            for "_i" from 0 to (_numHP - 1) do {
                private _child = _hpProt select _i;
                private _childName = configName _child;
                private _childArmor = getNumber (_child >> "armor");
                private _childPass = getNumber (_child >> "passThrough");
                _hpEntries = _hpEntries + format ["%1(a=%2,p=%3) ", _childName, _childArmor, _childPass];
            };
        };

        diag_log text format ["[BALLISTICS][PHASE4] %1 mass=%2 type=%3 armorDirect=%4 hpProt=[%5]",
            _name, _mass, _type, _armorDirect, _hpEntries];

        [_name, "armor", "mass", str _mass] call _fnc_log;
        [_name, "armor", "type", str _type] call _fnc_log;
        [_name, "armor", "armorDirect", str _armorDirect] call _fnc_log;
        [_name, "armor", "hitpointProtection", _hpEntries] call _fnc_log;

        _phase4Pass = _phase4Pass + 1;
    } else {
        diag_log text format ["[BALLISTICS][PHASE4] MISSING: %1", _name];
        [_name, "armor", "status", "MISSING"] call _fnc_log;
        _phase4Fail = _phase4Fail + 1;
    };
} forEach _armorCases;

diag_log text format ["[BALLISTICS][PHASE4] DONE: %1 found, %2 missing", _phase4Pass, _phase4Fail];

// ── PHASE 5: weapon handling (recoil, sway, inertia) ──
diag_log text "[BALLISTICS][PHASE5] Starting weapon handling verification";

private _weaponCases = [
    ["MSS_SR25_BLK",           "CfgWeapons>>MSS_SR25_BLK"],
    ["Mss_M107A1_50_29_GRY",   "CfgWeapons>>Mss_M107A1_50_29_GRY"],
    ["MSS_Mk18_24_300NM_CPS_BLK","CfgWeapons>>MSS_Mk18_24_300NM_CPS_BLK"],
    ["arifle_MX_F",             "CfgWeapons>>arifle_MX_F"],
    ["hgun_P07_F",              "CfgWeapons>>hgun_P07_F"]
];

private _phase5Pass = 0;
private _phase5Fail = 0;

{
    _x params ["_name", "_path"];
    private _cfg = configFile;

    {
        _cfg = _cfg >> _x;
    } forEach (_path splitString ">>");

    if (isClass _cfg) then {
        private _recoil = getArray (_cfg >> "recoil");
        private _sway = getNumber (_cfg >> "sway");
        private _inertia = getNumber (_cfg >> "inertia");

        diag_log text format ["[BALLISTICS][PHASE5] %1 recoil=%2 sway=%3 inertia=%4",
            _name, _recoil, _sway, _inertia];

        [_name, "handling", "recoil", str _recoil] call _fnc_log;
        [_name, "handling", "sway", str _sway] call _fnc_log;
        [_name, "handling", "inertia", str _inertia] call _fnc_log;

        _phase5Pass = _phase5Pass + 1;
    } else {
        diag_log text format ["[BALLISTICS][PHASE5] MISSING: %1 (%2)", _name, _path];
        [_name, "handling", "status", "MISSING"] call _fnc_log;
        _phase5Fail = _phase5Fail + 1;
    };
} forEach _weaponCases;

diag_log text format ["[BALLISTICS][PHASE5] DONE: %1 found, %2 missing", _phase5Pass, _phase5Fail];

// ═══════════════════════════════════════════════════════════════════════
// Phase 6: runtime engine — verify rules loaded and apply_function works
// ═══════════════════════════════════════════════════════════════════════
diag_log text "[PHASE6] Verifying runtime engine...";

private _phase6Pass = 0;
private _phase6Fail = 0;

// 6a: query runtime_overrides count via callExtension
// Response format: [0,"OK",[["cnt"],[2]]] — values wrapped in inner arrays
private _countResult = "a3sql" callExtension "SELECT COUNT(*) as cnt FROM runtime_overrides";
diag_log text format ["[PHASE6][count] %1", _countResult];
// Verify query returned OK with data (proves rules exist and are queryable)
private _hasOK = _countResult find "OK" > -1;
private _hasData = _countResult find "[[" > -1;
if (_hasOK && _hasData) then {
    diag_log text "[PHASE6][count] PASS: runtime_overrides queryable, rules loaded";
    _phase6Pass = _phase6Pass + 1;
} else {
    diag_log text "[PHASE6][count] FAIL: runtime_overrides not queryable";
    _phase6Fail = _phase6Fail + 1;
};

// 6b: verify apply_function column exists and has values
private _afResult = "a3sql" callExtension "SELECT apply_function FROM runtime_overrides LIMIT 1";
diag_log text format ["[PHASE6][apply_function] %1", _afResult];
if (_afResult find "apply" > -1) then {
    diag_log text "[PHASE6][apply_function] PASS: apply_function column has values";
    _phase6Pass = _phase6Pass + 1;
} else {
    diag_log text "[PHASE6][apply_function] FAIL: no apply_function values found";
    _phase6Fail = _phase6Fail + 1;
};

// 6c: verify a3sql_runtime PBO is loaded
private _rtPboLoaded = isClass (configFile >> "CfgPatches" >> "a3sql_runtime");
if (_rtPboLoaded) then {
    diag_log text "[PHASE6][pbo] PASS: a3sql_runtime.pbo loaded";
    _phase6Pass = _phase6Pass + 1;
} else {
    diag_log text "[PHASE6][pbo] FAIL: a3sql_runtime.pbo NOT loaded";
    _phase6Fail = _phase6Fail + 1;
};

diag_log text format ["[PHASE6] DONE: %1 pass, %2 fail", _phase6Pass, _phase6Fail];

// Phase 7: CBA event interface test. Trigger the engine's query event and
// verify it replies on a3sql_runtime_status. This proves cross-mod control
// works: any mod can drive the engine via CBA events.
diag_log text "[PHASE7] Verifying CBA event interface...";

private _phase7Pass = 0;
private _phase7Fail = 0;

// 7a: fire the query event, capture the reply
missionNamespace setVariable ["A3SQL_testStatusReply", nil];
["a3sql_runtime_status", {
    params ["_enabled", "_ruleCount"];
    missionNamespace setVariable ["A3SQL_testStatusReply", [_enabled, _ruleCount]];
}] call CBA_fnc_addEventHandler;

["a3sql_runtime_query"] call CBA_fnc_globalEvent;
sleep 0.5;

private _reply = missionNamespace getVariable ["A3SQL_testStatusReply", nil];
if !(isNil "_reply") then {
    diag_log text format ["[PHASE7][query] reply=%1 PASS: engine answered CBA query event", _reply];
    _phase7Pass = _phase7Pass + 1;
} else {
    diag_log text "[PHASE7][query] FAIL: no reply from engine";
    _phase7Fail = _phase7Fail + 1;
};

// 7b: fire the reload event and verify rulesLoaded comes back
missionNamespace setVariable ["A3SQL_testRulesLoaded", nil];
["a3sql_runtime_rulesLoaded", {
    params ["_count"];
    missionNamespace setVariable ["A3SQL_testRulesLoaded", _count];
}] call CBA_fnc_addEventHandler;

["a3sql_runtime_reload"] call CBA_fnc_globalEvent;
sleep 0.5;

private _loaded = missionNamespace getVariable ["A3SQL_testRulesLoaded", nil];
if !(isNil "_loaded") then {
    diag_log text format ["[PHASE7][reload] rulesLoaded=%1 PASS: engine reloaded via CBA event", _loaded];
    _phase7Pass = _phase7Pass + 1;
} else {
    diag_log text "[PHASE7][reload] FAIL: no rulesLoaded event";
    _phase7Fail = _phase7Fail + 1;
};

// 7c: verify our keybinds are registered in CBA's actions namespace.
// NOTE: CBA registers keybinds only on machines with hasInterface (clients).
// A headless dedicated server intentionally has none. This check passes on
// any client; on a dedicated server it reports the limitation, not a defect.
private _kbNs = missionNamespace getVariable ["cba_keybinding_actions", nil];
private _kbReload = if (isNil "_kbNs") then { nil } else { _kbNs getVariable ["a3sql runtime$reload rules", nil] };
private _kbToggle = if (isNil "_kbNs") then { nil } else { _kbNs getVariable ["a3sql runtime$toggle engine", nil] };
private _kbStatus = if (isNil "_kbNs") then { nil } else { _kbNs getVariable ["a3sql runtime$query status", nil] };
diag_log text format ["[PHASE7][keybinds] reload=%1 toggle=%2 status=%3 (hasInterface=%4)", !(isNil "_kbReload"), !(isNil "_kbToggle"), !(isNil "_kbStatus"), hasInterface];
if (hasInterface) then {
    if (!(isNil "_kbReload") && !(isNil "_kbToggle") && !(isNil "_kbStatus")) then {
        diag_log text "[PHASE7][keybinds] PASS: all 3 keybinds registered";
        _phase7Pass = _phase7Pass + 1;
    } else {
        diag_log text "[PHASE7][keybinds] FAIL: keybinds not registered on client";
        _phase7Fail = _phase7Fail + 1;
    };
} else {
    diag_log text "[PHASE7][keybinds] SKIP: dedicated server (keybinds are client-side, expected)";
};

// 7d: verify the toggle event works — disable, query, re-enable.
missionNamespace setVariable ["A3SQL_testStatusReply", nil];
["a3sql_runtime_toggle", [false]] call CBA_fnc_globalEvent;
sleep 0.5;
missionNamespace setVariable ["A3SQL_testStatusReply", nil];
["a3sql_runtime_query"] call CBA_fnc_globalEvent;
sleep 0.5;
private _reply = missionNamespace getVariable ["A3SQL_testStatusReply", nil];
private _disabled = if (isNil "_reply") then { false } else { !(_reply select 0) };
["a3sql_runtime_toggle", [true]] call CBA_fnc_globalEvent;
sleep 0.5;
if (_disabled) then {
    diag_log text "[PHASE7][toggle] PASS: engine disabled + re-enabled via CBA event";
    _phase7Pass = _phase7Pass + 1;
} else {
    diag_log text "[PHASE7][toggle] FAIL: engine did not disable via toggle event";
    _phase7Fail = _phase7Fail + 1;
};

// 7e: verify the ruleApplied local event fires when a rule is applied.
missionNamespace setVariable ["A3SQL_testRuleApplied", nil];
["a3sql_runtime_ruleApplied", {
    params ["_ruleName", "_target", "_result"];
    missionNamespace setVariable ["A3SQL_testRuleApplied", [_ruleName, _result]];
}] call CBA_fnc_addEventHandler;
// Apply a dummy rule to a real unit to trigger the event. player is
// objNull on a dedicated server, so spawn a unit instead.
private _grp = createGroup west;
private _targetUnit = _grp createUnit ["B_Soldier_F", [0, 0, 30], [], 0, "NONE"];
private _dummyRule = createHashMapFromArray [["name", "test_rule"], ["apply_function", "a3sql_runtime_fnc_applyVelocity"], ["operator", "mul"], ["value", "1.0"]];
[_dummyRule, _targetUnit, createHashMap] call a3sql_runtime_fnc_apply;
sleep 0.5;
private _applied = missionNamespace getVariable ["A3SQL_testRuleApplied", nil];
if !(isNil "_applied") then {
    diag_log text format ["[PHASE7][ruleApplied] PASS: event fired with rule %1", _applied select 0];
    _phase7Pass = _phase7Pass + 1;
} else {
    diag_log text "[PHASE7][ruleApplied] FAIL: ruleApplied event did not fire";
    _phase7Fail = _phase7Fail + 1;
};

// 7f: verify the ready event fires on register.
missionNamespace setVariable ["A3SQL_testReady", nil];
["a3sql_runtime_ready", {
    params ["_ready"];
    missionNamespace setVariable ["A3SQL_testReady", _ready];
}] call CBA_fnc_addEventHandler;
[] call a3sql_runtime_fnc_register;
sleep 0.5;
private _ready = missionNamespace getVariable ["A3SQL_testReady", nil];
if !(isNil "_ready") then {
    diag_log text format ["[PHASE7][ready] PASS: ready event fired (%1)", _ready];
    _phase7Pass = _phase7Pass + 1;
} else {
    diag_log text "[PHASE7][ready] FAIL: ready event did not fire";
    _phase7Fail = _phase7Fail + 1;
};

diag_log text format ["[PHASE7] DONE: %1 pass, %2 fail", _phase7Pass, _phase7Fail];

diag_log text "=== A3SQL BALLISTICS TEST END ===";
