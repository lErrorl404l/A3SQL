#!/usr/bin/env python3
"""A3DB development environment setup.

Creates junction/symlink so Arma 3 can find the addon source for file patching:

    <Arma 3>\\z\\a3db  →  <project>\\.hemttout\\dev

Usage:
    python tools/setup.py                   # auto-detect Steam path
    python tools/setup.py "C:/Arma 3"       # explicit path
"""

import os, sys, platform, pathlib

PROJECT = pathlib.Path(__file__).resolve().parent.parent
ARMA3_PATH = None


def find_steam_arma3() -> pathlib.Path | None:
    """Try to find Arma 3 via Steam library."""
    if platform.system() == "Windows":
        candidates = [
            "C:/Program Files (x86)/Steam/steamapps/common/Arma 3",
            "D:/SteamLibrary/steamapps/common/Arma 3",
        ]
    else:
        candidates = [
            pathlib.Path.home() / ".steam/steam/steamapps/common/Arma 3",
            pathlib.Path.home() / ".local/share/Steam/steamapps/common/Arma 3",
        ]
    for c in candidates:
        p = pathlib.Path(c)
        if p.is_dir():
            return p
    return None


def setup(source: pathlib.Path, target: pathlib.Path) -> None:
    """Create symlink or junction from target -> source."""
    if target.is_symlink() or target.is_dir():
        print(f"Already exists: {target}")
        return

    print(f"Creating link: {target} -> {source}")

    if platform.system() == "Windows":
        parent = target.parent
        parent.mkdir(parents=True, exist_ok=True)
        os.system(f'mklink /J "{target}" "{source}"')
    else:
        target.symlink_to(source, target_is_directory=True)

    print("Done")


def main():
    arma3 = ARMA3_PATH or find_steam_arma3()
    if not arma3:
        print("Arma 3 not found. Pass the path as argument:")
        print(
            f'  python {sys.argv[0]} "C:/Program Files (x86)/Steam/steamapps/common/Arma 3"'
        )
        sys.exit(1)

    # Link .hemttout/dev -> <Arma3>/z/a3db
    hemttout_dev = PROJECT / ".hemttout" / "dev"
    target = arma3 / "z" / "a3db"

    if not hemttout_dev.is_dir():
        print(f"Build output not found at {hemttout_dev}. Run `hemtt build` first.")
        sys.exit(1)

    setup(hemttout_dev, target)


if __name__ == "__main__":
    main()
