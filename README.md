# A3DB — Arma 3 Database Engine

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma** — a
Rust `callExtension` that lets modders write SQL directly in SQF.

## Quick Start

```sqf
// Initialize
private _version = ["a3db", "version"] callExtension;

// Create table + insert
private _r = ["a3db", "CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT)"] callExtension;
private _r = ["a3db", "INSERT INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3)"] callExtension;

// Query with fuzzy match
private _r = ["a3db", "SELECT * FROM weapons WHERE id %% 'rhs_m4'"] callExtension;
```

## Documentation

Full docs in [docs/README.md](docs/README.md) covering:

- SQL dialect reference (CREATE, INSERT, SELECT, UPDATE, DELETE, JOINs, ORDER BY, aggregates, fuzzy match)
- SQF API reference (all commands, response format, error codes)
- Building (Rust extension + HEMTT addon)
- Project structure

## Status

| Component | Status |
|---|---|
| SQL engine | 92 tests passing |
| CBA addon | HEMTT check clean |
| CI/CD | GitHub Actions pipeline |
| Cross-platform | Linux x86_64/i686, Windows x86_64/i686 |
| Structure | ACE3/CBA_A3 conventions |

## Dependencies

- [CBA_A3](https://github.com/CBATeam/CBA_A3) — Community Base Addons (required)
- [Rust](https://rustup.rs/) 1.80+ — for building the extension
- [HEMTT](https://hemtt.dev/) 1.20+ — for building the addon PBOs

## License

MIT
