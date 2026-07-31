# ── Corpus: ADMIN / SERVER COMMANDS ────────────────────────────────────
# Realistic queries from admin command systems, ban tracking, whitelist
# registries, and server management tooling.

CREATE TABLE IF NOT EXISTS server_commands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command TEXT NOT NULL,
    params TEXT,
    source TEXT DEFAULT 'sqf',
    status TEXT DEFAULT 'pending',
    executed_by TEXT,
    created_at TEXT
);

CREATE TABLE IF NOT EXISTS players (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rank TEXT DEFAULT 'private',
    whitelisted INTEGER DEFAULT 0,
    bans INTEGER DEFAULT 0,
    last_join TEXT
);

# Admin issues a ban — datetime('now') in VALUES (mods do this)
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('ban', '76561198000000002 0', 'sqf', 'pending', datetime('now'));

# Kick command
INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('kick', '76561198000000003', 'sqf', 'pending', datetime('now'));

# Whitelist check on join
# expect contains "OK"
SELECT uid, name, whitelisted FROM players WHERE uid = '76561198000000001';

# Player join — upsert pattern
INSERT OR REPLACE INTO players (uid, name, last_join) VALUES ('76561198000000001', 'PlayerOne', datetime('now'));

# Ban escalation — update then log
UPDATE players SET bans = bans + 1, whitelisted = 0 WHERE uid = '76561198000000001';

# Pending commands queue (the admin system's worker query)
# expect contains "ban"
SELECT id, command, params FROM server_commands WHERE status = 'pending' ORDER BY id ASC LIMIT 10;

# Mark executed
UPDATE server_commands SET status = 'executed' WHERE id = 1;

# Command audit trail with joins
# expect contains "OK"
SELECT c.id, c.command, c.params, p.name AS operator
FROM server_commands c
LEFT JOIN players p ON p.uid = c.executed_by
WHERE c.status = 'executed'
ORDER BY c.id DESC LIMIT 50;

# Rank distribution
SELECT rank, COUNT(*) AS count FROM players GROUP BY rank ORDER BY count DESC;

# Time-based cleanup of old command log
DELETE FROM server_commands WHERE created_at < datetime('now', '-30 days');

# Whitelist purge
DELETE FROM players WHERE whitelisted = 0 AND last_join < datetime('now', '-90 days');

# Aggregate: banned players count
# expect contains "OK"
SELECT COUNT(*) AS banned FROM players WHERE whitelisted = 0;
