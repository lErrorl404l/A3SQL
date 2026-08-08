# Evidence Record E3 — Licence and Openness (F-02, F-06, GDS 12/13)

Task T3c of the approved review plan. Pinned baseline: `585a46079d7b64e4cdf3d1c9a859d710602a91b9`
(review/00-snapshot.md, 2026-08-05). Collected 2026-08-08 at HEAD 5e5e73c
(only review/ commits on top of the pin; `git diff 585a460..HEAD --stat`
shows review/ files only).

Quoting policy (uk-code-licensing fair-dealing, CDPA s.29/30): repo lines are
quoted as short factual attributions only. Brief requirements are
paraphrased with section references; no verbatim policy reprints.

## F-02 — Licence mismatch (extension/Cargo.toml vs LICENSE)

Evidence commands (exit codes recorded):

```
$ grep -in 'license' extension/Cargo.toml LICENSE README.md deny.toml
extension/Cargo.toml:8: license = "MIT OR Apache-2.0"
LICENSE:1:                     Arma Public License Share Alike (APL-SA)
LICENSE:3: Copyright 2026 ABE Team
README.md:6:  [![License](https://img.shields.io/badge/License-APL--SA-red.svg?style=flat-square)](LICENSE)
README.md:111: ## License
README.md:113: Licensed under the [Arma Public License Share Alike (APL-SA)](LICENSE), Copyright 2026 ABE Team.
deny.toml:7: [licenses]
```

(All five paths exist at the pin; deny.toml present at repo root.)

Artifact facts:

- `extension/Cargo.toml:8` — SPDX expression `license = "MIT OR Apache-2.0"`.
- `LICENSE:1-3` — Arma Public License Share Alike (APL-SA) declaration,
  Copyright 2026 ABE Team.
- `README.md:6` (badge) and `README.md:111-113` (License section) — the
  distributed licence is declared as APL-SA.
- `deny.toml:9-19` allow-list — MIT, Apache-2.0, Apache-2.0 WITH
  LLVM-exception, BSL-1.0, GPL-2.0, BSD-3-Clause, ISC, MPL-2.0, Unicode-3.0,
  Zlib. APL-SA does not appear. (Expected: the allow-list governs third-party
  crate dependencies, and cargo-deny does not audit the root package, so this
  is corroboration, not a second defect.)

Record (7-field):

```
F-02 | extension/Cargo.toml:8 + LICENSE:1-3 + README.md:6/113 + deny.toml:9-19
      @ 585a460 | harm: MED — SPDX field does not match the distributed
      licence. APL-SA is non-OSI with no standard SPDX id; a crates.io
      publish would inherit the wrong SPDX expression (cargo publish copies
      the license field verbatim into crate metadata), granting downstream
      consumers a licence the authors did not intend and failing the
      uk-code-licensing SPDX-identification requirement | fix: align the
      Cargo.toml SPDX expression with LICENSE — either `license-file =
      "LICENSE"` (crates.io accepts a license-file for non-SPDX licences),
      or, if the split is intended, document it (crate code MIT/Apache-2.0,
      addon PBOs APL-SA) and add a NOTICE; verify publish status | clause:
      uk-code-licensing, compliance-mechanics SPDX identification (compound
      expressions) + GOV.UK Service Standard point 12 (open under an
      OSI-compatible licence or explain why not) | OPEN, lead, 2026-08-08 |
      v1.0 (report baseline)
```

Harm reasoning (recorded):

- The SPDX identification seam (uk-code-licensing "Compliance mechanics
  (SPDX)") requires the machine-readable licence field to match the
  distributed licence text. Here they disagree.
- APL-SA is not on the SPDX licence list and is not OSI-approved, so no
  standard SPDX id exists to substitute; this is why the mismatch cannot be
  closed by editing the expression alone.
- A crates.io publish of the extension crate would inherit `MIT OR
  Apache-2.0` verbatim. No publish is evident in workflows, but the seam is
  live for anyone who publishes.
- Plausible honest reading: the repo is intentionally APL-SA (the Arma mod
  ecosystem norm; Bohemia Interactive requires it for Workshop distribution)
  and the Cargo.toml field is simply wrong or stale. On that reading the bug
  is the SPDX field, not the licence choice, which caps practical exposure:
  the distributed artifact (mod zips, PBOs) carries the correct licence.
- Severity verdict: MED confirmed (bundle-seeded MED). Not HIGH — the
  distributed artifact declares APL-SA correctly; the defect is confined to
  the crate metadata seam. Not LOW — a publish would silently ship wrong
  licence metadata.

## F-06 — No SBOM/attestation in the build/release path (cross-reference)

Evidence commands:

```
$ grep -ril -e sbom -e spdx -e cyclonedx .github/workflows/ tools/
(no output; exit 1)
$ ls -la keys/
total 0  (no .bikey files)
```

`.github/workflows/` (7 files: build, cache-cleanup, ci, lint,
release-drafter, test, wiki) and `tools/` (26 files) contain no SBOM,
SPDX, or CycloneDX references. `keys/` is empty.

Record (7-field, cross-reference — no duplicate finding):

```
F-06 | .github/workflows/* + tools/* (grep 0 hits, exit 1) + keys/ (empty)
      @ 585a460 | harm: MED — no SBOM or attestation produced anywhere in
      the build/release path; release integrity is unverifiable by machine |
      fix: canonical record is F-03 in the T3b evidence file (defence-git-
      practices / uk-code-licensing SBOM clause): tracked Cargo.lock,
      SBOM-at-tag, ship .bikey | clause: uk-code-licensing organisational-
      control (SPDX SBOM per release generated in CI) — carried by the T3b
      F-03 record | OPEN, lead, 2026-08-08 | v1.0 (report baseline)
```

This record confirms the grep evidence and references the T3b F-03 record
as canonical. The T3b evidence file was not yet committed when this record
was written; it lands with the T3b commit.

## GDS 12/13 — Open source and open standards

GDS 12 — open source: **PASS**. LICENSE is present and named in README
(badge at README.md:6; License section at README.md:111-113; APL-SA,
Copyright 2026 ABE Team). The repo is public. Note: APL-SA is not OSI
approved; the licence is explicit and is the Arma platform norm, which
satisfies the standard's openness intent (licence declared, source
readable and reusable) for a game mod.

GDS 13 — open standards: **PASS with note**. The data interface is SQL,
the open standard; the dialect is documented (docs/wiki/SQL-Dialect.md) and
export is plain CSV. The SQF API surface is Arma-proprietary, but that is
the platform's inherent interface for a game extension, not a standards
choice.

## Accounting

| Finding | Status | Owner | Date | Version |
|---------|--------|-------|------|---------|
| F-02 | OPEN | lead | 2026-08-08 | v1.0 |
| F-06 | OPEN (cross-ref to T3b F-03) | lead | 2026-08-08 | v1.0 |
| GDS 12/13 | PASS / PASS-with-note | lead | 2026-08-08 | v1.0 |
