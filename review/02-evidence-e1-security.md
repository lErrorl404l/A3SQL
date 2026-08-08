# Evidence Record E1 — Security and FFI (F-01, F-05, F-07, F-10, F-11)

Task T3a of the approved review plan. Pinned baseline: `585a46079d7b64e4cdf3d1c9a859d710602a91b9`
(review/00-snapshot.md, 2026-08-05). Collected 2026-08-08 at HEAD 5e5e73c
(only review/ commits on top of the pin; `git diff 585a460..HEAD --stat`
shows review/ files only). No source file modified; no cargo build/clippy
run; no Arma harness attempted.

Quoting policy (uk-code-licensing fair-dealing, CDPA s.29/30): repo lines are
quoted as short factual attributions only. Brief requirements are
paraphrased with section references; no verbatim policy reprints.

## F-01 — Panic across the C ABI (HIGH)

Evidence commands (exit codes recorded):

```
$ grep -c 'catch_unwind' extension/src/ffi/*.rs
extension/src/ffi/dir.rs:0
extension/src/ffi/tests.rs:0

$ grep -n 'catch_unwind' extension/src/server.rs extension/src/bin/a3sql-server.rs
extension/src/server.rs:110:        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
extension/src/bin/a3sql-server.rs:159:        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a3sql::dispatch(trimmed, &[])))
```

Artifact facts (all at 585a460, blob `646e59cfaf74e3add74e7864f0e72aca4e9dce6a` for
`extension/src/ffi/dir.rs`):

- Four `#[unsafe(no_mangle)] pub unsafe extern "C"` entry points in
  `extension/src/ffi/dir.rs` — the plan cited 145/241/277; the actual
  declarations are at **:164** (`RVExtensionVersion`), **:241**
  (`RVExtension`), **:277** (`RVExtensionArgs`), plus **:342**
  (`RVExtensionRegisterCallback`). Line 145 is `sql_handler` (the first
  FFI-reachable unwrap, `DB.lock().unwrap_or_else` at :146). None of the
  four entry points wraps its body in `catch_unwind`.
- `catch_unwind` exists only in the TCP server paths: `server.rs:110`
  (serve_client) and `a3sql-server.rs:159` (standalone binary dispatch).
  Those paths are not the C ABI surface.
- Unwrap/expect density in FFI-reachable paths: `engine/sqf/eval.rs` **72**
  matching lines (plan expected ~71), `engine/serialize.rs` **24**,
  `engine/trigger.rs` **16**.
- Panic profile: `extension/Cargo.toml:96-100` `[profile.release]` sets
  `opt-level = "z"`, `lto`, `codegen-units = 1`, `strip` — **no `panic`
  setting**, so the default `panic = "unwind"` applies and `catch_unwind`
  IS meaningful under the shipped profile. The finding is NOT downgraded by
  a `panic = "abort"` profile; no `panic = "abort"` exists anywhere in the
  manifest.
- Defence-in-depth already present: `RVExtensionArgs` null-checks `argv` on
  the arma-rs path and returns `-1` instead of panicking (dir.rs:286-290,
  comment: "would panic across the FFI boundary"), with a regression test
  (`extension/tests/ffi.rs:398-404`, `ffi_args_null_output_returns_minus_one`).
  This fixes one specific panic vector but does not bound the eval path.

Record (7-field):

```
F-01 | extension/src/ffi/dir.rs:164/241/277/342 @ 585a460
      (blob 646e59c); unwrap density engine/sqf/eval.rs:72,
      engine/serialize.rs:24, engine/trigger.rs:16; catch_unwind only at
      server.rs:110, a3sql-server.rs:159 | harm: HIGH — availability.
      A SQF-triggered panic in the eval path unwinds through an
      `extern "C"` frame with no `catch_unwind` barrier; unwinding across a
      non-unwind ABI is UB and in practice aborts the host game process
      (Arma). Not memory unsafety: the abort is the failure mode, so the
      game server dies, not the engine memory. Reachable from untrusted
      mission input; 72 unwraps in eval.rs plus arma-rs internals | fix:
      wrap each unbarriered entry body in `catch_unwind(AssertUnwindSafe)`
      returning an error envelope on panic; add
      `#![deny(clippy::unwrap_used)]` on FFI-reachable modules or justify
      each unwrap; keep `panic = "unwind"` (already default) so the
      barrier is meaningful | clause: gap-rider (corpus gap #1 — FFI/unsafe
      boundary, no governing brief) with ncsc-secure-development P3
      (design-in security) as secondary | OPEN, lead, 2026-08-08 | v1.0
