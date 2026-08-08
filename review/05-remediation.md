# Remediation Record v1.3: Rules-Compliance Review of a3db/a3sql

## Document control

- Report: remediation loop for the rules-compliance review of the a3db/a3sql repository
- Version: v1.3
- Date: 2026-08-08
- Classification: OFFICIAL
- Review owner: lead
- Configuration management baseline: 585a46079d7b64e4cdf3d1c9a859d710602a91b9
- Status: all OPEN findings in review/04-findings-report.md v1.0 are now FIXED, WAIVED, or recorded no-action

Classification note. This record is OFFICIAL, the default marking for a
public repository. It holds no supplier-commercial detail and no
pre-release vulnerability detail.

Quoting policy. Repository lines appear only as short factual attributions
with file, line, and commit reference.

Status transitions. Every transition was re-verified against the pinned
baseline SHA 585a460 with date 2026-08-08. Test-first evidence means a
failing test reproduced the defect before the fix and passed after the fix.

## Per-finding records

### F-01 HIGH — panic-across-ABI

- Action: FIXED
- Test-first evidence:
  - Test: `ffi_string_panic_returns_error_envelope_not_abort` and
    `ffi_args_panic_returns_minus_one_and_envelope` in extension/tests/ffi.rs.
  - Before: `SELECT SQF_EVAL('1/0')` through the extern "C" entry aborted the
    test process (`thread caused non-unwinding panic. aborting.`, signal 6,
    SIGABRT). The panic unwound across the unbarriered ABI frame.
  - After: both tests pass. The entry returns a fixed `[-1,"ERR_INTERNAL",
    "Command failed"]` envelope and the process survives.
- Fix: wrap all four extern "C" entries (RVExtension, RVExtensionArgs,
  RVExtensionVersion, RVExtensionRegisterCallback) in
  `catch_unwind(AssertUnwindSafe)`. Deny `clippy::unwrap_used` in non-test
  builds of the ffi module (`#![cfg_attr(not(test), deny(clippy::unwrap_used))]`)
  and remove the remaining unwraps there (with_extension slot access and the
  CALLBACK lock). Other modules, including eval.rs, are untouched.
- RE-VERIFY: full `cargo test` passes (517 lib tests plus 13 integration
  binaries). `RUSTFLAGS="-D warnings" cargo clippy --all-targets` reports
  zero warnings. `grep -c catch_unwind extension/src/ffi/dir.rs` = 4 barrier
  calls. `grep -n "unwrap()" extension/src/ffi/dir.rs` (non-comment) = 0.
- Commit: 3924869 (`fix(ffi): catch panics at the C ABI boundary (F-01)`)
- Status: OPEN to FIXED, 2026-08-08

### F-02 MED — licence mismatch

- Action: FIXED
- Test-first evidence: no runtime test applies to a manifest field. The
  acceptance command is the gate: `cargo metadata` and `cargo build`.
- Before: `license = "MIT OR Apache-2.0"` while LICENSE carries APL-SA.
- After: `license = "LicenseRef-Arma-Public-License-Share-Alike"` with a
  documenting comment. `cargo metadata --manifest-path extension/Cargo.toml`
  exits 0 and reports the value. `cargo build` passes.
- Fix: set the truthful LicenseRef value (APL-SA has no SPDX identifier;
  LicenseRef is the accepted cargo form for unregistered licences). The
  LICENSE file ships with the crate. deny.toml is unaffected: its allow-list
  governs third-party dependencies, not this crate.
- RE-VERIFY: `cargo metadata` exit 0, `cargo build` exit 0.
- Commit: 692b286 (`build: make Cargo.toml license field truthful (F-02)`)
- Status: OPEN to FIXED, 2026-08-08

### F-03 MED — no SBOM, empty keys/

- Action: WAIVED (tooling not available; org action recorded)
- Evidence: `cargo cyclonedx --version` fails (`no such command`).
  `cargo-cyclonedx` is not installed. `keys/` is empty (0 files). `git tag -l`
  = 0. Installing cargo-cyclonedx requires a network fetch and a long
  compile; that is out of scope for this review loop.
- Justification: SBOM tooling is unavailable in this environment. The
  reproducibility half of the finding is served by F-04: the tracked
  Cargo.lock pins the dependency graph, so the build input set is
  reproducible and verifiable. Generating an SPDX SBOM at each tag and
  shipping the .bikey are release-time actions that need the org toolchain.
- Organisational follow-up: install cargo-cyclonedx, add an SBOM-at-tag CI
  step in .github/workflows, and ship the .bikey at release (hemtt sign with
  a release key).
