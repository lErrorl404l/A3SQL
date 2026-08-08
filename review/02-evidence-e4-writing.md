# Evidence E4: Writing and Classification Scans

Task T3d of the approved review plan. Owner: clerk. Date: 2026-08-08.
Pinned baseline: `585a46079d7b64e4cdf3d1c9a859d710602a91b9`
(review/00-snapshot.md). Scans executed at HEAD `5e5e73c7` (working tree
clean). Verified: no tracked prose file changed between the pin and the
scan, so the scans ran against the pinned content. The wiki submodule is
pinned at `69118c4` and was read read-only through the working tree.

Classification: OFFICIAL. This file is review output. It records
informational evidence only. It carries no classified content. Both
scans bind the review output at issuance (task T5), not the repository
as a gate. The repository scans are evidence, not gates.

Two checks ran. Two records follow. Both use the 7-field template
`F-### | artifact | harm | fix | clause | status+owner+date | version`.

## F-12 | writers-handbook style scan

F-12 | `README.md+585a460:4,51`, `CHANGELOG.md+585a460:28`,
`docs/README.md+585a460:38,183,372`,
`docs/wiki/Getting-Started.md+69118c4:3,84,105,121,157`,
`docs/wiki/Module-Guide.md+69118c4:90,238,274`,
`docs/wiki/Development-Setup.md+69118c4:6`, `docs/wiki/Plugins.md+69118c4:31`,
`docs/wiki/SQL-Dialect.md+69118c4:554`, `docs/wiki/TCP-Connector.md+69118c4:93,152`,
`docs/wiki/Home.md+69118c4:21`, `git log --oneline -20` (`5e5e73c..8486c63`)
| LOW, informational: the handbook binds the review report prose at
issuance, not the repository. Style drift in docs has no product impact.
| Fix: none in the repo. The T5 report must meet the standard (short
sentences, active voice, no contractions, -ize spellings, minimal
acronyms).
| `writers-handbook`: no-contractions rule (Punctuation and Grammar
tables), -ize spelling (Spelling section), minimize acronyms (Acronyms
section)
| OPEN, clerk, 2026-08-08 | v1.0-draft

Verdict: PARTIAL.

Pass items:
- Sentence length. Prose runs short. Example:
  "**A3SQL** is a live SQL database engine for Arma 3. It runs inside
  your server and lets your mission store, query, and update data with
  plain SQL." (README.md:4)
- Active voice. Imperatives carry the install and build sections.
  Example: "Install the mod from the Steam Workshop, or download the
  latest release and unpack `@a3sql` into your Arma 3 directory."
  (docs/README.md:32)
- Spelling. Zero -ise forms across all prose surfaces and all 20 commit
  messages. Example: "ROLLBACK is no-op when no transaction is active
  (matches PostgreSQL)." (CHANGELOG.md:28)

Fail items:
- Contractions. 18 prose lines across 9 files. Examples:
  - "A3SQL is modular. Remove any PBO you don't need:" (README.md:51)
  - "That's the whole mod: create, record, persist, query."
    (docs/wiki/Getting-Started.md:157)
  - "When you're done you'll have a working mod that creates a table,
    records an event, saves it to disk, and reads it back."
    (docs/wiki/Getting-Started.md:3)
- Acronym density. SQL, SQF, PBO, CBA, CSV, JSON, TCP, API, FFI, HEMTT,
  CLI appear without expansion. CBA links to its repository. SQF and PBO
  are Arma domain terms the audience knows. Partial pass for a mod
  audience.

Commit messages pass. All 20 use conventional-commit prefixes, an
imperative verb, no contractions, and no -ise forms.

## F-13 | security-classifications scan

F-13 | public-repo scan, pinned tree `585a460`:
`grep -rilE 'TOP SECRET|SECRET|OFFICIAL-SENSITIVE|RESTRICTED|CONFIDENTIAL'`
over `README.md CHANGELOG.md docs/ addons/ plugins/ .github/`. Exit 0:
6 files, 13 lines matched. All triaged as false positives. See detail
below.
| LOW, zero classifiable content: the grep produced signal, so a triage
pass was required. The triage resolved every hit as benign. No
classification marking, no over-marking, no under-marking exists.
| Fix: none required. The repo correctly sits at the OFFICIAL floor.
The T5 report must default to OFFICIAL and carry no over-marking.
| `security-classifications`: OFFICIAL is the default (scheme table);
stop over-classification and under-marking (common defects list)
| OPEN, clerk, 2026-08-08 | v1.0-draft

False-positive triage, 13 lines:

Prose (6 lines). The word "secret" in a demo login password:
- `docs/wiki/Security.md:52` `LOGIN admin secret123`
- `docs/wiki/Standalone-Server.md:82` same demo
- `docs/wiki/TCP-Connector.md:32,80,132,140` same demo in SQF, Python,
  and C examples

Workflow files (7 lines). `secrets.*` is GitHub Actions syntax for
encrypted workflow variables, not a classification:
- `.github/workflows/wiki.yml:26` `secrets.GITHUB_TOKEN`
- `.github/workflows/ci.yml:227,229,245,246,249`
  `secrets.STEAM_WORKSHOP_PUBLISHEDID`, `secrets.STEAM_USERNAME`,
  `secrets.STEAM_PASSWORD`, `secrets.STEAM_GUARD_CODE`
- `.github/workflows/release-drafter.yml:22` `secrets.GITHUB_TOKEN`

The demo password `secret123` is a placeholder credential inside LOGIN
examples. It is not a real secret and not a classification marking.
Readers may copy it into a live config; that is documentation hygiene,
not a classification defect, and it sits outside this brief.

Severity note. LOW, not N/A: the scan fired (13 hits), so a real triage
occurred. An N/A record would be correct only for a zero-hit grep with
no classifiable content to review. The hits here were triageable and
were triaged.

## Verdict

Style scan: PARTIAL, informational, LOW. The repo prose is strong on
voice, sentence length, and spelling, and weak on contractions.

Classification scan: PASS, LOW. The public repository holds no
classifiable content and sits correctly at the OFFICIAL floor.

Both records await G2 adjudication by the outside adjudicator.

Status accounting per jsp-945: F-12 and F-13 are OPEN, owned by clerk,
dated 2026-08-08, versioned v1.0-draft. Baseline v1.0 is fixed at
issuance (G3).
