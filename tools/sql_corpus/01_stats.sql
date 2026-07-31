# ── Corpus: STATS / SCORING ────────────────────────────────────────────
# Realistic queries from kill-tracking / stat mods (stats trackers,
# leaderboard systems). Statements end with ';' and may span lines.

# Schema setup — session stats with PK + UNIQUE player
CREATE TABLE IF NOT EXISTS session_stats (
    uid INTEGER PRIMARY KEY,
    player_name TEXT NOT NULL,
    kills INTEGER DEFAULT 0,
    deaths INTEGER DEFAULT 0,
    assists INTEGER DEFAULT 0,
    score INTEGER DEFAULT 0,
    team TEXT DEFAULT 'OPFOR',
    last_seen TEXT DEFAULT ''
);

# Per-kill event log (analytic mods)
CREATE TABLE IF NOT EXISTS kill_events (
    id INTEGER PRIMARY KEY,
    ts TEXT,
    killer_uid INTEGER,
    victim_uid INTEGER,
    killer_weapon TEXT,
    distance INTEGER,
    headshot INTEGER DEFAULT 0,
    killer_side TEXT
);

# Player joins — auto rowid
INSERT INTO session_stats (player_name, kills, deaths, assists, score) VALUES ('PlayerOne', 0, 0, 0, 0);
INSERT OR IGNORE INTO session_stats (uid, player_name, kills, deaths, assists, score) VALUES (76561198000000001, 'PlayerOne', 0, 0, 0, 0);

# A kill happens
INSERT INTO kill_events (killer_uid, victim_uid, killer_weapon, distance, headshot, killer_side) VALUES (76561198000000001, 76561198000000002, 'rhs_m4a1', 300, 1, 'BLUFOR');

# Leaderboard — the classic stat-mod query
# expect contains "PlayerOne"
SELECT player_name, kills, deaths, score FROM session_stats ORDER BY score DESC LIMIT 10;

# Per-weapon breakdown
SELECT killer_weapon, COUNT(*) AS kills, AVG(distance) AS avg_dist, SUM(headshot) AS hs
FROM kill_events GROUP BY killer_weapon ORDER BY kills DESC;

# Time-window query (last 24h via date modifier)
# expect contains "OK"
SELECT COUNT(*) FROM kill_events WHERE ts > datetime('now', '-1 day');

# Team aggregate
SELECT team, COUNT(*) AS players, SUM(kills) AS total_kills, AVG(score) AS avg_score
FROM session_stats GROUP BY team HAVING COUNT(*) > 0;

# Update on disconnect
UPDATE session_stats SET last_seen = datetime('now') WHERE uid = 76561198000000001;

# Cleanup old events (TTL pattern)
DELETE FROM kill_events WHERE ts < datetime('now', '-7 days');

# Joins between the two tables
# expect contains "OK"
SELECT s.player_name, COUNT(k.id) AS kill_count
FROM session_stats s
LEFT JOIN kill_events k ON s.uid = k.killer_uid
GROUP BY s.player_name ORDER BY kill_count DESC;

# Nested aggregate — most frequent victim of each killer
# expect contains "OK"
SELECT killer_uid, victim_uid, COUNT(*) AS times
FROM kill_events
WHERE (killer_uid, victim_uid) IN (
    SELECT killer_uid, victim_uid FROM kill_events GROUP BY killer_uid, victim_uid HAVING COUNT(*) > 1
)
GROUP BY killer_uid, victim_uid;
