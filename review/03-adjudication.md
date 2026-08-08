# Adjudication — G2 (Outside Adjudicator)

Independent adjudication of `review/02-evidence-*.md` against the pinned
snapshot `585a46079d7b64e4cdf3d1c9a859d710602a91b9` (review/00-snapshot.md).

Adjudicator: outside reviewer, fresh session, no evidence collection in this
review (jsp-940 assurance separation — the collector does not grade its own
evidence). Adjudicated 2026-08-08. Marking: OFFICIAL, no classified content.

## Method

For every candidate finding in T3a–T3d, the G2 falsifiable criteria were
re-run at the pinned SHA (independent of the collectors' runs):

1. **a — artifact resolves.** Re-ran the evidence command at the pin
   (`git show 585a460:<path>` for greps and line reads; `git log` for history
   claims; `git -C docs/wiki` at 69118c4 for wiki claims).
2. **b — line-range in bounds.** Read the cited lines directly.
3. **c — harm + fix both non-empty.**
4. **d — exactly one of {brief-clause, gap-rider}** (both or neither =
   REJECT).
5. **e — status + owner + date filled.**

## Collision mapping (final numbering)

F-12 and F-06 were each used twice across the evidence files. Final numbering
below is authoritative for the T5 report.

| Evidence record | Final ID | Role |
|---|---|---|
| E2 F-12 (vulnerability disclosure workflow, §F-12) | F-12 | finding (unchanged) |
| E4 F-12 (writers-handbook style scan) | F-13 | finding (renumbered) |
| E4 F-13 (security-classifications scan) | F-14 | finding (renumbered) |
| E2 F-06 (auth feature dormant, cross-brief T3c) | F-06 | finding (unchanged) |
| E3 F-06 (SBOM cross-reference record) | — | NOT a finding; cross-reference record to F-03 (canonical record is E2 F-03); carries no finding ID |
| E3 GDS 12/13 (PASS / PASS-with-note) | — | compliance records, not findings |

Note: E3's "F-06" SBOM record states "no duplicate finding" and names the T3b
F-03 record as canonical. It is evidence corroboration, not a finding, so no
new finding ID is issued for it.

## Per-finding verdicts

All 14 finding records ACCEPT. Re-run evidence (exit codes or read results):

| Finding (evidence file) | Final ID | Verdict | Re-run evidence at pin |
|---|---|---|---|
| E1 F-01 | F-01 | ACCEPT | a) `git show 585a460:extension/src/ffi/dir.rs \| grep -c catch_unwind` = **0**; ffi/tests.rs = 0; b) `no_mangle` at dir.rs 163/240/276/341 → declarations at 164/241/277/342 (`RVExtensionVersion/RVExtension/RVExtensionArgs/RVExtensionRegisterCallback`); :145 `sql_handler`, :146 `unwrap_or_else`; catch_unwind only at server.rs:110 and a3sql-server.rs:159; c) harm+fix present; d) gap-rider; e) OPEN/lead/2026-08-08 |
| E3 F-02 | F-02 | ACCEPT | a) `grep -in license` over Cargo.toml/LICENSE/README.md/deny.toml: Cargo.toml:8 `license = "MIT OR Apache-2.0"`, LICENSE:1 APL-SA + :3 Copyright 2026 ABE Team, README.md:6/111-113 APL-SA, deny.toml:9-19 allow-list (no APL-SA); b) in bounds; c) non-empty; d) brief-clause (uk-code-licensing); e) filled |
| E2 F-03 | F-03 | ACCEPT | a) `git grep -rilE 'cyclonedx\|spdx\|sbom' 585a460 -- .github/workflows/ tools/` = 0 hits (exit 1); `git tag -l` = **0**; keys/ empty at pin; b) n/a (no line range); c) non-empty; d) brief-clause (uk-code-licensing + defence-git-practices); e) filled |
| E2 F-04 | F-04 | ACCEPT | a) `git ls-files \| grep -c Cargo.lock` = **0** (exit 1); .gitignore:3-4; lint.yml:27 `hashFiles('extension/Cargo.lock')`; b) in bounds; c) non-empty; d) brief-clause; e) filled |
| E1 F-05 | F-05 | ACCEPT | a) ci.yml:20 = `uses: lErrorl404l/.github/.github/workflows/rust-addon-ci.yml@main` (blob 0f0f499 verified); checkout@3d3c42e at :31, cache@v6 at :88; b) in bounds; c) non-empty; d) brief-clause (ncsc-secure-development P6); e) filled |
| E2 F-06 (auth) | F-06 | ACCEPT | a) Cargo.toml:69-72 `[features]` — `default = ["sqf-preprocessor"]`, `auth` NOT default; config.rs:13/25/30 `#[allow(dead_code, reason = "phased auth implementation")]`; config.rs:50 `listener_require_auth.unwrap_or(true)`; b) in bounds; c) non-empty; d) gap-rider; e) filled |
| E1 F-07 | F-07 | ACCEPT | a) plugin.rs:230 `libloading::Library::new(path)` (dlopen, no hash/signature gate), :248 `std::mem::forget(lib)`; dispatch.rs:197 (`register_function`) / :207 (`plugin_dir`) both wired — **live sink confirmed, not a stub**; b) in bounds; c) non-empty; d) brief-clause (ncsc-supply-chain-security); e) filled |
| E2 F-08 | F-08 | ACCEPT | a) `git log 5e5e73c --format='%G?'` (collection SHA) = 296 commits: 219 N / 74 G / 3 E; last 30 = 26 G / 1 N / 3 E; E = dependabot[bot] only (key-not-in-ring, not unsigned — honest framing confirmed); b) n/a (history artifact); c) non-empty; d) brief-clause; e) filled |
| E2 F-09 | F-09 | ACCEPT | a) `git cat-file -e 585a460:.github/CODEOWNERS` = exit 1 (not in tree); .github/ holds codeql.yml, dependabot.yml, FUNDING.yml, etc.; b) n/a; c) non-empty; d) brief-clause; e) filled |
| E1 F-10 | F-10 | ACCEPT | a) Cargo.toml:58 `openssl-sys = { version = "0.9", features = ["vendored"] }`; :87 in machete `ignored` list (:83-94); zero openssl refs in src/tests (grep = 0); b) in bounds; c) non-empty; d) brief-clause; e) filled |
| E1 F-11 | F-11 | ACCEPT | a) fuzz.rs:207-233 `is_custom_command` skips ping/reset/save/load/listen/connect/plugin_dir/register_function/set_credentials before dispatch (verified, function body 209-233); include/a3sql_plugin.h = 1807 B, blob 6d5acdf (verified); behavioural DLL panic test unrun (OBJ-unrun, recorded as scheduled follow-up — honest); b) in bounds; c) non-empty; d) gap-rider; e) filled |
| E2 F-12 (advisory) | F-12 | ACCEPT | a) SECURITY.md:9 contact "see git log for contact", :10 private-advisory link; 48h/7d commitment at :11-12; scope list 14-24; b) in bounds; c) non-empty; d) brief-clause (ncsc-vulnerability-management); e) filled |
| E4 F-12 (style) | F-13 | ACCEPT | a) contraction lines read at pin: README.md:51 "don't"; docs/README.md:38/183/372; wiki at 69118c4: Getting-Started.md:3/84/105/121/157, Module-Guide.md:90/238/274, Development-Setup.md:6, Plugins.md:31, SQL-Dialect.md:554, TCP-Connector.md:93/152, Home.md:21 — 18 lines across 9 files confirmed (2 repo + 7 wiki); zero -ise forms (grep exit 1); 20 commit messages clean (grep 0 hits); b) in bounds; c) non-empty; d) brief-clause (writers-handbook); e) OPEN/clerk/2026-08-08 |
| E4 F-13 (classification) | F-14 | ACCEPT | a) classification grep at pin: 13 lines across 6 files confirmed — workflow refs ci.yml:227/229/245/246/249 + wiki.yml:26 + release-drafter.yml:22 (all `secrets.*` GitHub Actions syntax, no values) and wiki demo creds Security.md:52, Standalone-Server.md:82, TCP-Connector.md:32/80/132/140 ("secret123" placeholder); zero classifiable content — OFFICIAL floor correct; b) in bounds; c) non-empty; d) brief-clause (security-classifications); e) OPEN/clerk/2026-08-08 |

