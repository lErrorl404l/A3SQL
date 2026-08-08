# Evidence E2 — Provenance / Git / Supply-Chain / Vulnerability Disclosure

Review task T3b of the rules-compliance review (hyperplan bundle). Auditor-run
evidence for F-03, F-04, F-06 (cross-brief with T3c), F-08, F-09, F-12 and the
F-05 waiver record. All commands ran against the pinned snapshot 585a460
(HEAD 5e5e73c = pin + review/ commits only; `git diff 585a460..HEAD --stat`
shows review/ alone, verified 2026-08-08).

Marking: OFFICIAL. No classifiable content.

## Record format

`F-### | artifact (path+SHA+line) | harm (severity+impact) | fix |
{brief-clause | gap-rider} | status+owner+date | version`

## F-03 SBOM / attestation — OPEN

| Field | Record |
|---|---|
| artifact | `.github/workflows/` (7 files) + `tools/` — `grep -ril 'cyclonedx\|spdx\|sbom'` = 0 hits (exit 1); `keys/` empty (0 entries); `git tag -l` = 0 tags |
| harm | MED. Signed-release integrity unverifiable. Release zips in `releases/` (gitignored, `.gitignore:54`) carry no tag provenance and draft-typo names: `a3sql-0.2.0.0-d.zip`, `a3sql-0.2.0-7.zip`, `a3sql-0.2.0-e.zip`, `A3SQL_0.2.0.zip`, `a3sql-latest.zip`. Consumers cannot verify artifact origin. |
| fix | SBOM-at-tag (SPDX/CycloneDX generated per tag), ship `.bikey` signature key with releases, track the lockfile (see F-04) |
| clause | uk-code-licensing (organisational) + defence-git-practices release-provenance |
| status | OPEN, owner auditor, 2026-08-08 |
| version | baseline 585a460, v1.0 |

## F-04 Cargo.lock untracked — OPEN

| Field | Record |
|---|---|
| artifact | `git ls-files \| grep -c Cargo.lock` = 0; `.gitignore:3` `extension/Cargo.lock`, `.gitignore:4` `Cargo.lock` |
| harm | MED. CI SCA unpinned, dependabot lockfile-blind, shipped-binary reproducibility unproven. `lint.yml:27` caches deny on `hashFiles('extension/Cargo.lock')` — the hash input is untracked, so CI dependency state cannot be reproduced from the repo. |
| fix | Track `extension/Cargo.lock` (remove both .gitignore lines, commit the lock) |
| clause | defence-git-practices reproducibility + ncsc-secure-development |
| status | OPEN, owner auditor, 2026-08-08 |
| version | baseline 585a460, v1.0 |

## F-06 Auth feature dormant by default (cross-brief with T3c) — OPEN

| Field | Record |
|---|---|
| artifact | `extension/Cargo.toml:69-72` (`[features]`; `default = ["sqf-preprocessor"]`, `auth = ["ed25519-dalek"]` NOT default); `extension/src/config.rs:13,25,30` `#[allow(dead_code, reason = "phased auth implementation")]` |
| harm | MED. SECURITY.md self-claim vs shipped capability: SQF query path unauthenticated by default; ed25519 signing support compiles out unless the `auth` feature is selected. Fail-closed listener default exists (`config.rs:50`) but covers the TCP path only. |
| fix | Enable `auth` in default features or record a public roadmap for it |
| clause | gap-rider |
| status | OPEN, owner auditor (cross-ref T3c), 2026-08-08 |
| version | baseline 585a460, v1.0 |

Historical note (v1.3). This collection-era record describes the baseline
state. F-06 was resolved at v1.3 by removing the dormant auth feature
(maintainer decision, review/05-remediation.md F-06), so the record is
retained as historical evidence. The clause field cites no brief: no
governing clause applied, hence the gap-rider classification.

## F-08 Unsigned history — OPEN (honest framing)

| Field | Record |
|---|---|
| artifact | `git log --format='%G?'` whole history: 219 N / 74 G / 3 E across 296 commits; since 2026-01-01 identical (first commit 2026-07-23); last 30 = 26 G (Matthew Barker) + 1 N + 3 E (dependabot[bot]) |
| harm | MED. Non-repudiation partial; signing enforcement unverifiable locally. |
| fix | Enforce `commit.gpgsign = true` and signed release tags; enforcement itself is owner-policy (cannot be verified from checkout) |
| clause | defence-git-practices signed-commits clause |
| status | OPEN, owner auditor, 2026-08-08 |
| version | baseline 585a460, v1.0 |

