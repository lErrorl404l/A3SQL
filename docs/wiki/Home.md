# A3SQL: Arma 3 Database Engine

A3SQL is an embeddable SQL database engine for Arma 3. Think SQLite for Arma: a Rust callExtension that gives your mod, mission, or server a real SQL database at runtime. Store player stats, log events, track progression, and persist everything across missions, all with plain SQL from SQF. Mission makers use it for persistence and analytics, mod developers embed it as a dependency, and server admins query it from external tools over TCP.

## Quick Start

```sqf
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;
["INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3sql_fnc_execute;
// _result = [0, "OK", [["name","score"],["Scarface",1500]]]
```

Every query returns the same envelope: `[code, status, data]`. Code `0` with status `"OK"` means the statement ran; anything else is an error code with a message you can read. See [Getting Started](Getting-Started) for the full walkthrough.

## Built for Mission Data

- In-memory engine: a few thousand rows is nothing, 10k to 100k rows is comfortable. Keep your tables under 100k rows.
- No silent truncation: a SELECT that would overflow the 30KB response buffer fails loudly and tells you to page the result with cursors.
- Safe by default: parameterized queries stop SQL injection, and the TCP listener binds to loopback and requires LOGIN by default (fail-closed).
- Autosave hooks hold the database mutex only briefly, so mission-end saves do not stall gameplay.

## Wiki Pages

- **[Getting Started](Getting-Started)**: build your first stats-persistence mod, step by step
- **[SQL Dialect](SQL-Dialect)**: supported SQL syntax
- **[SQL Compatibility Report](SQL-Compatibility-Report)**: test your mod's SQL against the real engine
- **[Production Readiness](Production-Readiness)**: deployment status, gaps, and the 30-minute production checklist
- **[CBA Settings](CBA-Settings)**: addon configuration options
- **[Security](Security)**: parameterized queries, TCP authentication
- **[TCP Connector](TCP-Connector)**: external query access
- **[Standalone Server](Standalone-Server)**: run without Arma, remote mode
- **[Plugins](Plugins)**: extend A3SQL with Rust, C, or SQF plugins
- **[Development Setup](Development-Setup)**: full dev environment guide
- **[Building](Building)**: compiling the extension and addon
- **[Patch Framework](Patch-Framework)**: dynamic live-patching system (weapon values, textures, materials, config overrides, rule groups)
- **[Module Guide](Module-Guide)**: integrating mods with a3sql (analytics, loadouts, persistence, progression, multi-server sync)

## Status

| Component | Status |
|---|---|
| **Tests** | [![Tests](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml) |
| **Lint** | [![Lint](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml) |
| **Build** | [![Build](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml) |
| **CI** | [![CI](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml) |
| **Coverage** | The full dialect sweep and test suite against the real extension binary: the complete `a3sql_fnc_*` API (execute, save, load, loadJSON, dumpSQL, exportCSV/JSON/SQL, settings, postInit, init); composite primary keys, UPSERT (`ON CONFLICT DO UPDATE`), INSERT OR REPLACE, RETURNING; window functions ROW_NUMBER/RANK/DENSE_RANK/LAG/LEAD/FIRST_VALUE/LAST_VALUE with OVER and frames; prepared statements; cursors for paging past 30KB; triggers, FOREIGN KEY cascades, CHECK constraints; WITH RECURSIVE, FULL OUTER/NATURAL/USING joins, EXCEPT/INTERSECT, EXPLAIN, VACUUM/REINDEX, SAVEPOINT |
| **Security** | Parameterized queries, TCP LOGIN required by default (fail-closed), loopback listener |
