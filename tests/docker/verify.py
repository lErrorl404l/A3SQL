#!/usr/bin/env python3
"""Verify the A3SQL docker test log.

Reads a captured server log and reports the [PHASE] results. Exits 0 only
when every phase passed and the mission finished.
"""

import re
import sys

LOG = sys.argv[1] if len(sys.argv) > 1 else "tests/docker/run.log"

with open(LOG, encoding="utf-8", errors="replace") as f:
    text = f.read()

passes = re.findall(r"\[PHASE\d+\] (?:DONE: [^\n]*|\[PASS\][^\n]*)", text)
fails = re.findall(r"\[PHASE\d+\] \[FAIL\][^\n]*", text)
done = "[BALLISTICS] TEST END" in text or "=== A3SQL BALLISTICS TEST END ===" in text
errors = [l for l in text.splitlines() if "Error in expression" in l]

print(f"phases passed: {len(passes)}")
for p in passes:
    print(f"  {p}")
print(f"phases failed: {len(fails)}")
for f in fails:
    print(f"  {f}")
if not done:
    print("  mission did not reach END")
if errors:
    print(f"script errors: {len(errors)}")
    for e in errors[:10]:
        print(f"  {e}")

if fails or not done or errors:
    print("RESULT: FAIL")
    sys.exit(1)
print("RESULT: PASS")
