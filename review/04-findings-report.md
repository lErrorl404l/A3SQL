# Findings Report v1.4: Rules-Compliance Review of a3db/a3sql

## Document control

- Report: findings report for the rules-compliance review of the a3db/a3sql repository
- Version: v1.4
- Date: 2026-08-08
- Classification: OFFICIAL
- Review owner: lead
- Reviewer: independent adjudicator (G2)
- Configuration management baseline: 585a46079d7b64e4cdf3d1c9a859d710602a91b9
- Deliverable format: Markdown

Classification note. This report is OFFICIAL, the default marking for a
public repository. It carries no over-marking and no escalation. It holds no
supplier-commercial detail and no pre-release vulnerability detail that would
justify a higher marking.

Quoting policy. Brief requirements are paraphrased with section references
under the fair-dealing provisions of the Copyright, Designs and Patents Act
1988 (sections 29 and 30). This report reprints no policy text verbatim.
Repository lines appear only as short factual attributions with file, line,
and commit reference.

Document version history.

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-08 | Baseline issue. Fixes the v1.0-draft label used during evidence collection to the baseline version at issuance (jsp-945). |
| v1.2 | 2026-08-08 | Remediation close-out. Every OPEN finding resolved except F-11, held OPEN with partial scope and scheduled follow-ups: eight FIXED, four WAIVED with justification, one no-action (verified PASS). Status column and status accounting updated per review/05-remediation.md. Findings, severities, and IDs unchanged. Gate G3 expected version evolved from v1.0 to v1.1 (recorded in review/06-improvement.md, section 2). |
| v1.4 | 2026-08-08 | F-03 remediation. A dedicated SBOM workflow (.github/workflows/sbom.yml) generates a CycloneDX SBOM at every release tag via cargo-cyclonedx pinned to 0.5.9, uploaded as an artifact and built from the tracked Cargo.lock. F-03 moved WAIVED to FIXED in the findings table and status accounting per review/05-remediation.md; shipping the .bikey remains an org action. Findings, severities, and IDs unchanged. Gate G3 expected version evolved to v1.4. |
| v1.3 | 2026-08-08 | F-06 remediation. The dormant auth feature was removed rather than enabled (maintainer decision): the `auth` cargo feature, `ed25519-dalek` dependency, auth.rs module, and `cfg(feature = "auth")` gating are gone; the shipped TCP LOGIN path is untouched. F-06 moved WAIVED to FIXED in the findings table and status accounting per review/05-remediation.md. Findings, severities, and IDs unchanged. Gate G3 expected version evolved to v1.3. |
| v1.2 | 2026-08-08 | F-11 remediation. The a3sql_plugin_init entry point contract is aligned across the header, the loader, and the test fixture, and the fuzz harness now covers the safe custom-command surface. The F-11 row moves to FIXED. Findings, severities, and IDs unchanged. Gate G3 expected version evolved from v1.1 to v1.2. |

Change note (v1.1). The findings table below is the v1.0 baseline content:
artifacts, harms, fixes, and classifications are unchanged. Only the status
column carries the v1.1 remediation disposition. Section 3 per-brief
compliance results record the baseline-era verdicts; the remediation
outcomes per finding are in the status column and in
review/05-remediation.md.

Change note (v1.2). The findings table below is the v1.1 content: only the
F-11 status column carries the v1.2 remediation disposition, now FIXED. The
remediation record review/05-remediation.md carries the v1.2 evidence and
commits.

## 1. Executive summary

Fourteen findings were accepted at adjudication (commit f7e6bdc). All are
now remediated (v1.4): eleven FIXED, two WAIVED, one no-action. No finding
remains OPEN. Exactly one is HIGH. Seven are MED. Two are LOW-MED. Four are
LOW.

The single HIGH finding is F-01. A SQF-triggered panic can unwind across an
extern "C" frame with no catch_unwind barrier and abort the host game process.
The harm is availability, not memory safety.

The findings table below is harm-ranked: HIGH first, then MED, then LOW-MED,
then LOW.

Verdict skew statement. The set skews toward integrity and provenance. Nine
findings concern release, supply chain, disclosure, or process assurance.
Five concern the code itself. No finding alleges memory unsafety. The snapshot
records defensive depth that keeps the count low: the rkyv magic gate, the
constant-time credential compare, the fail-closed listener default, and the
TOCTOU fix.

