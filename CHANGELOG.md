# Changelog

## [Unreleased]

## [1.1.0]

Runtime engine generalisation, full CBA integration, and built-in apply functions.

### Added
- Runtime engine: `apply_function` column makes the event-driven override engine domain-agnostic
- Built-in apply functions: applyDamage, applyVelocity, applyWeather, applyAccuracy, applySpeed
- CBA event interface: reload, toggle, query, status, ruleApplied, rulesLoaded, ready events for cross-mod control
- CBA keybinds: Reload Rules (Ctrl+F5), Toggle Engine (Ctrl+F6), Query Status (Ctrl+F7)
- CBA versioning: `VERSIONING` macro registers A3SQL with CBA's version checker
- Binary format v0x03: column flags (auto_increment, not_null, unique, defaults) and `next_auto_inc` persisted across save/load
- Runtime-Engine.md: full architecture doc with schema, CBA events, keybinds, worked examples, and limitations
- Arma 3 modding standard (shared reference for HEMTT, CBA, SQF conventions)

### Changed
- Runtime engine: `fnc_apply.sqf` dispatches to `apply_function` via missionNamespace instead of hardcoded damage/velocity cases
- DB init deferred with `CBA_fnc_waitUntilAndExecute` until mission start (time > 0)
- Master switch (`a3sql_runtime_toggle`) overrides the CBA setting per mission; all handlers gate on it
- requiredAddons: cba_common, cba_events, cba_keybinding, cba_settings declared; defensive version guard in postInit
- CfgPatches names use `COMPONENT_NAME`; COMPONENT_BEAUTIFIED cleaned to un-prefixed names
- Auto-save interval reduced from 30s to 5s for better data durability
- `ColumnNotFoundInTable` error now explains SQL double-quote vs single-quote convention

### Fixed
- Logging: 47 calls to nonexistent `CBA_fnc_info`/`CBA_fnc_error` replaced with CBA `INFO_N`/`ERROR_N` macros (all INFO logging was silently dead)
- Binary format: column `auto_increment`, `not_null`, `unique`, `default`, `default_expr` flags now survive save/load (previously hardcoded to false)
- Binary format: `next_auto_inc` counter now persisted (previously lost on restart)
- Pre-commit hook: clippy grep now matches `^error` only (CBA profile warnings no longer cause false failures)
- `fnc_handleFired.sqf`: removed unused `_log_level` variable, moved Fired from addMissionEventHandler to per-object attachment

## [1.0.0]

First stable release.

### Added
- SQL cheat sheet with copy-paste patterns for schema, DML, joins, aggregates, window functions, CTEs, transactions, and control commands
- Quarto documentation book with dark mode, full-text search, and six-part structure
- SBOM attached to GitHub releases (Def Stan 05-138 supply-chain evidence)
- i686 cross-compile jobs for Linux and Windows

### Changed
- CI deduplicated: removed clippy and cargo test jobs already covered by org reusable workflow
- All CI actions SHA-pinned (supply-chain hardening)
- cargo-deny: added `GPL-2.0` (deprecated form) and `LicenseRef-Arma-Public-License-Share-Alike` to allow list
- cargo-deny: removed broken `[[licenses.clarify]]` entries for hemtt crates
- Documentation reorganised into six parts: Getting Started, SQL Reference, Integration, Networking, Operations, Development

### Fixed
- GitHub Pages deployment now uses GitHub Actions source (not branch deploy)
- Quarto build: removed PDF/epub formats requiring lualatex (HTML only)

## [0.2.0]

### Added
- Plugin system: Rust trait, C ABI dynamic, SQF registration
- RETURNING clause for INSERT/UPDATE/DELETE
- EXPLAIN command (JSON query plan)
- CREATE/DROP VIEW + transparent view resolution
- CHECK and FOREIGN KEY constraint enforcement
- Window frame specs (ROWS BETWEEN)
- EXCEPT/INTERSECT set operations
- FULL OUTER JOIN, NATURAL JOIN, JOIN USING
- COUNT(DISTINCT col)
- VACUUM / REINDEX commands
- CLI interactive REPL mode (`a3sql-server --interactive`)
- Graceful shutdown with auto-save on SIGTERM
- Full-text search via trigram index
- SQL compatibility corpus (7 SQL files, 150+ statements)

### Changed
- Database renamed from a3db to a3sql
- Standalone server now shares full code path with extension (PING, LOGIN, etc.)
- Pre-commit hooks enforce clippy + fmt + HEMTT check
- CI validates SQF syntax, config style, BOM, and runs CodeQL
- Dependabot configured for weekly Cargo + Actions updates

### Fixed
- ROLLBACK is no-op when no transaction is active (matches PostgreSQL)
- BOOL type supports both `BOOL` and `BOOLEAN` keywords
- All custom commands are case-insensitive
- has_aggregate() detects aggregates inside ExprWithAlias
- SQF fn_execute now passes `$1`/`$2` params through callExtension
- SQL injection: removed unsafe "already quoted" bypass in substitute_params