- Commit: none.
- Status: OPEN to WAIVED, 2026-08-08

### F-04 MED — Cargo.lock untracked

- Action: FIXED
- Test-first evidence: the acceptance command is the gate.
  `git ls-files | grep -c Cargo.lock` = 0 before, 1 after.
- Fix: track extension/Cargo.lock. The lock content is unchanged. The root
  Cargo.lock stays untracked: it is a stale duplicate with no root
  Cargo.toml manifest, so it corresponds to no build and tracking it would
  add a second, misleading lockfile.
- RE-VERIFY: `git ls-files | grep -c Cargo.lock` = 1.
- Commit: 9386354 (`build(deps): track Cargo.lock for reproducible builds (F-04)`)
- Status: OPEN to FIXED, 2026-08-08

### F-05 MED — org reusable workflow at main

- Action: WAIVED
- Evidence: ci.yml:20 `uses: lErrorl404l/.github/.github/workflows/
  rust-addon-ci.yml@main`. The org repository is private, and its commit SHA
  cannot be resolved from this environment.
- Justification: pinning the reusable workflow to a full commit SHA requires
  org-level access that a checkout does not have. This is an owner-level
  follow-up outside the review. ci.yml is deliberately unchanged.
- Owner: org admin.
- Commit: none.
- Status: OPEN to WAIVED, 2026-08-08

### F-06 MED — auth feature dormant

- Action: FIXED (dead code removed, per maintainer decision)
- Test-first evidence:
  - The feature was optional (`auth = ["ed25519-dalek"]` NOT in
    `default = ["sqf-preprocessor"]`) and its implementation carried
    `#[allow(dead_code, reason = "phased auth implementation")]` markers,
    so the code compiled out of every shipped build.
  - Before: `auth` feature + `ed25519-dalek` dependency present;
    `extension/src/auth.rs` (284 lines) and config plumbing compiled out.
  - After: feature, dependency, module, and gating removed; the shipped TCP
    LOGIN path is untouched. `cargo clippy --all-targets -- -D warnings` and
    `cargo machete` are clean; the full suite (504 lib + integration tests,
    including auth_default.rs fail-closed LOGIN and tcp_stress.rs) passes.
- Fix: remove the dormant feature and its implementation rather than enable
  it — a product decision by the maintainer. Delete auth.rs and its module
  registration, drop `auth` and `ed25519-dalek` from Cargo.toml (lockfile
  regenerated), strip the auth fields and `auth_enabled()`/`public_key_bytes()`
  from config.rs, and remove the `cfg(feature = "auth")` verify_auth branch
  and call-site preamble from dispatch.rs. The shipped LOGIN auth (ct_eq
  constant-time compare in server.rs) and `listener_require_auth` fail-closed
  default are unaffected.
- RE-VERIFY: full `cargo test` passes; clippy -D warnings clean.
- Commit: cd88878 (`build: remove dormant auth feature and ed25519 dependency (F-06)`)
- Status: WAIVED to FIXED, 2026-08-08

### F-07 MED — plugin dlopen no integrity check

- Action: FIXED (minimal gate; full gate scheduled)
- Test-first evidence:
  - Test: `plugin_dir_rejects_symlinks_before_dlopen` in extension/tests/plugins.rs.
    The fixture compiles a one-file cdylib with the local rustc and links
    the loadable plugin to a symlink named evil.so.
  - Before: `plugin_dir` returned `[0,"OK",["gate_test_plugin","gate_test_plugin"]]`
    — the symlink was dlopened alongside the regular file.
  - After: the regular file loads once; the symlink is rejected.
- Fix: reject non-regular files before dlopen in load_plugin_dir, using
  `DirEntry::file_type` (which does not follow symlinks). Document the trust
  boundary at the dlopen site: plugins are native code trusted like the
  mission; a hash or signature gate is required before plugin_dir can point
  at user-writable directories.
- RE-VERIFY: full `cargo test` passes; clippy -D warnings clean.
- Commit: a95745b (`fix(plugins): reject non-regular files before dlopen (F-07)`)
- Status: OPEN to FIXED, 2026-08-08

### F-08 MED — unsigned history

- Action: WAIVED
- Evidence: recount at the pinned baseline:
  `git log 585a460 --format='%G? %an' | sort | uniq -c` gives 218 N, 72 G,
  3 E of 293 commits. 74% of history is unsigned (N). The 3 E entries are
  dependabot commits whose key is not in the local ring — that is
  key-not-in-ring, not an unsigned commit (honest framing from the review
  evidence).
- Justification: enforcement of commit.gpgsign requires a GPG key, and no
  key exists in this environment. The maintainer should enable
  commit.gpgsign once a key is created. No non-repudiation change is
  possible from a checkout.