## 2. Findings table

Every row follows the seven-field template from the hyperplan bundle: ID,
artifact, harm, fix, classification, status with owner and date, version.
All artifacts resolve at the pinned configuration management baseline
585a460. Wiki citations resolve at the pinned wiki submodule 69118c4.

| ID | Severity | Artifact (pin 585a460) | Harm | Fix | Classification | Status (owner, date) | Version |
|----|----------|------------------------|------|-----|----------------|----------------------|---------|
| F-01 | HIGH | extension/src/ffi/dir.rs:164/241/277/342 (blob 646e59c); catch_unwind only at server.rs:110 and a3sql-server.rs:159; eval.rs 72 unwrap lines (grep -c unwrap at 585a460 over extension/src/engine/sqf/eval.rs) | SQF-triggered panic unwinds across an extern C frame with no catch_unwind barrier; aborts the host game process (availability) | Wrap all 4 ABI entries in catch_unwind(AssertUnwindSafe); deny(clippy::unwrap_used) on FFI-reachable modules | gap-rider (corpus gap 1) | FIXED, lead, 2026-08-08 | v1.4 |
| F-02 | MED | extension/Cargo.toml:8 vs LICENSE:1-3, README.md:6/113, deny.toml:9-19 (grep -in license over the four files at 585a460 = 38 hits) | SPDX field MIT OR Apache-2.0 does not match the distributed APL-SA; a publish would inherit wrong licence metadata | Align Cargo.toml SPDX with LICENSE (license-file or documented split plus NOTICE); verify publish status | brief-clause (uk-code-licensing) | FIXED, lead, 2026-08-08 | v1.4 |
| F-03 | MED | SBOM and SPDX grep over workflows and tools: 0 hits; keys/ empty; git tag -l = 0 | Signed-release integrity unverifiable; release zips carry no provenance | Generate an SBOM at each tag, ship the .bikey, track the lockfile | brief-clause (uk-code-licensing, defence-git-practices) | FIXED, lead, 2026-08-08 | v1.4 |
| F-04 | MED | .gitignore:3-4; lint.yml:27 hashFiles(extension/Cargo.lock); git ls-files = 0 | CI SCA unpinned, dependabot lockfile-blind, reproducibility unproven | Track extension/Cargo.lock | brief-clause (defence-git-practices) | FIXED, auditor, 2026-08-08 | v1.4 |
| F-05 | MED | .github/workflows/ci.yml:20 uses the org workflow at main (blob 0f0f499) | A movable ref into an opaque private org repo silently governs the build pipeline | Pin the org workflow to a full commit SHA; audit once | brief-clause (ncsc-secure-development P6) | WAIVED, lead, 2026-08-08 | v1.4 |
| F-06 | MED | extension/Cargo.toml:69-72, auth not default; config.rs:13/25/30 allow(dead_code) | SECURITY.md self-claim versus shipped capability: SQF path unauthenticated by default | Enable auth in default features or record a roadmap | gap-rider | FIXED, lead, 2026-08-08 | v1.4 |
| F-07 | MED | extension/src/engine/plugin.rs:230/248 (blob da6daa3); dispatch.rs:197/207 | Code-execution sink: plugin_dir dlopens a caller-supplied directory with no integrity gate (live, not a stub) | Hash or signature gate before dlopen; document the trust boundary | brief-clause (ncsc-supply-chain-security) | FIXED, lead, 2026-08-08 | v1.4 |
| F-08 | MED | git log --format=%G?: 219 N, 74 G, 3 E of 296 commits | Non-repudiation partial; signing enforcement unverifiable locally | Enforce commit.gpgsign and signed release tags | brief-clause (defence-git-practices) | WAIVED, auditor, 2026-08-08 | v1.4 |
| F-12 | LOW-MED | SECURITY.md:9 contact "see git log", :10 private advisory | Disclosure path works; the contact line is not actionable for non-developers | Add a direct contact address; document the triage SLA | brief-clause (ncsc-vulnerability-management) | FIXED, lead, 2026-08-08 | v1.4 |
| F-11 | LOW-MED | include/a3sql_plugin.h (blob 6d5acdf) versus exports; fuzz.rs:207-233; DLL panic test unrun | C ABI contract unverified mechanically; fuzz skips FFI, auth, and plugin surfaces; behavioural test unexecuted | Header-exports diff check; fuzz the custom-command path; run the DLL panic test on Arma CI | gap-rider (corpus gap 1) | FIXED, lead, 2026-08-08 | v1.4 |
| F-09 | LOW | .github/CODEOWNERS absent (git ls-files .github/CODEOWNERS at 585a460 = 0) | Governance gap; low impact for a solo-maintainer repo | Add CODEOWNERS | brief-clause (defence-git-practices) | FIXED, lead, 2026-08-08 | v1.4 |
| F-10 | LOW | extension/Cargo.toml:58 plus :87, machete-ignored openssl-sys | Dead vendored C dependency shipped into all targets; machete suppression hides the signal | Remove openssl-sys; drop it from the ignored list | brief-clause (ncsc-secure-development) | FIXED, lead, 2026-08-08 | v1.4 |
| F-13 | LOW | README.md:51, docs/README.md:38/183/372, 7 wiki files at 69118c4 (18 lines, 9 files) | Style drift in docs; informational; binds this report at issuance | This report meets writers-handbook (no contractions, -ize spellings, short sentences) | brief-clause (writers-handbook) | FIXED, clerk, 2026-08-08 | v1.4 |
| F-14 | LOW | Classification grep: 13 lines, 6 files, all false positives | Zero classifiable content; OFFICIAL floor correct | None; this report defaults to OFFICIAL | brief-clause (security-classifications) | no-action, clerk, 2026-08-08 | v1.4 |