Honest framing (mandatory): `%G?` = E means "signature cannot be checked —
key not in the local ring", NOT "unsigned". Git verifies signatures only when
the key is present locally. Signing was adopted partway through history:
recent commits are G-majority (26 of last 30 signed by Matthew Barker; the
3 E are dependabot[bot], a bot identity outside the local keyring). The N
remainder (219) are pre-adoption commits, including 59 by `Matt` and 150 by
`Matthew Barker` before signing started.

## F-09 No CODEOWNERS — OPEN

| Field | Record |
|---|---|
| artifact | `test -f .github/CODEOWNERS` = false; `.github/` holds codeql.yml, dependabot.yml, FUNDING.yml, ISSUE_TEMPLATE/, PULL_REQUEST_TEMPLATE.md, release-drafter.yml, stale.yml, workflows/ |
| harm | LOW. Governance gap: no named ownership on files; low impact for a solo-maintainer repo. |
| fix | Add `.github/CODEOWNERS` naming the maintainer and any collaborators |
| clause | defence-git-practices branch-protection / jsp-940 governance |
| status | OPEN, owner lead, 2026-08-08 |
| version | baseline 585a460, v1.0 |

## F-12 Vulnerability disclosure workflow — OPEN (partial, new finding)

| Field | Record |
|---|---|
| artifact | `SECURITY.md` (tracked): line 9 contact "see git log for contact" — no direct email; line 10 links GitHub private advisory form (`github.com/lErrorl404l/a3sql/security/advisories/new`); 48h acknowledgement / 7-day fix commitment stated; scope list lines 14-24 |
| harm | LOW-MED. Advisory path exists as a GitHub-native private-advisory form and is usable; the direct-contact fallback ("see git log") is not actionable for a non-developer reporter. Git log resolves to Matthew Barker <matthewbarker@librem.one> — usable but undocumented. |
| fix | Add a direct contact address to SECURITY.md line 9 and document the triage SLA; keep the private-advisory path |
| clause | ncsc-vulnerability-management |
| status | OPEN, owner lead, 2026-08-08 |
| version | baseline 585a460, v1.0 |

Verdict: F-12 is PARTIAL, not absent. A working private-advisory workflow is
configured through the GitHub security advisory feature and SECURITY.md states
the 48h/7d commitment. The gap is the non-actionable contact line and the
absence of a documented end-to-end disclosure procedure (e.g., advisory
creation steps, embargo policy).

## Compliance evidence (records, not findings)

- Submodule pin: `git submodule status` = `69118c42727a0752012bebdb8d46ff108d3e9de9 docs/wiki` — matches snapshot 00-snapshot.md.
- `cargo deny --manifest-path extension/Cargo.toml check` exit 0: advisories ok, bans ok, licenses ok, sources ok (cargo-deny 0.20.2). Warnings only: GPL-2.0 deprecated SPDX identifier in hemtt-error/hemtt-preprocessor/hemtt-tokens (optional build-time deps of the SQF preprocessor); `deny.toml:14` BSD-3-Clause allowance unmatched. CI gate equivalent: `lint.yml:21-28` deny job via EmbarkStudios/cargo-deny-action@v2.
- Secret scan: `git ls-files | grep -E '\.(rs|toml|yml)$' | xargs grep -nE 'api[_-]?key|password|secret'` — hits are all placeholders or fixtures: `ci.yml:227-249` `secrets.STEAM_*` / `secrets.GITHUB_TOKEN` context refs (no values), `wiki.yml:26` TOKEN ref, `extension/src/server.rs:77,85,231,272-305` test credentials "admin"/"secret", `extension/tests/*` fixture data, `io.rs:430` "secret.txt" symlink-escape test. Verdict: no committed secrets evident from static scan. This proves only that; server-side push protection is unverifiable from a checkout. Note: an unscoped grep also matched `target/` build artifacts (vendored arma3-wiki docs) — untracked, gitignored, not part of the snapshot.

## WAIVED items (owner: lead, 2026-08-08; justification; not investigated)

| Item | Justification |
|---|---|
| Branch-protection state | Server-side GitHub setting; needs admin token, unavailable to this review |
| MFA / 2FA posture | Org-level account policy; not verifiable from checkout |
| Org-level CSM / cyber posture | Out of scope for a repo-level evidence pass |
| `ci.yml:20` org reusable workflow audit (F-05) | `ci.yml:20-25` calls `lErrorl404l/.github/.github/workflows/rust-addon-ci.yml@main` — the private org repo is opaque to this checkout; SHA-pin fix recorded in plan, remediation deferred to org owner |
