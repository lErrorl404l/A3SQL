#!/usr/bin/env python3
"""
Arma 3 supportInfo to JSON converter.

Reads the raw text output of Arma 3's ``supportInfo ""`` command (from a file
or stdin) and writes structured JSON matching the CmdInfo shape expected by
``extension/src/engine/sqf/database.rs``.

Usage
-----
    python3 support_info_to_json.py < support_info.txt > commands.json
    python3 support_info_to_json.py --file support_info.txt --output commands.json

Format handled
--------------
Lines starting with  ``n:``  ``u:``  ``b:``  introduce a command:

    n:commandName          # nular (0 arg)
    u:commandName          # unary (1 arg)
    b:commandName          # binary (2 args)

Subsequent indented lines are continuation (description, type info):

    u:sqrt
        Type: Number
        Description: Square root

Unknown arity prefix → skipped with a warning.
Deprecated entries (containing "deprecated" in description) are flagged.

Output
------
JSON array of objects:

    [{"name": "sqrt", "arity": "unary", "ret": "Number", "groups": ["Engine"]}]

``ret`` defaults to ``"Other"`` when not specified.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from typing import TextIO


# ── Arity map ──────────────────────────────────────────────────────────────
ARITY_PREFIX: dict[str, str] = {
    "n": "nular",
    "u": "unary",
    "b": "binary",
}

# ── Return type classification ────────────────────────────────────────────
# Matches common type names in supportInfo dumps. Unknown → "Other".
KNOWN_TYPES: dict[str, str] = {
    "number": "Number",
    "scalar": "Number",
    "integer": "Number",
    "float": "Number",
    "string": "String",
    "text": "String",
    "structured text": "String",
    "boolean": "Boolean",
    "bool": "Boolean",
    "array": "Array",
    "code": "Other",
    "config": "Other",
    "control": "Other",
    "display": "Other",
    "object": "Other",
    "side": "Other",
    "group": "Other",
    "unit": "Other",
    "location": "Other",
    "task": "Other",
    "namespace": "Other",
    "nothing": "Nothing",
    "any": "Other",
}


def normalize_type(raw: str) -> str:
    """Map a supportInfo type string to our ReturnType classification."""
    key = raw.strip().rstrip(".").lower()
    # "Number" and "String" are the primary ones we care about
    return KNOWN_TYPES.get(key, "Other")


def parse_support_info(text: str) -> list[dict]:
    """Parse raw supportInfo text into a list of command metadata dicts."""
    commands: list[dict] = []
    lines = text.splitlines()

    current: dict | None = None
    continuation: list[str] = []

    for lineno, raw_line in enumerate(lines, start=1):
        line = raw_line.rstrip("\n\r")

        # Skip empty lines
        if not line.strip():
            continue

        # Check for arity-prefixed command: n:, u:, b:
        match = re.match(r"^([nub]):(.+)$", line)
        if match:
            # Flush previous entry
            if current is not None:
                _finalize_entry(current, continuation, commands)
                continuation = []

            prefix = match.group(1)
            name = match.group(2).strip().lower()

            # Skip empty names or comments
            if not name or name.startswith("#") or name.startswith("//"):
                current = None
                continue

            current = {
                "name": name,
                "arity": ARITY_PREFIX[prefix],
                "ret": "Other",  # default; updated from Type: line
                "groups": ["Engine"],
            }
            continue

        # Continuation lines (indented or following a command entry)
        if current is not None:
            continuation.append(line)
            continue

        # Non-indented line that isn't a command prefix → end of command block
        if current is not None:
            _finalize_entry(current, continuation, commands)
            current = None
            continuation = []

    # Flush last entry
    if current is not None:
        _finalize_entry(current, continuation, commands)

    return commands


def _finalize_entry(
    entry: dict,
    continuation: list[str],
    commands: list[dict],
) -> None:
    """Extract metadata from continuation lines and append to commands."""
    for cl in continuation:
        cl_stripped = cl.strip()

        # Extract return type: "Type: Number", "Returns: String", etc.
        type_match = re.match(
            r"(?:Type|Returns|Return type|Result)\s*:\s*(.+)",
            cl_stripped,
            re.IGNORECASE,
        )
        if type_match:
            raw_type = type_match.group(1).strip()
            entry["ret"] = normalize_type(raw_type)
            continue

        # Mark deprecated
        if re.search(r"deprecated", cl_stripped, re.IGNORECASE):
            entry.setdefault("flags", []).append("deprecated")

    commands.append(entry)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Parse Arma 3 supportInfo dump to structured JSON",
    )
    parser.add_argument(
        "--file",
        "-f",
        type=str,
        help="Read from FILE instead of stdin",
    )
    parser.add_argument(
        "--output",
        "-o",
        type=str,
        help="Write JSON to FILE instead of stdout",
    )
    parser.add_argument(
        "--pretty",
        "-p",
        action="store_true",
        default=False,
        help="Pretty-print JSON output",
    )
    parser.add_argument(
        "--dedup",
        action="store_true",
        default=True,
        help="Deduplicate by name (last arity wins) [default: on]",
    )
    return parser.parse_args(argv)


def dedup_commands(commands: list[dict]) -> list[dict]:
    """Deduplicate by name — last occurrence wins (most specific arity)."""
    seen: dict[str, dict] = {}
    for cmd in commands:
        seen[cmd["name"]] = cmd
    return list(seen.values())


def main() -> None:
    args = parse_args()

    # Read input
    source: TextIO
    if args.file:
        with open(args.file, "r", encoding="utf-8", errors="replace") as source:
            text = source.read()
    else:
        text = sys.stdin.read()

    if not text.strip():
        print("warning: empty input — no commands parsed", file=sys.stderr)
        sys.exit(0)

    # Parse
    commands = parse_support_info(text)
    if not commands:
        print(
            "warning: no commands found — is this valid supportInfo output?\n"
            "Expected lines starting with n:, u:, or b:.",
            file=sys.stderr,
        )
        sys.exit(0)

    if args.dedup:
        before = len(commands)
        commands = dedup_commands(commands)
        after = len(commands)
        if before != after:
            print(
                f"info: deduplicated {before - after} duplicate names ({after} unique)",
                file=sys.stderr,
            )

    # Emit stats to stderr
    arities: dict[str, int] = {}
    for cmd in commands:
        arities[cmd["arity"]] = arities.get(cmd["arity"], 0) + 1
    print(
        f"info: parsed {len(commands)} commands "
        f"(nular={arities.get('nular', 0)}, "
        f"unary={arities.get('unary', 0)}, "
        f"binary={arities.get('binary', 0)})",
        file=sys.stderr,
    )

    # Write JSON
    indent = 2 if args.pretty else None
    output = json.dumps(commands, indent=indent, sort_keys=False)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as out:
            out.write(output)
            out.write("\n")
        print(f"info: wrote {args.output}", file=sys.stderr)
    else:
        sys.stdout.write(output)
        sys.stdout.write("\n")


if __name__ == "__main__":
    main()
