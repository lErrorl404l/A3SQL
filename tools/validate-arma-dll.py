#!/usr/bin/env python3
"""Validate extension DLLs for Arma 3 compatibility.

Usage: python3 tools/validate-arma-dll.py <mod-root>
       python3 tools/validate-arma-dll.py ~/Downloads/a3sql/@a3sql

Checks PE headers, exports, dependencies, and file layout.
Exit code 0 = all checks passed, 1 = issues found.
"""

import struct
import sys
import os
import glob


def read_cstring(data, offset):
    end = data.index(b"\x00", offset)
    return data[offset:end].decode("ascii", errors="replace")


def rva_to_offset(rva, sections):
    for vaddr, raw_size, raw_ptr in sections:
        if vaddr <= rva < vaddr + raw_size:
            return rva - vaddr + raw_ptr
    return None


def validate_dll(path):
    issues = []
    warnings = []

    with open(path, "rb") as f:
        data = f.read()

    # MZ header
    if data[:2] != b"MZ":
        return [f"FAIL: {os.path.basename(path)}: No MZ header (not a PE file)"]

    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if pe_offset + 4 > len(data) or data[pe_offset : pe_offset + 4] != b"PE\x00\x00":
        return [f"FAIL: {os.path.basename(path)}: Invalid PE signature"]

    coff = pe_offset + 4
    machine = struct.unpack_from("<H", data, coff)[0]
    characteristics = struct.unpack_from("<H", data, coff + 18)[0]
    opt_size = struct.unpack_from("<H", data, coff + 16)[0]
    num_sections = struct.unpack_from("<H", data, coff + 2)[0]

    opt = coff + 20
    magic = struct.unpack_from("<H", data, opt)[0]

    is_pe32plus = magic == 0x20B
    is_pe32 = magic == 0x10B

    if not is_pe32 and not is_pe32plus:
        issues.append(f"FAIL: Unknown PE magic 0x{magic:04x}")

    # Machine check
    if machine == 0x14C:
        arch = "x86"
    elif machine == 0x8664:
        arch = "x86_64"
    else:
        arch = f"UNKNOWN(0x{machine:04x})"
        issues.append(f"FAIL: Unknown machine type 0x{machine:04x}")

    # DLL characteristics at opt+70
    dll_chars = struct.unpack_from("<H", data, opt + 70)[0]
    subsystem = struct.unpack_from("<H", data, opt + 68)[0]

    # IMAGE_FILE_DLL
    if not (characteristics & 0x2000):
        issues.append("FAIL: IMAGE_FILE_DLL flag not set")

    # NX_COMPAT (required for Arma 3 security)
    if not (dll_chars & 0x0100):
        warnings.append("WARN: NX_COMPAT (DEP) not set")

    # DYNAMIC_BASE (ASLR)
    if not (dll_chars & 0x0040):
        warnings.append("WARN: DYNAMIC_BASE (ASLR) not set")

    # HIGH_ENTROPY_VA for 64-bit
    if is_pe32plus and not (dll_chars & 0x0020):
        warnings.append("WARN: HIGH_ENTROPY_VA not set on 64-bit")

    # Parse sections
    sec_start = opt + opt_size
    sections = []
    for i in range(num_sections):
        off = sec_start + i * 40
        vsize = struct.unpack_from("<I", data, off + 8)[0]
        vaddr = struct.unpack_from("<I", data, off + 12)[0]
        raw_size = struct.unpack_from("<I", data, off + 16)[0]
        raw_ptr = struct.unpack_from("<I", data, off + 20)[0]
        sections.append((vaddr, raw_size, raw_ptr))

    # Parse exports
    if is_pe32:
        num_rva = struct.unpack_from("<I", data, opt + 92)[0]
        rva_start = opt + 96
    else:
        num_rva = struct.unpack_from("<I", data, opt + 108)[0]
        rva_start = opt + 112

    export_names = set()
    if num_rva > 0:
        exp_rva = struct.unpack_from("<I", data, rva_start)[0]
        exp_size = struct.unpack_from("<I", data, rva_start + 4)[0]
        if exp_rva > 0 and exp_size > 0:
            exp_off = rva_to_offset(exp_rva, sections)
            if exp_off:
                num_names = struct.unpack_from("<I", data, exp_off + 24)[0]
                num_funcs = struct.unpack_from("<I", data, exp_off + 20)[0]
                ordinal_base = struct.unpack_from("<I", data, exp_off + 16)[0]
                name_ptr_rva = struct.unpack_from("<I", data, exp_off + 32)[0]

                # DLL name
                exp_name_rva = struct.unpack_from("<I", data, exp_off + 12)[0]
                if exp_name_rva:
                    n_off = rva_to_offset(exp_name_rva, sections)
                    if n_off:
                        dll_name_in_export = read_cstring(data, n_off)
                        base = os.path.basename(path)
                        if dll_name_in_export.lower() != base.lower():
                            warnings.append(
                                f'WARN: Export DLL name "{dll_name_in_export}" != file name "{base}"'
                            )

                # Named exports
                if num_names > 0 and name_ptr_rva:
                    np_off = rva_to_offset(name_ptr_rva, sections)
                    if np_off:
                        for ni in range(num_names):
                            n_rva = struct.unpack_from("<I", data, np_off + ni * 4)[0]
                            n_off = rva_to_offset(n_rva, sections)
                            if n_off:
                                export_names.add(read_cstring(data, n_off))

    # Check Arma 3 required exports
    arma_required = ["RVExtension", "RVExtensionArgs", "RVExtensionVersion"]
    arma_optional = ["RVExtensionRegisterCallback", "RVExtensionFeatureFlags"]
    for e in arma_required:
        if e not in export_names:
            issues.append(f"FAIL: Missing required Arma export: {e}")
    for e in arma_optional:
        if e not in export_names:
            warnings.append(f"WARN: Missing optional Arma export: {e}")

    # Check for .so files (Linux ELF) — Arma 3 Publisher rejects these
    # (checked at directory level, not per-DLL)

    # Print report
    basename = os.path.basename(path)
    print(
        f"  {basename}: {arch} | PE{'32+' if is_pe32plus else '32'} | {num_sections} sections | {len(export_names)} exports"
    )
    print(f"    DllCharacteristics: 0x{dll_chars:04x} | Subsystem: 0x{subsystem:04x}")

    return issues, warnings


