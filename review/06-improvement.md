# Improvement Record v1.1: Review-Process Feedback (jsp-940)

## Document control

- Report: improvement loop for the rules-compliance review of a3db/a3sql
- Version: v1.1
- Date: 2026-08-08
- Classification: OFFICIAL
- Owner: lead
- Configuration management baseline: 585a46079d7b64e4cdf3d1c9a859d710602a91b9

This record closes the jsp-940 improvement loop: what the remediation round
learned about the review process itself. Each item names the lesson, the
evidence, and the change for the next round.

## 1. Evidence numbering collision

The evidence ledger (review/02-evidence-e4-writing.md) assigned F-12 to the
writers-handbook scan and F-13 to the security-classifications scan. The
final report (review/04-findings-report.md) renumbered: F-12 is the advisory
contact, F-13 is the contractions fix, F-14 is the classification scan. The
G2 collision mapping table resolved the duplicates, but the mismatch
survived into the remediation brief: the task instructions cited F-13 as the
contractions finding while evidence E4's F-13 was the classification scan.

Lesson. Evidence F-IDs are provisional until the report freeze. Collision
mapping resolves duplicates after the fact; it does not remove the
confusion for later phases that consume the final IDs.

Change for next round. Renumber evidence sections to the final F-IDs at
adjudication, before the report issues. Or carry explicit provisional IDs
(EN-1, EN-2) in evidence so no collision with final IDs is possible.

## 2. Gate-design lessons

Three faults surfaced in review/gates.sh.

First, G3 hard-codes the report version in two places: the version-line
grep (`Version: v1.0`) and the per-row version check in the awk pass
(`ver != "v1.0"`). The v1.1 bump required editing both. A single version
variable at the top of the script would make the bump one edit, not two.

Second, G3 restricts the status vocabulary to OPEN, FIXED, and WAIVED. The
remediation of F-11 is partial by design (a finding-note recorded, two
follow-ups scheduled), which has no gate-valid status. The record had to
carry "OPEN (PARTIAL)" inside the OPEN form, which loses the partial
signal in machine-readable terms.

Third, the version change is itself a gate evolution. G3 was the gate that
verified v1.0 closure, and it now verifies v1.1. That is legitimate
progression, not gate drift, because the change was recorded in this
improvement record alongside the report bump commit.

Change for next round. Parameterize the version constant. Add PARTIAL to
the accepted status vocabulary, or split status from a "remediation state"
field so partial progress is representable.

## 3. Submodule boundary in remediation

The F-13 contraction fix spans two repositories: seven wiki files live in
the docs/wiki submodule, and two files live in the parent. Remediation
required a commit in the submodule followed by a parent pointer bump. The
report's artifact citations (docs/wiki/...@69118c4) worked because the
submodule was pinned, but the two-repo commit pattern was not flagged for
the remediation phase.

Change for next round. When evidence cites submodule content, record in the
finding that remediation crosses a repository boundary and names the
expected commit shape.

## 4. Evidence-scan miss

Evidence E4 recorded 18 contraction lines. The remediation re-scan found a
19th (TCP-Connector.md:72, "do and don't") that the evidence scan missed.
The re-scan after remediation is what caught it.

Change for next round. Run scans with a documented, case-insensitive regex
and re-run the same scan after remediation to catch stragglers. Record the
scan command in the evidence so the re-run is byte-identical.

## 5. Test-first for manifest and gate findings

Behavioural findings (F-01, F-07) take a failing test that aborts or
misbehaves. Manifest findings (F-02, F-04, F-10) have no runtime test; the
failing "test" is a machine-checkable command: `cargo metadata` exit 0, the
lockfile presence count, `cargo machete`. The remediation records both
classes with their acceptance commands.

Change for next round. Distinguish behavioural TDD from gate-command TDD in
the remediation record, so a missing behavioural test is not confused with
a manifest change that cannot have one.

## 6. Vestigial test fixture

The pre-existing plugin test (tests/plugins.rs gap_c_abi_plugin) copies
/tmp/test_plugin3.so, a fixture no build step produces. When the fixture is
absent the test passes vacuously: plugin_dir on an empty directory returns
an OK envelope. The new F-07 test builds its own fixture with the local
rustc (68 ms) and asserts a real load and a real rejection.

Change for next round. Audit tests whose fixtures are not built by the
pipeline. Name them, build the fixture in the test, or remove them.
Vacuous assertions are false coverage.

## Summary

Five lessons for the next review round: renumber evidence to final IDs at
adjudication, parameterize gate versions and widen the status vocabulary,
flag submodule boundaries in findings, re-run scans post-remediation, and
distinguish behavioural from gate-command TDD.