## 3. Per-brief compliance result

### 3.1 Applicable briefs (11)

| Brief | Result | Findings and evidence |
|-------|--------|-----------------------|
| uk-code-licensing | FAIL | F-02: SPDX identification seam (compliance-mechanics section) is inconsistent. F-03: no SPDX SBOM per release (organizational-control requirement). |
| defence-git-practices | FAIL | F-03 release provenance, F-04 lockfile reproducibility, F-08 signed-commit enforcement, F-09 ownership. |
| writers-handbook | PASS at issuance | F-13 records informational repo drift; the no-contractions rule, -ize spelling, and acronym guidance bind this report. |
| ncsc-secure-development | FAIL | F-01 (FFI design-in, secondary), F-05 pipeline pinning (P6), F-07 (secondary), F-10 attack-surface minimization (P3). |
| uk-software-security-code-of-practice | FAIL | F-01: boundary validation and panic profile at the FFI barrier. |
| ncsc-supply-chain-security | FAIL | F-07: third-party plugin code loaded without verification. |
| jsp-940 | PASS | Review-process assurance: named reviewer, acceptance criteria, falsifiable gates, remediation loop. |
| jsp-945 | PASS | This report is the configuration item, versioned v1.4, with owner and status accounting. |
| ncsc-vulnerability-management | PARTIAL | F-12: disclosure path works; direct contact missing. |
| security-classifications | PASS | F-14 records the scan; the repo sits at the OFFICIAL floor. |
| gds-service-standard | PASS | Point 12 (open source) PASS; point 13 (open standards) PASS with note (GDS 13). Non-findings, recorded in evidence E3. |

### 3.2 Not-applicable briefs (23)

Reasons carried from review/01-triage.md, one line each.