GDS 12/13 records (E3): ACCEPTED as compliance records, not findings. GDS 12
PASS — LICENSE present and named in README (APL-SA declared); GDS 13
PASS-with-note — SQL/CSV open interface, SQF is platform-inherent.

### Minor citation variances (non-material, do not affect any verdict)

- E1 trigger.rs unwrap count: record says 16; `grep -cE '\.unwrap\('` = 16,
  `grep -c 'unwrap'` = 18. Density corroboration only; the F-01 structural
  claim (zero catch_unwind at 4 ABI entries) is independently verified.
- E1 ffi.rs regression test cited as `ffi_args_null_output_returns_minus_one`;
  actual name `ffi_args_sql_null_argv_returns_minus_one` (tests/ffi.rs:397-405,
  null-argv panic regression — substance verified).
- E4 commit-window notation "`5e5e73c..8486c63`" is opaque (8486c63 exists but
  is not in this branch); the 20-commit message style check was re-run clean.

## Severity calibration

- **F-01 HIGH — correct.** Four unbarriered `extern "C"` entries with
  `panic = "unwind"` default (no `panic = "abort"` anywhere in the manifest)
  and 72 unwrap lines in eval.rs; untrusted mission SQF reaches the eval path.
  Unwinding across a non-unwind ABI is UB and in practice aborts the host
  game process. Availability harm, not memory unsafety — HIGH stands.