def validate_mod_folder(mod_root):
    all_issues = []
    all_warnings = []

    if not os.path.isdir(mod_root):
        print(f"ERROR: {mod_root} is not a directory")
        return 1

    print(f"Mod root: {mod_root}")
    print()

    # Check for .so / Linux files
    so_files = glob.glob(os.path.join(mod_root, "**", "*.so"), recursive=True)
    so_files += glob.glob(os.path.join(mod_root, "**", "liba3sql*"), recursive=True)
    if so_files:
        print("CRITICAL: Linux ELF files found — Publisher WILL reject these:")
        for f in so_files:
            print(f"  DELETE: {os.path.relpath(f, mod_root)}")
            all_issues.append(
                f"FAIL: Linux file in mod: {os.path.relpath(f, mod_root)}"
            )
        print()

    # Check for DLLs
    dll_files = glob.glob(os.path.join(mod_root, "*.dll"))
    dll_files += glob.glob(os.path.join(mod_root, "**", "*.dll"), recursive=True)
    dll_files = list(set(dll_files))  # dedupe

    if not dll_files:
        all_issues.append("FAIL: No DLL files found in mod folder")
    else:
        print(f"Extension DLLs found: {len(dll_files)}")
        for dll in sorted(dll_files):
            issues, warnings = validate_dll(dll)
            all_issues.extend(issues)
            all_warnings.extend(warnings)
        print()

    # Check mod.cpp
    mod_cpp = os.path.join(mod_root, "mod.cpp")
    if os.path.exists(mod_cpp):
        with open(mod_cpp) as f:
            content = f.read()
        if "version" not in content.lower():
            all_warnings.append("WARN: mod.cpp missing version field")
        print(f"  mod.cpp: present")
    else:
        all_warnings.append("WARN: mod.cpp not found")
        print(f"  mod.cpp: MISSING")

    # Check keys
    keys_dir = os.path.join(mod_root, "keys")
    bikeys = glob.glob(os.path.join(keys_dir, "*.bikey"))
    if bikeys:
        print(f"  keys/: {len(bikeys)} bikey(s)")
    else:
        all_warnings.append("WARN: No .bikey files in keys/")

    # Check addons
    addons_dir = os.path.join(mod_root, "addons")
    if os.path.isdir(addons_dir):
        pbos = glob.glob(os.path.join(addons_dir, "*.pbo"))
        print(f"  addons/: {len(pbos)} PBO(s)")
    else:
        all_issues.append("FAIL: addons/ directory not found")

    # Check for .bisign files
    bisigns = glob.glob(os.path.join(mod_root, "**", "*.bisign"), recursive=True)
    print(f"  Signature files: {len(bisigns)}")

    print()
    if all_warnings:
        print("WARNINGS:")
        for w in all_warnings:
            print(f"  {w}")
        print()

    if all_issues:
        print("ISSUES (Publisher will reject):")
        for i in all_issues:
            print(f"  {i}")
        print()
        print("RESULT: FAIL — fix issues before publishing")
        return 1
    else:
        print("RESULT: PASS — mod folder looks valid for Publisher")
        return 0


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <mod-root>")
        print(f"Example: {sys.argv[0]} ~/Downloads/a3sql/@a3sql")
        sys.exit(1)
    sys.exit(validate_mod_folder(sys.argv[1]))