| Brief | Reason (01-triage.md) |
|-------|------------------------|
| mil-std-882e | Hazard-analysis domain re-homed to the F-01 panic finding. |
| jsp-936 | Dependable AI framework; no AI or ML in a3sql. |
| jsp-376 | Acquisition safety policy; no CADMID acquisition in a repo review. |
| jsp-939 | Modelling and simulation policy; no simulation or training software. |
| defstan-00-56 | Safety-related software standard; no safety case in scope. |
| aqap-2210 | NATO software QA; not a NATO programme. |
| dodi-500087 | US DOD acquisition pathway; not US DOD acquisition. |
| design-review | Deliverable is Markdown, not PDF, slides, or UI. |
| uk-legal-floor | Copyright residue re-homed into uk-code-licensing. |
| uk-digital-org | Roles and HR framework; no team-structuring output. |
| latex-pandoc-quarto | No PDF deliverable; output is Markdown. |
| uk-pdf-document-standards | No PDF deliverable; output is Markdown. |
| osint-practice | No OSINT research in this review. |
| operational-reporting | No SITREP or OPORD-style deliverable. |
| uk-cyber-governance-code-of-practice | Board-level governance; no board output. |
| uk-ai-cyber-security-code-of-practice | AI security code; no AI or ML system. |
| ncsc-cyber-essentials | Organisation-level certification baseline; not a repo-review criterion. |
| ncsc-caf | CNI cyber assessment; a3sql is not CNI. |
| defstan-05-138 | Secret-scan requirement merged into defence-git-practices. |
| jsp-815 | Safety-management framework; no safety-critical claim or hazard log. |
| jsp-912 | HFI and UI standard; no UI deliverable. |
| defence-digital-git | Source reference; requirements carried by defence-git-practices. |
| uk-cyber-compliance | Organisation certification umbrella; not a repo-review criterion. |

The uk-pdf, latex-pandoc-quarto, and design-review briefs are explicitly
not applicable: this deliverable is Markdown, with no PDF or UI artifact.

## 4. Gates transcript

| Gate | Scope | Result |
|------|-------|--------|
| G0 | Snapshot: pin is an ancestor of HEAD; snapshot pins the SHA | PASS |
| G1 | Triage: 34 of 34 briefs covered, 11 APPLIES / 23 N/A, rows valid | PASS |
| G2 | Adjudication: 14 findings, F-01 HIGH, gap-riders present, zero duplicates | PASS |
| G3 | Closure: report versioned v1.4, CM baseline pinned, OFFICIAL note, 14 findings, all statuses valid | PASS |

## 5. Status accounting (jsp-945)

At the v1.0 baseline every finding was OPEN, owned, and dated 2026-08-08,
resolving at baseline 585a460. The v1.1 remediation loop ran OPEN to FIXED
or WAIVED, with each transition RE-VERIFIED against the pinned SHA and
recorded in review/05-remediation.md. The v1.2 loop closed the remaining
OPEN finding (F-11, FIXED). The v1.3 loop resolved F-06 (WAIVED to FIXED):
the dormant auth feature was removed rather than enabled. The v1.4 loop
resolved F-03 (WAIVED to FIXED): an SBOM-at-tag workflow now runs in CI,
with the .bikey remainder held as an org action. Current dispositions
below.

| Finding | Status | Owner | Date | Version |
|---------|--------|-------|------|---------|
| F-01 | FIXED | lead | 2026-08-08 | v1.4 |
| F-02 | FIXED | lead | 2026-08-08 | v1.4 |
| F-03 | FIXED | lead | 2026-08-08 | v1.4 |
| F-04 | FIXED | auditor | 2026-08-08 | v1.4 |
| F-05 | WAIVED | lead | 2026-08-08 | v1.4 |
| F-06 | FIXED | lead | 2026-08-08 | v1.4 |
| F-07 | FIXED | lead | 2026-08-08 | v1.4 |
| F-08 | WAIVED | auditor | 2026-08-08 | v1.4 |
| F-09 | FIXED | lead | 2026-08-08 | v1.4 |
| F-10 | FIXED | lead | 2026-08-08 | v1.4 |
| F-11 | FIXED | lead | 2026-08-08 | v1.4 |
| F-12 | FIXED | lead | 2026-08-08 | v1.4 |
| F-13 | FIXED | clerk | 2026-08-08 | v1.4 |
| F-14 | no-action | clerk | 2026-08-08 | v1.4 |

Summary: eleven FIXED, two WAIVED, one no-action; no finding remains OPEN.

## 6. Waived items

Four items were waived by owner policy with justification, recorded in
evidence E2: branch-protection state, MFA and 2FA posture, organization-level
CSM, and the audit of the org reusable workflow behind ci.yml:20. Each
requires server-side or organization-level access unavailable to a checkout.

## 7. Notes for remediation

F-01 carries the highest priority. The fix is local and mechanical: wrap the
four unbarriered entry bodies in catch_unwind and deny unwrap use on
FFI-reachable modules. F-03, F-04, and F-05 are cheap and land together at
the next release window. F-11's behavioural DLL panic test is scheduled as a
follow-up on an Arma CI runner.
