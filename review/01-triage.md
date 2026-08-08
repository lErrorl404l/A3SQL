# Clause-Level Applicability Triage — a3db/a3sql

Triage of the 34-brief defence corpus (`briefs-index.md`, 7 Aug 2026; briefs
in the rules config directory) against the pinned snapshot
(`review/00-snapshot.md` @ 585a460). Zero PARTIAL: every brief is APPLIES or
N/A. APPLIES rows receive evidence collection in T3a–T3d; N/A rows are
excluded. Owners and checks for APPLIES rows preserved verbatim from the
hyperplan bundle section 3 final matrix.

Corpus count: 34 briefs (`briefs/*.md`), matches the expected 34 — no
adjustment needed. Note: the bundle N/A prose lists "ncsc-vulnerability-
management's parent overlap", but the final matrix places the brief itself
in the APPLIES set (row 9); the parent-overlap concern is dropped, and the
brief is applied directly.

## APPLIES (11)

| # | Brief | Status | Reason | Owner | Checks |
|---|-------|--------|--------|-------|--------|
| 1 | uk-code-licensing | APPLIES | licence metadata vs LICENSE (MIT/Apache vs APL-SA), SPDX/deny.toml | researcher | cargo metadata + deny.toml diff |
| 2 | defence-git-practices | APPLIES | commit-signing enforcement, CI gates, release provenance | auditor | release-window git log + workflows |
| 3 | writers-handbook | APPLIES | binds review output prose (docs/CHANGELOG/commit text) | clerk | style scan |
| 4 | ncsc-secure-development | APPLIES | FFI/unsafe design-in, secure CI, test security | researcher | catch_unwind/unwrap scans, Miri CI, action pinning |
| 5 | uk-software-security-code-of-practice | APPLIES | boundary validation, build env, signed-release deploy | researcher | FFI barrier, panic profile, artifact sigs |
| 6 | ncsc-supply-chain-security | APPLIES | deps known (Cargo.lock tracked, deny enforced), third-party plugin, submodule pin | auditor | lockfile status, dlopen integrity, git submodule status |
| 7 | jsp-940 | APPLIES | review-process QA (GAI of findings report) | architect | plan + report structure |
| 8 | jsp-945 | APPLIES | review-process CM (report = config item) + repo versioned releases | architect | report fields + tags |
| 9 | ncsc-vulnerability-management | APPLIES | SECURITY.md disclosure path (48h/7d) | auditor | advisory walk-through |
| 10 | security-classifications | APPLIES | no classifiable content in public repo | clerk | CHANGELOG/wiki/plugins scan |
| 11 | gds-service-standard | APPLIES | points 12/13 only (open source, open standards) | clerk | LICENSE/format checks |

## N/A (23)

| # | Brief | Status | Reason | Owner | Checks |
|---|-------|--------|--------|-------|--------|
| 12 | mil-std-882e | N/A | hazard-analysis domain re-homed to uk-software-security panic finding (F-01) | — | — |
| 13 | jsp-936 | N/A | Dependable AI framework; no AI/ML in a3sql | — | — |
| 14 | jsp-376 | N/A | acquisition safety policy; no CADMID acquisition in repo review | — | — |
| 15 | jsp-939 | N/A | M&S policy; no simulation/training software | — | — |
| 16 | defstan-00-56 | N/A | safety-related software standard; no safety case in scope | — | — |
| 17 | aqap-2210 | N/A | NATO software QA; not a NATO programme | — | — |
| 18 | dodi-500087 | N/A | US DOD software acquisition pathway; not US DOD acquisition | — | — |
| 19 | design-review | N/A | deliverable is Markdown, not PDF/slides/UI | — | — |
| 20 | uk-legal-floor | N/A | copyright residue re-homed into uk-code-licensing | — | — |
| 21 | uk-digital-org | N/A | roles/HR framework; no team-structuring output | — | — |
| 22 | latex-pandoc-quarto | N/A | no PDF deliverable; output is Markdown | — | — |
| 23 | uk-pdf-document-standards | N/A | no PDF deliverable; output is Markdown | — | — |
| 24 | osint-practice | N/A | no OSINT research in this review | — | — |
| 25 | operational-reporting | N/A | no SITREP/OPORD-style deliverable | — | — |
| 26 | uk-cyber-governance-code-of-practice | N/A | board-level governance; no board/director output | — | — |
| 27 | uk-ai-cyber-security-code-of-practice | N/A | AI security code; no AI/ML system | — | — |
| 28 | ncsc-cyber-essentials | N/A | org-level certification baseline; not a repo-review criterion | — | — |
| 29 | ncsc-caf | N/A | CNI cyber assessment; a3sql is not CNI | — | — |
| 30 | defstan-05-138 | N/A | secret-scan requirement merged into defence-git-practices | — | — |
| 31 | jsp-815 | N/A | SMS/safety-case framework; no safety-critical claim or hazard log in scope | — | — |
| 32 | jsp-912 | N/A | HFI/UI standard; no UI deliverable | — | — |
| 33 | defence-digital-git | N/A | source reference for defence-git-practices; requirements carried by the practices brief | — | — |
| 34 | uk-cyber-compliance | N/A | org certification umbrella (CE+/NIS/ISO 27001/CSM); not a repo-review criterion | — | — |