- **F-07 MED — correct despite the plan correction.** The evidence proved the
  plugin path is LIVE (dispatch.rs:197/207), not the stub scaffold the plan
  assumed. MED, not HIGH, because exploitation needs `plugin_dir` invoked on
  an attacker-writable directory inside a server the admin runs; it is a
  real code-execution sink with an admin-side precondition, not an
  unauthenticated remote sink. The correction is recorded in the evidence and
  strengthens, not weakens, the finding.
- **F-08 MED — correct.** 74% of history (219/296) is unsigned; recent
  commits are signed (26/30 G at collection) and the 3 E are dependabot
  (key-not-in-ring, not unsigned). MED for partial non-repudiation is right;
  the honest framing in the evidence is accurate.
- **F-12 advisory LOW-MED — correct.** The GitHub-native private-advisory
  path works and the 48h/7d commitment is stated; only the contact line is
  non-actionable. MED would overstate; LOW-MED matches a partial gap in an
  otherwise-functional process.
- F-02 MED, F-03 MED, F-04 MED, F-05 MED, F-06 MED, F-09 LOW, F-10 LOW,
  F-11 LOW-MED, F-13 LOW, F-14 LOW — all calibrated to their stated harm;
  no challenge.

## Acceptance criteria (bundle section 7)

| # | Criterion | Verdict |
|---|---|---|
| a | total findings <= 18 | PASS — 14 |
| b | harm-ranked, F-01 HIGH first, no MED outranks a HIGH | PASS — F-01 HIGH first; MEDs follow; LOW-MED/LOW after |
| c | 3 cheapest greps each yielded >= 1 finding | PASS — catch_unwind grep (0 hits) → F-01; licence grep → F-02; SBOM grep (0 hits) → F-03 |
| d | >= 1 gap-rider finding | PASS — F-01, F-06, F-11 are gap-riders |
| e | zero findings cite a brief clause without artifact proving it | PASS — every cited artifact re-verified at the pin (blobs 646e59c, 0f0f499, da6daa3, f784f51, 6d5acdf, 4a4d2a5, d6c72e5, 9e8df0b all match) |

## Final numbered finding list (T5 report MUST use this)

Ordered by harm, not by numeric ID. 14 findings, all OPEN.

