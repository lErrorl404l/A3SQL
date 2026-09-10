# Changelog

## [Unreleased]

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
