# ── Corpus: LOADOUTS / PERSISTENCE ─────────────────────────────────────
# Realistic queries from loadout-manager and player-persistence mods.

CREATE TABLE IF NOT EXISTS loadout_templates (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    faction TEXT NOT NULL,
    role TEXT DEFAULT 'rifleman',
    loadout_data TEXT NOT NULL,
    created_at TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS player_loadouts (
    uid TEXT PRIMARY KEY,
    template_id INTEGER,
    custom_data TEXT,
    saved_at TEXT DEFAULT '',
    FOREIGN KEY (template_id) REFERENCES loadout_templates(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS progression (
    uid TEXT PRIMARY KEY,
    xp INTEGER DEFAULT 0,
    level INTEGER DEFAULT 1,
    prestige INTEGER DEFAULT 0,
    playtime_seconds INTEGER DEFAULT 0
);

# Template creation (the loadout manager's save flow)
INSERT INTO loadout_templates (name, faction, role, loadout_data) VALUES ('Rifleman M4', 'BLUFOR', 'rifleman', '["rhs_m4a1","30Rnd_556x45"]');

# List templates for a faction — the browser query
# expect contains "Rifleman"
SELECT id, name, role FROM loadout_templates WHERE faction = 'BLUFOR' ORDER BY name ASC;

# Fuzzy search — a3sql's %% operator (like a forgiving LIKE)
# expect contains "OK"
SELECT id, name FROM loadout_templates WHERE name %% 'rifle';

# Assign loadout to player — upsert
INSERT OR REPLACE INTO player_loadouts (uid, template_id, saved_at) VALUES ('76561198000000001', 1, datetime('now'));

# Get player's loadout
# expect contains "OK"
SELECT lt.name, lt.loadout_data FROM player_loadouts pl
JOIN loadout_templates lt ON lt.id = pl.template_id
WHERE pl.uid = '76561198000000001';

# Progression tick
UPDATE progression SET xp = xp + 100, playtime_seconds = playtime_seconds + 600 WHERE uid = '76561198000000001';
INSERT OR IGNORE INTO progression (uid, xp, playtime_seconds) VALUES ('76561198000000001', 0, 0);

# Level-up check (subquery + arithmetic)
# expect contains "OK"
SELECT uid, xp, level FROM progression WHERE xp >= level * 1000;

# Leaderboard by progression
SELECT uid, level, prestige FROM progression ORDER BY prestige DESC, level DESC, xp DESC LIMIT 20;

# Persistence round-trip via the extension's save/load (control commands)
save corpus_loadouts;

# Count templates per role (HAVING with alias)
# expect contains "OK"
SELECT role, COUNT(*) AS count FROM loadout_templates GROUP BY role HAVING count > 0;

# Delete a template and cascade to player assignments (FK cascade)
DELETE FROM loadout_templates WHERE id = 1;