```

## F-05 — Org reusable workflow pinned to @main (MED)

Evidence commands (exit codes recorded):

```
$ sed -n '15,25p' .github/workflows/ci.yml
  RUSTFLAGS: -D warnings

jobs:
  # ── Core pipeline (org-wide reusable) ───────────────────────────────
  ci:
    uses: lErrorl404l/.github/.github/workflows/rust-addon-ci.yml@main
    with:
      crate-name: a3sql
      crate-path: extension
      rust-cross: true
```

Artifact facts (all at 585a460, blob `0f0f49997b19ee7b8dd77b2868b71ebbaaeeea1b`):

- `.github/workflows/ci.yml:20` — `uses: lErrorl404l/.github/.github/
  workflows/rust-addon-ci.yml@main` (movable ref to a private org repo).
  The plan's check (exit 0 if @main confirmed) is satisfied.
- Direct actions in the same file ARE SHA-pinned: `actions/checkout@
  3d3c42e5aac5ba805825da76410c181273ba90b1` (v7.0.1, lines 31/56/82/147/193),
  `actions-rust-lang/setup-rust-toolchain@166cdcfd11aee3cb47222f9ddb555ce30ddb9659`
  (v1, lines 33/58), `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`
  (v7, lines 46/71), `actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c`
  (v8, line 94), `arma-actions/hemtt@a0570ddd80d2b7be9c0f052494c0f700b42668dd`
  (v1, lines 86/151/197), `softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228`
  (v3.0.2, line 182).
- Two major-tag refs remain (low risk): `actions/cache@v6` (line 88) and
  `actions/upload-artifact@v7` (line 135).

Record (7-field):

```
F-05 | .github/workflows/ci.yml:20 @ 585a460 (blob 0f0f499)
      `uses: lErrorl404l/.github/.github/workflows/rust-addon-ci.yml@main`
      | harm: MED — build pipeline integrity. The core CI pipeline is a
      movable ref into an opaque private org repo; a @main change there
      silently alters how every a3sql build runs (toolchain, signing
      steps, artifact assembly) with no reviewable diff in this repo.
      Direct actions are SHA-pinned (mitigating), but the org workflow
      governs the pipeline itself | fix: pin the org workflow to a full
      commit SHA (`@<40-hex>`) and update it deliberately; audit the org
      repo's history once (bundle section 5 waived item) | clause:
      ncsc-secure-development P6 (pin build pipeline) — gap-rider not
      needed, the brief clause covers it | OPEN, lead, 2026-08-08 | v1.0
