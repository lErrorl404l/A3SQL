#!/usr/bin/env python3
"""Pre-commit guard: no stale repo-metrics in the docs.

Fails if any markdown in README.md, docs/, or docs/wiki/ states a
repo-metric as a hard number — test counts, dialect-sweep counts,
MSRV, or version strings. These go stale the moment code changes;
docs should reference live sources instead (CI badges, Cargo.toml's
rust-version, the sweep/test commands without counts).

Deliberately NOT flagged (product defaults users need, not repo
metrics):
  - ports / bind addresses (33306, 127.0.0.1)
  - hard limits (30KB output cap, 128 nesting depth, 100-iteration CTE cap)
  - byte/size numbers in worked examples (a 50KB value)
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# repo-metric patterns: "116 checks", "700+ tests", "517 library tests",
# "524 tests", "522 unit + 206 integration", "116/116 pass", "Rust 1.88",
# "MSRV 1.88", "edition 2024", "v0.2.0"-style version strings
PATTERNS = [
    (re.compile(r"\b\d+\+?\s+(test|tests|check|checks)\b"), "test/check count"),
    (re.compile(r"\b\d+\s*(?:lib|unit|integration)?\s*tests?\b"), "test count"),
    (re.compile(r"\b\d+/\d+\s+pass\b"), "pass/total ratio"),
    (re.compile(r"\bRust\s+1\.\d+"), "Rust MSRV"),
    (re.compile(r"\bMSRV\b", re.IGNORECASE), "MSRV"),
    (re.compile(r"\bedition\s+20\d\d\b"), "Rust edition"),
    # version strings: "v0.2.0" or "x 0.2.0" — exactly THREE dotted groups
    # (major.minor.patch). NOT IP addresses (four octets, 127.0.0.1) — the
    # (?!\.) after the third group excludes them.
    (re.compile(r"(?<=[A-Za-z])\s*v?\d+\.\d+\.\d+(?!\.)"), "version string"),
]

FILES = [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md"))]


def main() -> int:
    bad = []
    for path in FILES:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for rx, what in PATTERNS:
                if rx.search(line):
                    bad.append(
                        f"{path.relative_to(ROOT)}:{lineno}: {what}: {line.strip()}"
                    )
    if bad:
        print(
            "Static repo-metrics found in docs (these go stale — reference live sources instead):"
        )
        for b in bad:
            print(f"  {b}")
        print(
            "\nRemove the hard numbers, or adjust the pattern in tools/check_docs_static.py if it's a product default."
        )
        return 1
    print("docs static-metric check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
