#!/usr/bin/env python3
"""
SQL compatibility report — run a corpus of realistic SQL workloads through
the real a3sql extension and produce a human-readable report.

The corpus in tools/sql_corpus/ models what real Arma mods do (stats
trackers, admin systems, loadout managers). A mod developer can drop their
own .sql file in and get an instant "does a3sql handle my SQL?" answer.

Usage:
  python3 tools/sql_compat_report.py [--bin PATH] [file.sql ...]

  --bin PATH   Extension binary (auto-detected like sql_smoke_test.py).
  file.sql     Corpus files. Defaults to tools/sql_corpus/*.sql

Corpus file format (same as smoke test):
  SQL statement per line, '#' comments, '# expect error' / '# expect contains X'
  to assert behaviour. All statements run in ONE shared database session,
  so earlier statements build state later ones read.

Exit code: 0 all pass, 1 any fail.
"""

import ctypes
import glob
import os
import shutil
import sys
import tempfile

BUF_SIZE = 30720


def find_binary(explicit):
    if explicit:
        return explicit
    env = os.environ.get("A3SQL_BIN")
    if env:
        return env
    for c in [
        "extension/target/release/liba3sql.so",
        "a3sql_x64.so",
        "a3sql.so",
    ]:
        if os.path.exists(c):
            return c
    return "extension/target/release/liba3sql.so"


def load_lib(path):
    lib = ctypes.CDLL(os.path.abspath(path))
    lib.RVExtension.argtypes = [ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p]
    lib.RVExtension.restype = None
    return lib


def call(lib, statement):
    """STRING-form call — identical to `"a3sql" callExtension stmt` in SQF."""
    buf = ctypes.create_string_buffer(BUF_SIZE)
    lib.RVExtension(buf, BUF_SIZE, statement.encode("utf-8", "replace"))
    return buf.value.decode("utf-8", "replace")


def run_file(lib, path):
    """Run one corpus file in the shared session. Returns (passed, failed, [failures]).

    Statements are accumulated until a line ends with ';' — real SQL files
    span multiple lines. '#' comments and '# expect X' assertions are
    honored at any point."""
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()

    passed = failed = 0
    failures = []
    pending = "ok"  # "ok" | "error" | ("contains", text)

    # (start_line, statement_text) being accumulated
    buf_line = None
    buf = []

    def flush():
        nonlocal buf_line, buf
        if buf_line is None:
            return
        stmt = " ".join(buf).strip()
        response = call(lib, stmt)
        if pending == "error":
            ok = not response.startswith('[0,"OK"')
        elif isinstance(pending, tuple):
            ok = pending[1] in response
        else:
            ok = response.startswith('[0,"OK"')
        if ok:
            nonlocal passed
            passed += 1
        else:
            nonlocal failed
            failed += 1
            failures.append((buf_line, stmt[:80], response[:160]))
        buf_line = None
        buf = []

    for lineno, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("# expect "):
            flush()
            if stripped.startswith("# expect error"):
                pending = "error"
            elif stripped.startswith("# expect contains "):
                text = stripped[len("# expect contains ") :].strip()
                if len(text) >= 2 and text[0] == text[-1] and text[0] in "'\"":
                    text = text[1:-1]
                pending = ("contains", text)
            continue
        if stripped.startswith("#"):
            flush()
            continue

        if buf_line is None:
            buf_line = lineno
        buf.append(stripped)
        if stripped.endswith(";"):
            flush()
            pending = "ok"

    flush()
    return passed, failed, failures


def main():
    args = sys.argv[1:]
    bin_path = None
    files = []
    i = 0
    while i < len(args):
        if args[i] == "--bin":
            bin_path = args[i + 1]
            i += 2
        elif args[i].startswith("--"):
            print(f"error: unknown option {args[i]}")
            return 2
        else:
            files.append(args[i])
            i += 1

    if not files:
        files = sorted(
            glob.glob(os.path.join(os.path.dirname(__file__), "sql_corpus", "*.sql"))
        )
    if not files:
        print("error: no corpus files found (pass paths or use tools/sql_corpus/)")
        return 2

    resolved = find_binary(bin_path)
    if not os.path.exists(resolved):
        print(f"error: extension binary not found: {resolved}")
        return 2

    # Fresh session per run: hermetic data dir, one shared DB across files
    tmp = tempfile.mkdtemp(prefix="a3sql_compat_")
    cfg = os.path.join(tmp, "a3sql.toml")
    with open(cfg, "w") as f:
        f.write(f'data_dir = "{tmp}/data"\n')
    os.environ["A3SQL_CONFIG"] = cfg
    lib = load_lib(resolved)

    total_p = total_f = 0
    print(f"a3sql compatibility report — binary: {resolved}\n")
    for path in files:
        p, f_, failures = run_file(lib, path)
        total_p += p
        total_f += f_
        domain = os.path.basename(path).replace(".sql", "")
        status = "PASS" if f_ == 0 else f"FAIL ({f_} of {p + f_})"
        print(f"  [{status}] {domain}: {p} passed, {f_} failed")
        for lineno, stmt, resp in failures:
            print(f"      line {lineno}: {stmt}")
            print(f"        -> {resp}")

    shutil.rmtree(tmp, ignore_errors=True)
    print(f"\nTOTAL: {total_p} passed, {total_f} failed")
    if total_f == 0:
        print("All realistic workloads run clean. Your mod's SQL will likely work —")
        print("but still run your own queries through this report before shipping.")
    else:
        print("Gaps found. See failures above — fix your SQL or report the gap.")
    return 1 if total_f else 0


if __name__ == "__main__":
    sys.exit(main())
