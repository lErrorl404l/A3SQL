# Changelog

Steam Workshop change notes (BBCode) live in `workshop-changelog.bbcode`.
Keep both files in sync when updating this changelog.

## [Unreleased]

## [1.1.0]

Runtime engine generalisation, full CBA integration, built-in apply functions, and SQL-compatibility fixes verified in a live Arma 3 dedicated server.

### Added
- Runtime engine: `apply_function` column makes the event-driven override engine domain-agnostic
- Built-in apply functions: applyDamage, applyVelocity, applyWeather, applyAccuracy, applySpeed
- CBA event interface: reload, toggle, query, status, ruleApplied, rulesLoaded, ready events for cross-mod control
- CBA keybinds: Reload Rules (Ctrl+F5), Toggle Engine (Ctrl+F6), Query Status (Ctrl+F7)
- CBA versioning: `VERSIONING` macro registers A3SQL with CBA's version checker
- Binary format v0x03: column flags (auto_increment, not_null, unique, defaults) and `next_auto_inc` persisted across save/load
- `NULLIF(a, b)` builtin: returns NULL when the two arguments are equal, else the first
- `match_search(haystack, needle)` builtin: case-insensitive substring test backing the `MATCH` operator
- `REGEXP` operator (`Expr::RLike`): wildcard pattern matching (`*` any sequence, `?` single char)
- `CEIL()` / `FLOOR()`: sqlparser emits dedicated AST nodes for these; the evaluator now handles them (previously fell through as unknown functions)
- `CREATE TABLE AS SELECT` (CTAS): the handler existed but was never wired into statement dispatch
- `FILTER (WHERE ...)` aggregate clause: `A3sqlDialect` now declares `supports_filter_during_aggregation`, so `COUNT(*) FILTER (WHERE ...)` parses
- sqllogictest harness: engine implements the `sqllogictest::DB` trait (same harness as DuckDB/DataFusion); corpora in `extension/tests/slt/` cover core CRUD, type behaviour, grouping, views, transactions, and constraints
- SQLite differential tests: 187 lines of side-by-side behaviour checks against a real SQLite engine
- Math verification suite (`math_verify`): ABS, CEIL, FLOOR, ROUND, POW, SQRT, SIGN against known-good reference values
- Runtime-Engine.qmd: full architecture doc with schema, CBA events, keybinds, worked examples, and limitations
- Runtime-Engine.qmd: testing section documenting the sqllogictest, differential, and math-verification harnesses
- Arma 3 modding standard (shared reference for HEMTT, CBA, SQF conventions)

### Changed
- Runtime engine: `fnc_apply.sqf` dispatches to `apply_function` via missionNamespace instead of hardcoded damage/velocity cases
- DB init deferred with `CBA_fnc_waitUntilAndExecute` until mission start (time > 0)
- Master switch (`a3sql_runtime_toggle`) overrides the CBA setting per mission; all handlers gate on it
- requiredAddons: cba_common, cba_events, cba_keybinding, cba_settings declared; defensive version guard in postInit
- CfgPatches names use `COMPONENT_NAME`; COMPONENT_BEAUTIFIED cleaned to un-prefixed names
- Auto-save interval reduced from 30s to 5s for better data durability
- `ColumnNotFoundInTable` error now explains SQL double-quote vs single-quote convention
- `MATCH` operator rewritten to `match_search(left, right)` in the SQL preprocessor (sqlparser's GenericDialect has no infix `MATCH`)

### Fixed
- Logging: 47 calls to nonexistent `CBA_fnc_info`/`CBA_fnc_error` replaced with CBA `INFO_N`/`ERROR_N` macros (all INFO logging was silently dead)
- Binary format: column `auto_increment`, `not_null`, `unique`, `default`, `default_expr` flags now survive save/load (previously hardcoded to false)
- Binary format: `next_auto_inc` counter now persisted (previously lost on restart)
- Pre-commit hook: clippy grep now matches `^error` only (CBA profile warnings no longer cause false failures)
- `fnc_handleFired.sqf`: removed unused `_log_level` variable, moved Fired from addMissionEventHandler to per-object attachment
- `MIN` / `MAX` aggregates now ignore NULL values (NULL previously compared as greater than every number via the string fallback, so `MAX` could return NULL)
- `REGEXP` returned a parse error for every query because sqlparser 0.62 parses it as `Expr::RLike`, not a binary operator; the evaluator now handles `Expr::RLike`
- Boolean-typed expressions (comparisons, AND/OR/NOT, REGEXP, MATCH, EXISTS) previously surfaced as integers; SLT expectations corrected to `true`/`false`
- Docker smoke test: extension is built in a Debian bookworm container so the `.so` links against GLIBC ≤ 2.36 and actually loads in the Arma 3 server container (a host build requiring GLIBC 2.39 silently failed to load, turning every smoke assertion into a false pass)
- Docker smoke test: corrected armake flags, container entrypoint, CBA mod download/layout, mission pack path, and server-root mount; profiles dir is now tmpfs so the test leaves no root-owned files behind
- Docker smoke test: `DESCRIBE x` (not `DESCRIBE TABLE x`), MERGE with table-source form, removed `RELEASE SAVEPOINT` (consumed by `ROLLBACK TO`), and a genuinely invalid syntax-error probe

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
