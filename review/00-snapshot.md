# Review Snapshot — a3db/a3sql @ 585a460

Pinned baseline for the rules-compliance review (hyperplan bundle, task T1).
All later findings cite this snapshot.

- SHA: 585a46079d7b64e4cdf3d1c9a859d710602a91b9
- Date: 2026-08-05
- Branch: main
- Working tree: clean

## Inventory

Source of truth: `git ls-files`. 386 tracked files total (203 addons, 113
extension, 26 tools, 15 .github, 7 include, 7 .hemtt, plus root-level files).

### addons/ — 203 tracked files, 10 subdirectories

| Subdir | Role |
|--------|------|
| admin | admin tooling |
| analytics | analytics framework |
| database | database layer |
| loadouts | loadout handling |
| main | core addon |
| patch_core | core patch |
| patch_editor | editor patch |
| patch_operators | operators patch |
| persistence | persistence layer |
| progression | progression system |

### tools/ — 26 tracked files (Python tooling)

Present and tracked: `sql_smoke_test.py`, `check_docs_static.py`,
`sqf_validator.py`, `config_style_checker.py`, `sql_dialect_sweep.py`,
`sql_corpus/`, `getExtensionHash.py`. Also: `a3sql-patch.py`, `a3sql-sync.py`,
`a3sql-webhook.py`, `build_current_addon.py`, `build_dev.sh`,
`copy_ext_binaries.sh`, `fuzz_corpus.txt`, `launch.sh`, `proton.py`,
`search_privates.py`, `search_unused_privates.py`, `setup.py`,
`smoke_test.sql`, `sqfvmChecker.py`, `sql_compat_report.py`, `__init__.py`.
Untracked and gitignored: `tools/__pycache__/`.

### include/ — 7 tracked files

- `include/a3sql_plugin.h` (public C ABI header)
- `include/x/cba/` (vendored CBA headers): `main/` ($PBOPREFIX$,
  script_macros_common.hpp, script_macros.hpp, script_mod.hpp) and `xeh/`
  ($PBOPREFIX$, script_xeh.hpp)

### plugins/ — 1 tracked file

- `plugins/example/plugin.c` (third-party plugin example; no integrity check
  before dlopen — see F-07)

### releases/ — 5 zips, no tag provenance

`git tag -l` = 0. Release artifacts lack provenance (names carry draft
typos):

- `a3sql-0.2.0.0-d.zip`
- `a3sql-0.2.0-7.zip`
- `a3sql-0.2.0-e.zip`
- `A3SQL_0.2.0.zip`
- `a3sql-latest.zip`

### .github/workflows/ — 7 files

`build.yml`, `cache-cleanup.yml`, `ci.yml`, `lint.yml`,
`release-drafter.yml`, `test.yml`, `wiki.yml`

### keys/ — empty

No `.bikey` files committed. Signed-release integrity unverifiable (F-03).

### docs/wiki — submodule pinned

`git submodule status` → `69118c42727a0752012bebdb8d46ff108d3e9de9 docs/wiki`

## Top-level gitignored binaries (present, NOT committed)

Root `a3sql.dll`, `a3sql.so`, `a3sql_x64.dll`, `a3sql_x64.so` are present in
the working tree, gitignored + untracked. NOT part of the pinned snapshot.

## Evidence baseline (verified ground truth, HEAD 585a460 @ 2026-08-05)

Embedded verbatim from hyperplan bundle section 6:

- rkyv magic gate EXISTS: `serialize/binary.rs:26` `BINARY_MAGIC=b"A3SQ"`,
  `:172` rejects mismatch + length before parse; FNV-1a trailer with
  bit-flip/truncation tests (`io.rs:567-607`).
- TOCTOU fix real: `write_no_follow` O_NOFOLLOW (`io.rs:171-182`),
  symlink-escape tests (`io.rs:427-464`), crash-window durability
  (`io.rs:567-811`).
- `ct_eq` constant-time (`server.rs:25-32`) + fail-closed listener default
  (`config.rs` `listener_auth_required=true`).
- FFI surface tested in-process: `extension/tests/ffi.rs` (21) exercises
  RVExtension/RVExtensionArgs/RVExtensionVersion with real C pointers;
  `abi.rs` (28) is dispatch-level API tests; real-binary coverage = `ci.yml`
  `sql_smoke_test.py`.
- Miri CI filter `--lib -- auth engine::serialize engine::index
  engine::functions::eval::test dispatch server engine::trigger`
  (`test.yml:39`); FFI surface deliberately excluded (miri can't run
  extern "C").
- 728 = `#[test]` ATTRIBUTES in source; CI documents 349 in lib suite
  (`test.yml:34`).

## Note

Inventory only. No triage, no judgement — this snapshot is the pinned
baseline every later finding cites.