```

## F-07 — Plugin dlopen without integrity gate (MED)

Evidence commands (exit codes recorded):

```
$ sed -n '225,253p' extension/src/engine/plugin.rs
fn load_plugin_file(path: &str) -> Result<String, EngineError> {
    // Safety: libloading is safe — the plugin is a shared lib we control.
    // The plugin C ABI must match a3sql_plugin.h.
    unsafe {
        let lib = libloading::Library::new(path).map_err(|e| EngineError::Exec(format!("dlopen: {}", e)))?;
        ...
        // The library handle is leaked intentionally — plugins live for the process lifetime.
        std::mem::forget(lib);
```

Artifact facts (all at 585a460, blob `da6daa3a40ffab20137bb66c71a59859e1584c75`):

- `extension/src/engine/plugin.rs:230` — `libloading::Library::new(path)`
  (dlopen) with no hash or signature check before loading; the whole
  `load_plugin_file` (:225-253) has no integrity gate.
- `plugin.rs:248` — `std::mem::forget(lib)` intentional leak, commented
  "plugins live for the process lifetime" (deliberate, documented).
- **Plan correction**: the plan described the hooks as `#[allow(dead_code)]`
  scaffold "not yet wired". That claim does NOT hold at the pin: no
  `#[allow(dead_code)]` exists in plugin.rs, and the plugin path IS wired —
  `dispatch.rs:197` routes `register_function` and `dispatch.rs:207` routes
  `plugin_dir <dir>` to `load_plugin_dir`, reachable from SQF
  `callExtension`. The sink is live, not a stub; its reachability narrows
  only on who can issue the `plugin_dir` command (mission-provided SQF on a
  server the admin runs). Severity MED stands.

Record (7-field):

```
F-07 | extension/src/engine/plugin.rs:230/248 @ 585a460 (blob da6daa3);
      reachable via dispatch.rs:197/207 | harm: MED — code-execution sink.
      `plugin_dir <dir>` dlopens every .so in a caller-supplied directory
      with no hash/signature gate; if that path is attacker-writable
      (untrusted mission SQF invoking callExtension "plugin_dir" on a
      writable dir), arbitrary native code executes in the game server
      process. Today the default setup does not enable plugins, so
      exploitation requires the plugin_dir command to be invoked; the
      gate is absent, not just dormant | fix: hash or signature gate before
      dlopen (allowlist of known plugin hashes, or .bikey signature), and
      document the trust boundary (plugin_dir is code execution, server
      owner's responsibility) | clause: ncsc-supply-chain-security
      third-party code clause (verify third-party components before use);
      ncsc-secure-development P3 secondary | OPEN, lead, 2026-08-08 | v1.0
```

## F-10 — openssl-sys vendored and dead (LOW)

Evidence commands (exit codes recorded):

```
$ sed -n '78,94p' extension/Cargo.toml
[package.metadata.cargo-machete]
ignored = [
    "hemtt-tokens",
    "indexmap",
    "openssl-sys",
    ...
]

$ grep -rn 'openssl' extension/src/ extension/tests/   # → 0 hits
```

Artifact facts (all at 585a460, blob `f784f51b4fbb756ad06a16711d50a58a4f301d6c`):

- `extension/Cargo.toml:58` — `openssl-sys = { version = "0.9", features =
  ["vendored"] }` is a direct dependency, present solely for "reliable
  cross-compilation (i686, Windows)" per its comment.
- `extension/Cargo.toml:87` — `"openssl-sys"` sits in the
  `[package.metadata.cargo-machete] ignored` allowlist (:83-94). Machete
  flags it as unused; the suppression hides that.
- Zero references to openssl anywhere in `extension/src/` or
  `extension/tests/` at the pin; the lockfile still carries `openssl-sys`,
  `openssl-src`, `openssl-probe` (Cargo.lock:1232-1247).

Record (7-field):

```
F-10 | extension/Cargo.toml:58 + :87 (machete ignored) @ 585a460
      (blob f784f51) | harm: LOW — dead C supply-chain surface + binary
      bloat. A vendored C library (compiles its own OpenSSL C sources)
      with zero Rust references is shipped into all 4 extension targets,
      inflating the DLLs and carrying a C dependency that is not
      exercised or reviewed; the machete allowlist suppresses the unused-
      dependency signal instead of the dependency | fix: remove
      `openssl-sys` from Cargo.toml:58 and drop it from the machete
      ignored list; if a future target genuinely needs vendored TLS,
      re-add with justification | clause: ncsc-secure-development P3
      (minimise attack surface) — gap-rider not needed | OPEN, lead,
      2026-08-08 | v1.0
```

## F-11 (FFI part) — Coverage-gap evidence (LOW-MED)

This is a coverage-gap record, NOT a live-harness finding. Three sub-gaps:

1. **Header vs exports never diffed.** `include/a3sql_plugin.h` (1807 B,
   blob `6d5acdf621f3e8cab833a4023d5bb129854eaacc`) declares the C ABI
   contract (a3sql_plugin_init, callbacks); the `#[unsafe(no_mangle)]`
   exports in `ffi/dir.rs` are never mechanically diffed against it.
   `repr(C)` layout, calling convention (cdecl vs stdcall on 32-bit
   Windows — see dir.rs:8-11 `--kill-at` note), and string marshalling
   across the two sides are unverified by any test or tool.
2. **Fuzz deliberately skips custom commands.** `extension/src/fuzz.rs:
   207-233` `is_custom_command` filters out ping/reset/save/load/
   listen/connect/plugin_dir/register_function/set_credentials/… before
   they reach the dispatcher, explicitly so the fuzzer stays on the SQL
   path. Fuzz coverage = SQL parse→execute only; the FFI, TCP auth, and
   plugin surfaces are not fuzzed.
3. **Behavioural DLL-load panic test is OBJ-unrun.** The hyperplan F-11
   evidence check (load the DLL, invoke a panicking export, assert no
   process abort) requires an Arma runtime harness. It was NOT run in this
   task (per plan: do not attempt to run Arma). Recorded as a scheduled
   follow-up, owner: lead, target: next release window.

Record (7-field):

```
F-11 | include/a3sql_plugin.h @ 585a460 (blob 6d5acdf) vs ffi/dir.rs
      exports (blob 646e59c); fuzz.rs:207-233 custom-command skip; the
      behavioural DLL panic test (unrun) | harm: LOW-MED — verification
      gap. The public C ABI contract is not mechanically checked against
      the implementation (calling convention, layout, marshalling);
      fuzzing does not reach the FFI, auth, or plugin surfaces; the one
      behavioural panic test that would prove F-01's harm end-to-end is
      unexecuted | fix: (1) a diff/snapshot check binding a3sql_plugin.h
      declarations to the no_mangle exports; (2) extend fuzz targets to
      the dispatch custom-command path in a side-effect-free harness;
      (3) schedule the DLL-load panic test on an Arma CI runner | clause:
      gap-rider (corpus gap #1 — FFI/unsafe boundary) | OPEN, lead,
      2026-08-08 | v1.0
```

## Exculpatory defensive facts (verified at the pin)

These keep the findings above honest; each verified at 585a460 with a
line-range.

1. **rkyv magic gate.** `extension/src/engine/serialize/binary.rs:26`
   `const BINARY_MAGIC: &[u8; 4] = b"A3SQ"`; `:170-176` rejects short data
   (`data.len() < 5 + CHECKSUM_LEN`), wrong magic, and wrong format
   version BEFORE any rkyv parse. Serialized input cannot reach rkyv
   without passing the magic gate. (Blob `1a53733678f0bce8ce13442f049af6f6abd7deee`.)
2. **Constant-time credential compare.** `extension/src/server.rs:25-32`
   `ct_eq` folds the length difference into the accumulator and XORs
   `max(len)` bytes; the comment documents the residual length leak as
   acceptable (credential length is already visible on the wire). (Blob
   `4a4d2a500c2fcbdfbdba69c15d94c9418089e2ce`.)
3. **Fail-closed listener default.** `extension/src/config.rs:50`
   `listener_auth_required` returns `self.listener_require_auth.unwrap_or(true)`
   — auth on by default. (Blob `d6c72e502b191d071e9c351cd6b8e32fd0ebb67f`.)
4. **TOCTOU fix.** `extension/src/dispatch/commands/io.rs:171-182`
   `write_no_follow` uses `O_NOFOLLOW` for save writes; symlink-escape
   tests at `io.rs:427-464` (`rejects_final_symlink_escaping_data_dir`,
   `rejects_subdir_symlink_escaping_data_dir`); crash-window durability
   tests at `io.rs:567-607` (FNV-1a trailer, bit-flip rejection at
   offsets [5, 6, 12, payload_end-1]). (Blob
   `9e8df0bb02feea0e4ae12e0e9c3318b4ba955b02`.)
5. **Miri CI filter excludes FFI deliberately.** `.github/workflows/
   test.yml:39` runs miri on `--lib -- auth engine::serialize engine::index
   engine::functions::eval::test dispatch server engine::trigger` only; the
   FFI surface is excluded because miri cannot run `extern "C"` (comment at
   test.yml:34-38 states this explicitly).
6. **FFI exercised in-process.** `extension/tests/ffi.rs` — 21 `#[test]`
   entries exercising RVExtensionVersion/RVExtension/RVExtensionArgs with
   real C pointers (null-pointer, small-buffer, overflow, utf-8, paging
   cases); `extension/tests/abi.rs` — 28 dispatch-level tests;
   `extension/src/ffi/tests.rs` — 25 unit tests needing private access to
   `write_output`/`CALLBACK`. This does not close F-01 (in-process tests
   cannot prove the real Arma host survives a panic), but it bounds the
   FFI's behavioural surface.

## Accounting

| Finding | Status | Owner | Date | Version |
|---------|--------|-------|------|---------|
| F-01 | OPEN | lead | 2026-08-08 | v1.0 |
| F-05 | OPEN | lead | 2026-08-08 | v1.0 |
| F-07 | OPEN | lead | 2026-08-08 | v1.0 |
| F-10 | OPEN | lead | 2026-08-08 | v1.0 |
| F-11 (FFI) | OPEN | lead | 2026-08-08 | v1.0 |

Plan deltas recorded honestly: F-01 ABI entry lines are 164/241/277/342
(not 145/241/277; :145 is `sql_handler`); eval.rs unwrap lines are 72 (plan
~71); F-07's plugin path is wired, not stub scaffold, and no
`#[allow(dead_code)]` exists on it; the release profile has NO
`panic = "abort"`, so `catch_unwind` is meaningful and the F-01 harm
stands as written.
