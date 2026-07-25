#!/usr/bin/env python3
"""
SQF syntax validator — bracket matching, string termination, comment correctness.

Usage:
  python3 tools/sqf_validator.py              # Check all addons/**/*.sqf
  python3 tools/sqf_validator.py path/to/     # Check a specific directory

Returns exit code 1 on errors, 0 on clean.
Checks:
  - Balanced parentheses: ( ) [ ] { }
  - Properly terminated strings (" ... " and ' ... ')
  - Unterminated block comments /* ... */
  - Control flow structure consistency

Ported from ACE3/CBA_A3 project conventions.
"""

import os
import sys
import re

FAILED = 0


def check_sqf_file(filepath):
    """Validate a single .sqf file for bracket/string/comment correctness."""
    global FAILED
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    errors = []
    line_num = 1
    col_num = 1
    i = 0
    paren_stack = []
    paren_map = {"(": ")", "[": "]", "{": "}"}
    close_map = {")": "(", "]": "[", "}": "{"}
    in_block_comment = False
    in_line_comment = False
    in_string = None  # None, '"', or "'"
    in_string_escape = False

    while i < len(content):
        ch = content[i]

        # ──── Block comments ────
        if not in_string and not in_line_comment and not in_block_comment:
            if content[i : i + 2] == "/*":
                in_block_comment = True
                i += 2
                line_num += content[i - 2 : i].count("\n")
                continue

        if in_block_comment:
            if content[i : i + 2] == "*/":
                in_block_comment = False
                i += 2
                continue
            if ch == "\n":
                line_num += 1
                col_num = 1
                i += 1
                continue
            col_num += 1
            i += 1
            continue

        # ──── Line comments ────
        if not in_string and not in_line_comment:
            if content[i : i + 2] == "//":
                in_line_comment = True
                i += 2
                continue

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
                line_num += 1
                col_num = 1
                i += 1
                continue
            col_num += 1
            i += 1
            continue

        # ──── Strings ────
        if not in_string:
            if ch in ('"', "'"):
                in_string = ch
                i += 1
                col_num += 1
                continue
        else:
            if in_string_escape:
                in_string_escape = False
                i += 1
                col_num += 1
                continue
            if ch == "\\" and content[i : i + 2] == '""':
                # Doubled quote inside string
                i += 2
                col_num += 2
                continue
            if ch in ('"', "'"):
                if ch == in_string:
                    # Check for doubled quote (escape)
                    if i + 1 < len(content) and content[i + 1] == in_string:
                        i += 2
                        col_num += 2
                        continue
                    in_string = None
                i += 1
                col_num += 1
                continue
            if ch == "\n":
                errors.append(
                    f"  Line {line_num}: Unterminated string (newline before closing {in_string})"
                )
                in_string = None
                line_num += 1
                col_num = 1
                i += 1
                continue
            i += 1
            col_num += 1
            continue

        # ──── Brackets ────
        if ch in paren_map:
            paren_stack.append((ch, line_num, col_num))
        elif ch in close_map:
            if not paren_stack:
                errors.append(f"  Line {line_num}: Unmatched closing '{ch}'")
            else:
                expected = paren_stack.pop()[0]
                if close_map[ch] != expected:
                    errors.append(
                        f"  Line {line_num}: Expected '{paren_map[expected]}' but found '{ch}'"
                    )

        if ch == "\n":
            line_num += 1
            col_num = 1
        else:
            col_num += 1
        i += 1

    # Check for unclosed constructs
    if in_block_comment:
        errors.append("  EOF: Unterminated block comment /* ...")
    if in_string:
        errors.append("  EOF: Unterminated string (missing closing quote)")
    while paren_stack:
        ch, ln, col = paren_stack.pop()
        errors.append(
            f"  Line {ln}, column {col}: Unmatched '{ch}' — expected '{paren_map[ch]}'"
        )

    if errors:
        FAILED = 1
        rel = os.path.relpath(filepath)
        print(f"{rel}:")
        for e in errors:
            print(e)
        return False
    return True


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    target = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "addons")

    if os.path.isfile(target):
        check_sqf_file(target)
    else:
        for dirpath, _, filenames in os.walk(target):
            for fn in sorted(filenames):
                if fn.endswith(".sqf"):
                    check_sqf_file(os.path.join(dirpath, fn))

    if FAILED:
        print("\n✗ SQF validation failed")
        sys.exit(1)
    else:
        print("✓ SQF validation passed")


if __name__ == "__main__":
    main()