- Owner: maintainer.
- Commit: none.
- Status: OPEN to WAIVED, 2026-08-08

### F-09 LOW — no CODEOWNERS

- Action: FIXED
- Evidence: .github/CODEOWNERS absent before; added after.
- Fix: add .github/CODEOWNERS naming the sole maintainer (Matthew Barker,
  @lErrorl404l) as owner of the sensitive paths: extension/src/ffi/,
  .github/workflows/, SECURITY.md. Minimal and factual.
- RE-VERIFY: `test -f .github/CODEOWNERS` = true.
- Commit: 004df77 (`docs: add CODEOWNERS for sensitive paths (F-09)`)
- Status: OPEN to FIXED, 2026-08-08

### F-10 LOW — dead openssl-sys dependency

- Action: FIXED
- Test-first evidence: no runtime test applies to a dependency removal. The
  acceptance commands are the gates.
- Before: openssl-sys at Cargo.toml:58 referenced by no source file and
  hidden by the machete-ignore entry at Cargo.toml:87.
- After: dependency and ignore entry removed. `cargo machete` v0.9.2 (the CI
  version) reports "didn't find any unused dependencies". Full `cargo test`
  passes. Clippy -D warnings clean.
- Fix: remove the dead vendored C dependency and its machete-ignore entry.
- RE-VERIFY: `cargo machete` exit 0, `cargo test` all green.
- Commit: 5b950ce (`build: drop unused openssl-sys dependency (F-10)`)
- Status: OPEN to FIXED, 2026-08-08

### F-11 LOW-MED — FFI coverage gap

- Action: FIXED
- Test-first evidence:
  - Contract gate: `loader_init_matches_header_signature` in
    extension/tests/plugins.rs. Before: the loader resolved the entry point
    as `fn(*mut c_void) -> *const c_char` and called it with a null context
    argument, while the header declared `const char* (*)(void)`. The gate is
    a mechanical source-level check because the violation is runtime-benign
    on the supported ABIs (the callee ignores the extra argument); it failed
    against the pre-fix loader and passes now that the loader resolves
    `fn() -> *const c_char`.
  - Loader round-trip: the F-07 cdylib fixture in extension/tests/plugins.rs
    now declares the exact published header signature
    `a3sql_plugin_init(void)` and loads through plugin_dir. Before: the
    fixture declared the drifted `fn(*mut c_void)` form. After: the
    header-consistent plugin loads exactly once through the real loader.
  - Fuzzer: `custom_commands_never_panic` (proptest) and
    `custom_commands_never_crash` (fixed regression list) in
    extension/src/fuzz.rs. Before: fuzz.rs:207-233 skipped the entire
    custom-command surface. After: the pure in-memory command handlers run
    through the dispatch path, and the fixed list covers every safe command
    shape deterministically.
- Before: header-versus-exports diff, include/a3sql_plugin.h against the
  no_mangle exports at extension/src/ffi/dir.rs:181/263/315/401 and
  extension/src/engine/plugin.rs:275:
  - `a3sql_plugin_register_function`: signature matches. Header declares
    `int32_t (const char*, const char*, int32_t, int32_t)`; the export is
    `extern "C" fn(*const c_char, *const c_char, i32, i32) -> i32`. Calling
    convention matches (C default, cdecl on 32-bit Windows). Param name
    differs cosmetically (function_name versus func_name).
  - `a3sql_plugin_init`: signature mismatch. The header declares
    `const char* (*)(void)` (no arguments); the loader resolved
    `fn(*mut c_void) -> *const c_char` and called it with one null context
    argument. This is benign on the supported ABIs (the callee ignores the
    extra argument) but it is a contract violation against the header that
    plugin authors compile against.
  - Doc drift: the header said plugins are "loaded at startup"; no startup
    directory scan exists, loading happens through the plugin_dir command
    only. The header said function names are "prefixed with fn_"; the C ABI
    registrar stores the name verbatim and the fn_ prefix is applied at the
    SQL call surface.
  - No repr(C) structs cross the boundary; the two function signatures are
    the entire contract, so repr(C) is not in play.
- After: the header, the loader, and the test fixture all declare the
  no-argument entry point; the header documents the plugin_dir load path and
  the fn_ call surface accurately; the fuzzer covers the safe custom-command
  surface. Side-effecting commands (connect, listen, stop, save, load,
  export_to_file, plugin_dir) remain excluded from the fuzzer by design
  because they open TCP, spawn threads, write files, or dlopen libraries.
