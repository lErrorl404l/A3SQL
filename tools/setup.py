#!/usr/bin/env python3
"""A3DB dev setup — file patching symlink + BI key generation.

Usage:
    python tools/setup.py               # auto-detect + symlink
    python tools/setup.py --report      # print env report
    python tools/setup.py --keys        # generate signing keys
"""

import os, sys, pathlib

PROJECT = pathlib.Path(__file__).resolve().parent.parent

try:
    from tools.proton import find_arma3, report as env_report
except ImportError:
    find_arma3 = lambda: None
    env_report = lambda: "proton.py not found"


def setup_file_patching(a3_path):
    dev = PROJECT / ".hemttout" / "dev"
    target = a3_path / "z" / "a3db"
    if not dev.is_dir():
        print(f"Run `hemtt build` first — no {dev}")
        return
    if target.is_symlink() or target.is_dir():
        print(f"Already set up: {target}")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(dev, target_is_directory=True)
    print(f"Symlink: {target} -> {dev}")


def generate_keys():
    d = PROJECT / "keys"
    d.mkdir(exist_ok=True)
    exe = "/ext/SteamLibrary/steamapps/common/Arma 3 Tools/DSSignFile/DSCreateKey.exe"
    k = d / "a3db"
    if k.with_suffix(".biprivatekey").exists():
        print(f"Keys exist: {k}")
        return
    if os.path.exists(exe):
        import subprocess as sp

        sp.run(["wine64", exe, str(k)], check=True)
        print(f"Keys: {k}.bikey + .biprivatekey")
    else:
        print("Arma 3 Tools not found — skipping keys")


def main():
    a3 = find_arma3()
    if a3:
        setup_file_patching(pathlib.Path(a3["path"]))
    else:
        print("Arma 3 not found (try --report)")
    if "--keys" in sys.argv:
        generate_keys()


if __name__ == "__main__":
    if "--report" in sys.argv:
        print(env_report())
    else:
        main()