| ID | Severity | Artifact (pin 585a460) | Harm | Fix | Classification | Status+Owner+Date |
|---|---|---|---|---|---|---|
| F-01 | HIGH | extension/src/ffi/dir.rs:164/241/277/342 (blob 646e59c); catch_unwind only at server.rs:110, a3sql-server.rs:159; eval.rs 72 unwrap lines | SQF-triggered panic unwinds across extern C frame with no catch_unwind barrier; aborts host game process (availability) | catch_unwind(AssertUnwindSafe) at all 4 ABI entries; deny(clippy::unwrap_used) on FFI-reachable modules | gap-rider (corpus gap #1) | OPEN, lead, 2026-08-08 |
| F-02 | MED | extension/Cargo.toml:8 vs LICENSE:1-3, README.md:6/113, deny.toml:9-19 | SPDX field MIT OR Apache-2.0 does not match distributed APL-SA; a publish would inherit wrong licence metadata | Align Cargo.toml SPDX with LICENSE (license-file or documented split + NOTICE); verify publish status | brief-clause | OPEN, lead, 2026-08-08 |
| F-03 | MED | workflows+tools SBOM/SPDX grep 0 hits; keys/ empty; git tag -l = 0 | Signed-release integrity unverifiable; release zips carry no provenance | SBOM-at-tag, ship .bikey, track lockfile | brief-clause | OPEN, auditor, 2026-08-08 |
| F-04 | MED | .gitignore:3-4; lint.yml:27 hashFiles(extension/Cargo.lock); git ls-files = 0 | CI SCA unpinned, dependabot lockfile-blind, reproducibility unproven | Track extension/Cargo.lock | brief-clause | OPEN, auditor, 2026-08-08 |
| F-05 | MED | .github/workflows/ci.yml:20 uses org workflow @main (blob 0f0f499) | Movable ref into opaque private org repo silently governs the build pipeline | Pin org workflow to full commit SHA; audit once | brief-clause | OPEN, lead, 2026-08-08 |
| F-06 | MED | extension/Cargo.toml:69-72 auth not default; config.rs:13/25/30 allow(dead_code) | SECURITY.md self-claim vs shipped: SQF path unauthenticated by default | Enable auth in default features or record roadmap | gap-rider | OPEN, auditor, 2026-08-08 |
| F-07 | MED | extension/src/engine/plugin.rs:230/248 (blob da6daa3); dispatch.rs:197/207 | Code-execution sink: plugin_dir dlopens caller-supplied dir with no integrity gate (live, not stub) | Hash/signature gate before dlopen; document trust boundary | brief-clause | OPEN, lead, 2026-08-08 |
| F-08 | MED | git log --format=%G?: 219 N / 74 G / 3 E of 296 commits | Non-repudiation partial; signing enforcement unverifiable locally | Enforce commit.gpgsign and signed release tags | brief-clause | OPEN, auditor, 2026-08-08 |
| F-12 | LOW-MED | SECURITY.md:9 contact see git log, :10 private advisory | Disclosure path works; contact line not actionable for non-developers | Add direct contact address; document triage SLA | brief-clause | OPEN, lead, 2026-08-08 |
| F-11 | LOW-MED | include/a3sql_plugin.h (blob 6d5acdf) vs exports; fuzz.rs:207-233; DLL panic test unrun | C ABI contract unverified mechanically; fuzz skips FFI/auth/plugin surfaces; behavioural test unexecuted | Header-exports diff check; fuzz custom-command path; run DLL panic test on Arma CI | gap-rider (corpus gap #1) | OPEN, lead, 2026-08-08 |
| F-09 | LOW | .github/CODEOWNERS absent | Governance gap; low impact for solo-maintainer repo | Add CODEOWNERS | brief-clause | OPEN, lead, 2026-08-08 |
| F-10 | LOW | extension/Cargo.toml:58 + :87 machete-ignored openssl-sys | Dead vendored C dep shipped into all targets; machete suppression hides signal | Remove openssl-sys and drop from ignored list | brief-clause | OPEN, lead, 2026-08-08 |
| F-13 | LOW | README.md:51, docs/README.md:38/183/372, 7 wiki files at 69118c4 (18 lines/9 files) | Style drift in docs; informational, binds report prose at issuance | T5 report must meet writers-handbook (no contractions, -ize, short sentences) | brief-clause | OPEN, clerk, 2026-08-08 |
| F-14 | LOW | classification grep: 13 lines/6 files, all false positives | Zero classifiable content; OFFICIAL floor correct | None; T5 report defaults OFFICIAL | brief-clause | OPEN, clerk, 2026-08-08 |

All records carry baseline 585a460, version v1.0 (report baseline; v1.0-draft
in E4 fixed to v1.0 at G3 issuance per jsp-945).

## Gate result

`bash review/gates.sh G2` — PASS (see gate output).
