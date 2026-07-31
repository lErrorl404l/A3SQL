#!/usr/bin/env python3
"""
SQL smoke test — validates SQL against the real a3sql extension binary.

Runs a script of SQL statements through the exact C ABI Arma uses
(RVExtension / STRING form) and checks each response. Catches engine
incompatibilities (unsupported syntax, wrong column types, constraint
behaviour) before a mod ships, without needing Arma.

Usage:
  python3 tools/sql_smoke_test.py [--bin PATH] script.sql [script2.sql ...]

  --bin PATH   Extension binary to test against. Auto-detected in this order:
               --bin flag, $A3SQL_BIN, extension/target/release/liba3sql.so,
               ./a3sql.so, ./a3sql_x64.so. On Windows try the .dll names.
  script.sql   Text file of statements, one per line. Blank lines and lines
               starting with '#' are ignored.

Script format (assertions):
  CREATE TABLE t (...)            Statement must return [0,"OK"
  # expect error                  NEXT statement must NOT return [0,"OK"
  INSERT ...                      (expected to fail, e.g. UNIQUE violation)
  # expect contains "substring"   NEXT statement's response must contain it

The tool runs in a hermetic data dir (temp dir via A3SQL_CONFIG), so
save/load statements never touch ./a3sql_data.

Exit code: 0 all passed, 1 any failed, 2 usage error.
"""

import ctypes
import os
import shutil
import sys
import tempfile

BUF_SIZE = 20480  # must match OUTPUT_BUF_SIZE in the extension


def find_binary(explicit):
    if explicit:
        return explicit
    env = os.environ.get("A3SQL_BIN")
    if env:
        return env
    candidates = [
        "extension/target/release/liba3sql.so",
        "a3sql_x64.so",
        "a3sql.so",
    ]
    if sys.platform.startswith("win"):
        candidates = [
            "extension/target/release/a3sql.dll",
            "a3sql_x64.dll",
            "a3sql.dll",
        ]
    for c in candidates:
        if os.path.exists(c):
            return c
    return candidates[0]


def make_hermetic_config():
    """Point the extension's data dir at a temp dir via A3SQL_CONFIG."""
    d = tempfile.mkdtemp(prefix="a3sql_smoke_")
    cfg = os.path.join(d, "a3sql.toml")
    with open(cfg, "w") as f:
        f.write(f'data_dir = "{d}/data"\n')
    return cfg, d


def load_lib(path):
    lib = ctypes.CDLL(path)
    lib.RVExtension.argtypes = [ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p]
    lib.RVExtension.restype = None
    return lib


def call(lib, statement):
    """STRING-form call — identical to `"a3sql" callExtension stmt` in SQF."""
    buf = ctypes.create_string_buffer(BUF_SIZE)
    lib.RVExtension(buf, BUF_SIZE, statement.encode("utf-8", "replace"))
    return buf.value.decode("utf-8", "replace")


def run_script(lib, path):
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()

    passed = failed = 0
    pending = "ok"  # "ok" | "error" | ("contains", text)

    for lineno, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("# expect "):
            if stripped.startswith("# expect error"):
                pending = "error"
            elif stripped.startswith("# expect contains "):
                text = stripped[len("# expect contains ") :].strip()
                if len(text) >= 2 and text[0] == text[-1] and text[0] in "'\"":
                    text = text[1:-1]
                pending = ("contains", text)
            continue
        if stripped.startswith("#"):
            continue

        response = call(lib, stripped)
        if pending == "error":
            ok = not response.startswith('[0,"OK"')
            pending = "ok"
        elif isinstance(pending, tuple):
            ok = pending[1] in response
            pending = "ok"
        else:
            ok = response.startswith('[0,"OK"')

        status = "PASS" if ok else "FAIL"
        if ok:
            passed += 1
        else:
            failed += 1
        print(f"  {status}  {path}:{lineno}  {stripped[:60]}")
        if not ok:
            print(f"         response: {response[:200]}")

    if pending != "ok":
        print(f"  WARN  {path}: trailing assertion with no following statement")
    return passed, failed


def main():
    args = [a for a in sys.argv[1:]]
    bin_path = None
    scripts = []
    i = 0
    while i < len(args):
        if args[i] == "--bin":
            if i + 1 >= len(args):
                print("error: --bin requires a path")
                return 2
            bin_path = args[i + 1]
            i += 2
        elif args[i].startswith("--"):
            print(f"error: unknown option {args[i]}")
            return 2
        else:
            scripts.append(args[i])
            i += 1

    if not scripts:
        print(__doc__)
        return 2
    for s in scripts:
        if not os.path.exists(s):
            print(f"error: script not found: {s}")
            return 2

    resolved = find_binary(bin_path)
    if not os.path.exists(resolved):
        print(f"error: extension binary not found: {resolved}")
        print("Pass --bin PATH or build first: cargo build --release")
        return 2

    cfg_file, cfg_dir = make_hermetic_config()
    os.environ["A3SQL_CONFIG"] = cfg_file
    lib = load_lib(resolved)

    total_p = total_f = 0
    for s in scripts:
        p, f = run_script(lib, s)
        total_p += p
        total_f += f

    shutil.rmtree(cfg_dir, ignore_errors=True)
    print(
        f"\nSQL smoke test {'PASSED' if total_f == 0 else 'FAILED'}: "
        f"{total_p} passed, {total_f} failed (binary: {resolved})"
    )
    return 1 if total_f else 0


if __name__ == "__main__":
    sys.exit(main())
