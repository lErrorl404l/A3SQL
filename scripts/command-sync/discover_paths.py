#!/usr/bin/env python3
"""Discover SteamCMD and Arma 3 DS paths via Steam library VDF.

Outputs JSON with detected paths for use by sync_local.sh.

Usage:
    python3 scripts/command-sync/discover_paths.py

Output:
    {"steamcmd": "/usr/bin/steamcmd", "arma3": "/path/to/arma3"}
    Keys are null when not found.
"""

import json
import os
import shutil
import sys
from pathlib import Path

# Import from the repo's tools module
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.proton import find_library_folders

ARMA3_DS_APP_ID = "233780"


def find_steamcmd() -> str | None:
    """Find steamcmd binary."""
    # Check PATH first
    which = shutil.which("steamcmd")
    if which:
        return which
    # Check common locations
    for p in [
        Path.home() / ".steam/steam/steamcmd.sh",
        Path.home() / ".local/share/Steam/steamcmd.sh",
        "/usr/bin/steamcmd",
        "/usr/games/steamcmd",
    ]:
        if p.is_file() or p.is_symlink():
            return str(p)
    # Check Steam library folders
    for lib in find_library_folders():
        path = Path(lib["path"]) / "steamcmd"
        if (path / "steamcmd.sh").is_file():
            return str(path / "steamcmd.sh")
        steamapps = Path(lib["path"]) / "steamapps/common/SteamCMD"
        if (steamapps / "steamcmd.sh").is_file():
            return str(steamapps / "steamcmd.sh")
    return None


def find_arma3_ds() -> str | None:
    """Find Arma 3 Dedicated Server installation."""
    for lib in find_library_folders():
        apps = lib.get("apps", {})
        if str(ARMA3_DS_APP_ID) in apps:
            path = Path(lib["path"]) / f"steamapps/common/Arma 3 Dedicated Server"
            if path.is_dir():
                return str(path)
            # Try alternative common names
            path2 = Path(lib["path"]) / "steamapps/common/Arma 3 Server"
            if path2.is_dir():
                return str(path2)
    return None


def find_default_output() -> str:
    """Default output directory (user-writable tmp)."""
    return "/tmp/sync_output"


if __name__ == "__main__":
    result = {
        "steamcmd": find_steamcmd(),
        "arma3": find_arma3_ds(),
        "output": find_default_output(),
    }
    print(json.dumps(result, indent=2))
