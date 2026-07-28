diag_log "[A3SQL Test] Starting loadouts tests...";

// Test: createTemplate
private _created = ["TestTemplate", "testFaction", []] call a3sql_loadouts_fnc_createTemplate;
diag_log format ["[A3SQL Test] createTemplate: %1", _created];

// Test: listTemplates
private _templates = call a3sql_loadouts_fnc_listTemplates;
diag_log format ["[A3SQL Test] listTemplates returned: %1", _templates];

// Test: listByFaction
private _byFaction = ["testFaction"] call a3sql_loadouts_fnc_listByFaction;
diag_log format ["[A3SQL Test] listByFaction: %1", _byFaction];

// Cleanup
["TestTemplate"] call a3sql_loadouts_fnc_deleteTemplate;

diag_log "[A3SQL Test] Loadouts tests complete.";
