diag_log "[A3SQL Test] Starting patch tests...";

// Test: getRule (no rules yet, should return empty)
private _rule = ["test_rule"] call a3sql_patch_core_fnc_getRule;
diag_log format ["[A3SQL Test] getRule: %1", _rule];

// Test: listRules
private _rules = call a3sql_patch_core_fnc_listRules;
diag_log format ["[A3SQL Test] listRules returned: %1 rules", count _rules];

// Test: listOverrides (patch_operators must be loaded)
if (!isNil "a3sql_patch_operators_fnc_listOverrides") then {
    private _overrides = call a3sql_patch_operators_fnc_listOverrides;
    diag_log format ["[A3SQL Test] listOverrides: %1", _overrides];
};

// Test: applyAll (safe with no rules)
private _applied = call a3sql_patch_core_fnc_applyAll;
diag_log format ["[A3SQL Test] applyAll: %1", _applied];

diag_log "[A3SQL Test] Patch tests complete.";
