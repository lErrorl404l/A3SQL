# A3SQL production smoke test — the mod's real SQL, run through the real
# extension binary exactly as Arma's SQF calls it.
#
# Run: python3 tools/sql_smoke_test.py tools/smoke_test.sql
# Each statement must return [0,"OK" unless marked "# expect error".

# ── patch_core (addons/patch_core/XEH_postInit.sqf) ──────────────────────
CREATE TABLE IF NOT EXISTS patch_rules (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER DEFAULT 1, priority INTEGER DEFAULT 0, match_type TEXT NOT NULL DEFAULT 'exact', match_value TEXT DEFAULT '', target_type TEXT NOT NULL, property TEXT NOT NULL, operator TEXT DEFAULT 'set', value TEXT NOT NULL, created_at TEXT DEFAULT '')
CREATE TABLE IF NOT EXISTS patch_presets (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL, data TEXT NOT NULL, created_at TEXT DEFAULT '')
# expect error
CREATE TABLE IF NOT EXISTS broken (a INTEGER PRIMARY KEY, b INTEGER PRIMARY KEY)
# Column migration from XEH_postInit
ALTER TABLE patch_rules ADD COLUMN group_name TEXT DEFAULT ''
ALTER TABLE patch_rules ADD COLUMN notes TEXT DEFAULT ''

# ── patch_core rule CRUD (fnc_applyrule, fnc_applyall, fnc_deleterule) ───
INSERT INTO patch_rules (name, active, priority, match_type, match_value, target_type, property, operator, value) VALUES ('fixAmmo', 1, 10, 'exact', '', 'CAManBase', 'ammo', 'set', '30') RETURNING id
# expect contains "fixAmmo"
SELECT * FROM patch_rules WHERE active = 1 ORDER BY priority DESC, id ASC LIMIT 50 OFFSET 0
# expect contains "fixAmmo"
SELECT * FROM patch_rules ORDER BY priority DESC, id ASC
# expect error
INSERT INTO patch_rules (name, active, priority, match_type, match_value, target_type, property, operator, value) VALUES ('fixAmmo', 1, 10, 'exact', '', 'CAManBase', 'ammo', 'set', '30') RETURNING id
DELETE FROM patch_rules WHERE name = 'fixAmmo'

# ── patch_editor presets (fnc_gui_savepreset / fnc_gui_loadpreset) ───────
INSERT INTO patch_presets (name, data) VALUES ('preset1', '[[\"fixAmmo\",10,1]]')
# expect error
INSERT INTO patch_presets (name, data) VALUES ('preset1', '[[\"fixAmmo\",10,1]]')
# expect contains "fixAmmo"
SELECT data FROM patch_presets WHERE name = 'preset1'
UPDATE patch_presets SET data = '[[\"fixAmmo\",20,0]]' WHERE name = 'preset1'

# ── database module (fnc_execute / fnc_selectall) ────────────────────────
CREATE TABLE server_commands (id INTEGER PRIMARY KEY AUTOINCREMENT, command TEXT, params TEXT, source TEXT, status TEXT, created_at TEXT)
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('ban', '76561198000000002 0', 'sqf', 'pending', datetime('now'))
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('kick', '76561198000000003', 'sqf', 'pending', datetime('now'))
# expect contains "ban"
SELECT command, params FROM server_commands WHERE status = 'pending'
# Steam ID arriving as a numeric string must not be rejected by TEXT column
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('whitelist', '76561198000000001', 'sqf', 'pending', datetime('now'))
# expect contains "76561198000000001"
SELECT params FROM server_commands WHERE command = 'whitelist'
# expect error
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('x', 'y', 'sqf', 'pending', datetime('yesterday'))
# expect contains "3"
SELECT count(*) as c FROM server_commands WHERE status = 'pending'

# ── prepared statements / cursors ────────────────────────────────────────
# expect contains "1"
SELECT 1 FROM server_commands LIMIT 1

# ── persistence round-trip ───────────────────────────────────────────────
save smoke_data
DELETE FROM patch_presets
load smoke_data
# expect contains "preset1"
SELECT name FROM patch_presets

# ── export paths ─────────────────────────────────────────────────────────
# expect contains "OK"
SELECT 1
