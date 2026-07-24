# A3DB — Arma 3 Database Engine

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma**.

## Quick Start

```sqf
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3db_fnc_execute;
["INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)"] call a3db_fnc_execute;
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3db_fnc_execute;
```

## Wiki Pages

- **[Getting Started](Getting-Started)** — Full worked example
- **[SQL Dialect](SQL-Dialect)** — Supported SQL syntax
- **[CBA Settings](CBA-Settings)** — Addon Configuration options
- **[Security](Security)** — Parameterized queries, TCP authentication
- **[TCP Connector](TCP-Connector)** — External query access
- **[Standalone Server](Standalone-Server)** — Run without Arma, remote mode
- **[Building](Building)** — Compiling the extension and addon

## Status

| Component | Status |
|---|---|
| SQL engine | 118 tests passing |
| CBA addon | HEMTT check clean |
| Linting | Clippy clean |
| CI/CD | GitHub Actions |
| Cross-platform | Linux x86_64/i686, Windows x86_64/i686 |
| Security | Parameterized queries, TCP LOGIN |
