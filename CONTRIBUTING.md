# Contributing to A3SQL

Thanks for considering contributing! Here's how to get started.

## Quick Start

```bash
git clone https://github.com/lErrorl404l/a3sql.git
cd a3sql
cargo build --release -p a3sql
hemtt build
# See docs/wiki/Development-Setup.md for full setup
```

## Pull Request Process

1. **Small, focused PRs** — one feature or fix per PR. Large changes should be
   discussed in an issue first.
2. **Tests** — run `cargo test --lib -p a3sql` before submitting. New features
   should include tests.
3. **Pre-commit hooks** — the repo has `.githooks/` configured. Run:
   ```bash
   git config core.hooksPath .githooks
   ```
   This checks formatting, clippy, and HEMTT lint before every commit.
4. **Rust code** — follows standard Rust conventions. Run `cargo clippy -p a3sql`
   and `cargo fmt --check` before pushing.
5. **SQF code** — follows ACE3 conventions (tab indents, `PREP()` macros).
   Run `uv run python3 tools/sqf_validator.py addons/` before pushing.

## Code Standards

### Rust
- No `as any` / `@ts-ignore` / `unwrap()` in production code. Use proper error
  handling with `Result` and the `A3sqlError` type.
- All new public functions need doc comments.
- Follow `cargo clippy` — zero warnings.

### SQF
- Tab indentation (4-space visual width).
- Functions go in their own file as `fn_<name>.sqf` with `PREP(<name>)` in
  `XEH_preInit.sqf`.
- Use `TRACE_1`, `TRACE_2`, `ERROR_1` macros instead of raw `diag_log`.
- See ACE3 coding guidelines for full conventions.

### Documentation
- SQL features go in `docs/wiki/SQL-Dialect.md`.
- API changes go in `docs/wiki/Getting-Started.md`.
- PRs that add features should update the relevant docs.

## Development Environment

See [Development Setup](https://github.com/lErrorl404l/a3sql/wiki/Development-Setup)
for full instructions on file patching, code signing, and testing.

## Release Process

Releases are automated via [Release Drafter](.github/release-drafter.yml).
Tags are created manually:
```bash
git tag v0.2.0 && git push origin v0.2.0
```
The CI will build, sign, and publish to Steam Workshop automatically.
