# ── Corpus: SQF PERSISTENCE MIGRATION ─────────────────────────────────
# Models what mods ACTUALLY do today without SQL — profileNamespace
# key-value stores, CBA persistent settings, ACE-Arsenal-style loadout
# blobs, missionNamespace stats arrays, event feeds. Each pattern is
# expressed as the a3sql equivalent, proving the migration path.

# ── Pattern 1: key-value store (profileNamespace equivalent) ───────────
# Mods: `profileNamespace setVariable ["myMod_key", value]`
CREATE TABLE kv_store (
    mod_key TEXT PRIMARY KEY,
    value TEXT,
    updated_at TEXT
);

# setVariable equivalent — upsert on write
INSERT OR REPLACE INTO kv_store (mod_key, value, updated_at) VALUES ('myMod_radioChannels', '["ch1","ch2"]', datetime('now'));

# getVariable equivalent
# expect contains "ch1"
SELECT value FROM kv_store WHERE mod_key = 'myMod_radioChannels';

# overwrite (mods overwrite keys constantly)
INSERT OR REPLACE INTO kv_store (mod_key, value, updated_at) VALUES ('myMod_radioChannels', '["ch1","ch3"]', datetime('now'));
# expect contains "ch3"
SELECT value FROM kv_store WHERE mod_key = 'myMod_radioChannels';

# deleteVariable equivalent
DELETE FROM kv_store WHERE mod_key = 'myMod_radioChannels';

# ── Pattern 2: per-player blobs (ACE Arsenal loadout style) ────────────
# Mods: `profileNamespace setVariable [format ["myMod_loadout_%1", uid], loadout]`
CREATE TABLE player_blobs (
    player_uid TEXT PRIMARY KEY,
    blob TEXT,
    saved_at TEXT DEFAULT ''
);

# save an SQF-serialized loadout array
INSERT OR REPLACE INTO player_blobs (player_uid, blob, saved_at) VALUES ('76561198000000001', '["rhs_m4a1","30Rnd_556x45","rhs_weap_m4","NVGoggles"]', datetime('now'));

# load on join — mod's initPlayerLocal equivalent
# expect contains "rhs_m4a1"
SELECT blob FROM player_blobs WHERE player_uid = '76561198000000001';

# update blob on loadout change
UPDATE player_blobs SET blob = '["rhs_mk18","30Rnd_556x45","Mk18"]', saved_at = datetime('now') WHERE player_uid = '76561198000000001';

# ── Pattern 3: stat counters (profileNamespace numbers) ────────────────
# Mods: `profileNamespace setVariable ["myMod_totalKills", _current + 1]`
CREATE TABLE counters (
    counter_key TEXT PRIMARY KEY,
    value INTEGER DEFAULT 0
);

INSERT OR IGNORE INTO counters (counter_key, value) VALUES ('total_kills', 0);
UPDATE counters SET value = value + 1 WHERE counter_key = 'total_kills';
UPDATE counters SET value = value + 1 WHERE counter_key = 'total_kills';

# read counter
# expect contains "2"
SELECT value FROM counters WHERE counter_key = 'total_kills';

# ── Pattern 4: event feed (missionNamespace arrays) ────────────────────
# Mods: `_feed pushBack [time, type, data]` in missionNamespace
CREATE TABLE event_feed (
    seq INTEGER PRIMARY KEY,
    event_type TEXT,
    payload TEXT,
    created_at TEXT DEFAULT ''
);

INSERT INTO event_feed (event_type, payload) VALUES ('kill', '{"killer":"a","victim":"b"}');
INSERT INTO event_feed (event_type, payload) VALUES ('assist', '{"killer":"a","victim":"c"}');
INSERT INTO event_feed (event_type, payload) VALUES ('kill', '{"killer":"c","victim":"b"}');

# pushBack count — mods read _feed length
# expect contains "3"
SELECT COUNT(*) FROM event_feed;

# latest N events — mods iterate the tail
# expect contains "OK"
SELECT seq, event_type, payload FROM event_feed ORDER BY seq DESC LIMIT 5;

# ── Pattern 5: per-player state map (multi-key per uid) ────────────────
# Mods: `profileNamespace setVariable [format ["myMod_%1_%2", uid, key], v]`
CREATE TABLE player_state (
    uid TEXT,
    state_key TEXT,
    state_value TEXT,
    PRIMARY KEY (uid, state_key)
);

INSERT INTO player_state (uid, state_key, state_value) VALUES ('u1', 'role', 'medic');
INSERT INTO player_state (uid, state_key, state_value) VALUES ('u1', 'squad', 'alpha');
INSERT INTO player_state (uid, state_key, state_value) VALUES ('u2', 'role', 'rifleman');

# mod reads one key
# expect contains "medic"
SELECT state_value FROM player_state WHERE uid = 'u1' AND state_key = 'role';

# mod lists all state for a player (profileNamespace iteration equivalent)
# expect contains "OK"
SELECT state_key, state_value FROM player_state WHERE uid = 'u1' ORDER BY state_key;

# ── Pattern 6: whole-store save/load (mission end / start) ─────────────
# Mods: `saveProfileNamespace` / ACE's persistent save
save sqf_persistence;
# expect contains "OK"
SELECT COUNT(*) FROM kv_store;
