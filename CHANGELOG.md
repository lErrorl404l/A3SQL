# Changelog

## [Unreleased]

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
