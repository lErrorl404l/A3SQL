# A3DB — Arma 3 Database Engine

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma** — a
Rust `callExtension` that lets modders write SQL directly in SQF.

```sqf
// Create table, insert, query — no boilerplate
["CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING)"] call a3db_fnc_execute;
["INSERT INTO weapons VALUES ('m4a1', 'M4A1')"] call a3db_fnc_execute;
_result = ["SELECT * FROM weapons WHERE name %% 'm4'"] call a3db_fnc_execute;
```

## Status

| Component | Status |
|---|---|
| SQL engine | 118 tests passing |
| CBA addon | HEMTT check clean (0 warnings) |
| Rust linting | Clippy clean (0 warnings) |
| CI/CD | GitHub Actions — test + cross-compile + release |
| Cross-platform | Linux x86_64/i686, Windows x86_64/i686 |
| Security | Parameterized queries (`$1`,`$2`), TCP LOGIN auth, CBA credentials |
| Auto-start | TCP listener starts at game boot (no mission needed) |
| Structure | ACE3/CBA_A3 conventions |

## Quick Start

```sqf
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3db_fnc_execute;
["INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)"] call a3db_fnc_execute;
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3db_fnc_execute;
// Returns: [0, "OK", [["name","score"],["Scarface",1500]]]
```

## Documentation

Full docs in [docs/README.md](docs/README.md) covering:

- SQL dialect reference (all supported syntax)
- SQF API reference (all commands, response format, error codes)
- Advanced features (fuzzy search, window functions, CTEs, transactions)
- Security (parameterized queries, TCP auth, CBA credentials)
- External TCP connector (Python, CLI)
- Building extension + addon
- ACE3 project structure

Worked example in [docs/example.md](docs/example.md).

## Testing

Test without Arma 3:

```bash
cargo test -p a3db --lib   # 118 tests (0.01s)
hemtt check                 # SQF + config validation
```

## Dependencies

- [CBA_A3](https://github.com/CBATeam/CBA_A3) — Community Base Addons
- [Rust](https://rustup.rs/) 1.80+ — build the extension
- [HEMTT](https://hemtt.dev/) 1.20+ — build addon PBOs

## License

Arma 3 Share Alike