- Scheduled follow-up (recorded, not part of this round): run the behavioural
  DLL panic harness test on an Arma CI runner (the F-01 barrier is proven
  in-process; the real-binary harness remains a scheduled follow-up).
- RE-VERIFY: full `cargo test` passes (519 lib tests plus 13 integration
  binaries, including the contract gate and the two new fuzz tests).
  `RUSTFLAGS="-D warnings" cargo clippy --all-targets` reports zero warnings.
  The loader carries no `fn(*mut std::ffi::c_void)` form.
- Commit: 0195202 (signature alignment), 51c4247 (fuzzer coverage)
- Status: OPEN to FIXED, 2026-08-08

### F-12 LOW-MED — non-actionable advisory contact

- Action: FIXED
- Evidence: SECURITY.md:9 previously read "Email details to the project
  maintainer (see git log for contact)". The private-advisory link was the
  real mechanism but sat behind the non-actionable line.
- Fix: make the private advisory the primary and unambiguous channel, add a
  concrete email fallback (the maintainer's public commit address), and keep
  the 48h/7d SLA. The contraction "We'll" in the touched block is expanded.
- RE-VERIFY: SECURITY.md:9-12 point at the advisory and the email; no
  "see git log" reference remains.
- Commit: 9b36d68 (`docs: make SECURITY.md advisory contact actionable (F-12)`)
- Status: OPEN to FIXED, 2026-08-08

### F-13 LOW — contractions in docs

- Action: FIXED
- Evidence: 18 contraction lines across 9 files recorded in
  review/02-evidence-e4-writing.md. The scan missed one further line,
  TCP-Connector.md:72 "do and don't", fixed as part of this finding.
- Fix: rewrite all 19 lines into non-contracted forms with meaning
  unchanged. Seven wiki files are committed in the docs/wiki submodule
  (commit 6217347); README.md and docs/README.md are committed in the parent
  with the submodule pointer bump (commit 921471a). tools/check_docs_static.py
  passes.
- RE-VERIFY: case-insensitive contraction grep over README.md, CHANGELOG.md,
  docs/, and docs/wiki/*.md returns zero matches.
- Commit: 921471a (parent) plus 6217347 (docs/wiki submodule)
- Status: OPEN to FIXED, 2026-08-08

### F-14 LOW — classification scan

- Action: no-action (verified PASS)
- Evidence: the classification grep fires on 13 lines in 6 files; every hit
  triages as a false positive (the demo password "secret123" in LOGIN
  examples, and GitHub Actions `secrets.*` workflow syntax). Zero
  classifiable content exists.
- Fix: none. The repository sits correctly at the OFFICIAL floor.
- Commit: none.
- Status: OPEN to no-action (PASS), 2026-08-08

## Waived items (restated from report v1.0, section 6)

Four items were waived by owner policy at the baseline, recorded in evidence
E2, and remain waived: branch-protection state, MFA and 2FA posture,
organization-level CSM, and the audit of the org reusable workflow behind
ci.yml:20. Each requires server-side or organization-level access that a
checkout does not have.

## Status accounting (jsp-945), v1.3

All transitions re-verified against baseline 585a460, dated 2026-08-08.
The v1.2 round closed the last OPEN finding: F-11 moved OPEN to FIXED.
The v1.3 round resolved F-06: WAIVED to FIXED — the dormant auth feature
was removed per maintainer decision instead of being enabled.

| Finding | Status | Owner | Date | Version |
|---------|--------|-------|------|---------|
| F-01 | FIXED | lead | 2026-08-08 | v1.3 |
| F-02 | FIXED | lead | 2026-08-08 | v1.3 |
| F-03 | WAIVED | auditor | 2026-08-08 | v1.3 |
| F-04 | FIXED | auditor | 2026-08-08 | v1.3 |
| F-05 | WAIVED | lead | 2026-08-08 | v1.3 |
| F-06 | FIXED | lead | 2026-08-08 | v1.3 |
| F-07 | FIXED | lead | 2026-08-08 | v1.3 |
| F-08 | WAIVED | auditor | 2026-08-08 | v1.3 |
| F-09 | FIXED | lead | 2026-08-08 | v1.3 |
| F-10 | FIXED | lead | 2026-08-08 | v1.3 |
| F-11 | FIXED | lead | 2026-08-08 | v1.3 |
| F-12 | FIXED | lead | 2026-08-08 | v1.3 |
| F-13 | FIXED | clerk | 2026-08-08 | v1.3 |
| F-14 | no-action | clerk | 2026-08-08 | v1.3 |

Summary: 10 FIXED, 3 WAIVED, 1 no-action, zero findings remain OPEN.
