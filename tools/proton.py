"""
Proton/AEE tooling — Steam library discovery, Arma 3 path helpers, Wine↔Linux path translation.

Usage:
    from tools.proton import find_arma3, find_sqfvm, wine_to_linux, linux_to_wine

    a3 = find_arma3()
    print(a3["path"])           # /ext/SteamLibrary/steamapps/common/Arma 3
    print(a3["compatdata"])     # /ext/SteamLibrary/steamapps/compatdata/107410
    print(a3["proton"])         # /home/matt/.local/share/Steam/compatibilitytools.d/CachyOS-11.0-100/proton

CLI:
    python -m tools.proton        # print full report
    python -m tools.proton --path # just the Arma 3 path
"""

from __future__ import annotations

import os
import re
import shutil
import sys
from pathlib import Path
from typing import Any

ARMA3_APP_ID = "107410"

# ── VDF parser (minimal, just what we need) ──────────────────────────


def _parse_vdf(text: str) -> dict[str, Any]:
    """Parse a simple VDF KeyValues document into nested dicts/lists."""
    stack: list[dict | list] = [{}]
    key: str | None = None
    # strip comments
    text = re.sub(r"//.*", "", text)
    # tokenize
    tokens = re.findall(r'"[^"\\]*(?:\\.[^"\\]*)*"|[\[\]{}]', text)

    for tok in tokens:
        if tok in ("{", "}"):
            if tok == "{":
                new: dict | list = {} if key else []
                container = stack[-1]
                if isinstance(container, dict) and key is not None:
                    container[key] = new
                elif isinstance(container, list):
                    container.append(new)
                stack.append(new)
                key = None
            else:  # "}"
                stack.pop()
            continue

        val = tok.strip('"')
        container = stack[-1]

        if isinstance(container, dict):
            if key is None:
                key = val
            else:
                container[key] = val
                key = None
        elif isinstance(container, list):
            container.append(val)

    return stack[0]  # type: ignore[return-value]


# ── Steam library discovery ──────────────────────────────────────────


def _find_config_vdf() -> Path | None:
    """Locate Steam's config/libraryfolders.vdf across standard locations."""
    candidates = [
        Path.home() / ".local/share/Steam/config/libraryfolders.vdf",
        Path.home() / ".steam/steam/config/libraryfolders.vdf",
        Path.home() / ".steam/root/config/libraryfolders.vdf",
    ]
    for p in candidates:
        if p.is_file():
            return p
    return None


def find_library_folders() -> list[dict[str, Any]]:
    """Return list of Steam library folders with their paths and apps."""
    vdf_path = _find_config_vdf()
    if vdf_path is None:
        return []

    raw = vdf_path.read_text(encoding="utf-8")
    parsed = _parse_vdf(raw)
    folders_raw = parsed.get("libraryfolders", {})

    folders: list[dict[str, Any]] = []
    for key in sorted(folders_raw.keys(), key=lambda k: int(k) if k.isdigit() else 0):
        entry = folders_raw[key]
        if isinstance(entry, dict) and "path" in entry:
            folders.append(entry)
    return folders


# ── Arma 3 discovery ─────────────────────────────────────────────────


def find_arma3() -> dict[str, str] | None:
    """Locate Arma 3 installation via Steam libraries.

    Returns dict with keys: path, compatdata, steam_library, app_id
    or None if not found.
    """
    folders = find_library_folders()
    for folder in folders:
        apps = folder.get("apps", {})
        if not isinstance(apps, dict):
            continue
        if ARMA3_APP_ID not in apps:
            continue
        lib_path = Path(folder["path"])
        a3_path = lib_path / "steamapps/common/Arma 3"
        compatdata = lib_path / "steamapps/compatdata" / ARMA3_APP_ID
        result: dict[str, str] = {
            "path": str(a3_path.resolve()),
            "compatdata": str(compatdata.resolve()) if compatdata.is_dir() else "",
            "steam_library": str(lib_path.resolve()),
            "app_id": ARMA3_APP_ID,
        }
        if not a3_path.is_dir():
            continue
        # try getting config version
        version_file = compatdata / "version"
        if version_file.is_file():
            result["proton_version"] = version_file.read_text(encoding="utf-8").strip()
        return result
    return None


# ── Proton discovery ─────────────────────────────────────────────────


def find_proton_binary() -> str | None:
    """Find a usable Proton binary (compatibility tools or system wine)."""
    search_dirs = [
        Path.home() / ".local/share/Steam/compatibilitytools.d",
        Path.home() / ".steam/root/compatibilitytools.d",
        Path.home() / ".steam/steam/compatibilitytools.d",
    ]
    for d in search_dirs:
        if not d.is_dir():
            continue
        # prefer newest proton
        entries = sorted(d.iterdir(), reverse=True)
        for entry in entries:
            proton_bin = entry / "proton"
            if proton_bin.is_file():
                return str(proton_bin.resolve())
            # also check for "proton" in subdirs
            for sub in entry.iterdir():
                if sub.name == "proton" and sub.is_file():
                    return str(sub.resolve())

    # fallback to system wine
    system_wine = shutil.which("wine")
    if system_wine:
        return system_wine
    return None


# ── SQFVM discovery ──────────────────────────────────────────────────


def find_sqfvm() -> str | None:
    """Locate sqfvm binary."""
    # common locations
    candidates = [
        Path.home() / ".local/bin/sqfvm.bin",
        Path.home() / ".local/bin/sqfvm",
        "/usr/local/bin/sqfvm.bin",
        "/usr/local/bin/sqfvm",
        "/usr/bin/sqfvm.bin",
        "/usr/bin/sqfvm",
    ]
    for c in candidates:
        if c.is_file():
            return str(c.resolve())

    # PATH fallback
    found = shutil.which("sqfvm") or shutil.which("sqfvm.bin")
    if found:
        return found
    return None


# ── Path translation ─────────────────────────────────────────────────


def linux_to_wine(path: str) -> str:
    """Convert a Linux absolute path to a Wine Z:\\ style path.

    >>> linux_to_wine("/ext/SteamLibrary/game")
    "Z:\\\\ext\\\\SteamLibrary\\\\game"
    """
    if not path.startswith("/"):
        path = os.path.abspath(path)
    return "Z:" + path.replace("/", "\\\\")


def wine_to_linux(wine_path: str) -> str:
    """Convert a Wine Z:\\ style path back to a Linux path.

    >>> wine_to_linux("Z:\\\\ext\\\\SteamLibrary\\\\game")
    "/ext/SteamLibrary/game"
    """
    wine_path = wine_path.strip().replace("\\", "/")
    if wine_path.startswith("Z:") or wine_path.startswith("z:"):
        wine_path = wine_path[2:]
    # handle drive letters other than Z: (e.g. C: for compatdata pfx)
    if len(wine_path) > 1 and wine_path[1] == ":" and wine_path[0].isalpha():
        # C:\\users\\matt -> need pfx/drive_c mapping
        return wine_path  # return as-is, can't resolve without pfx context
    if wine_path.startswith("/"):
        return wine_path
    return "/" + wine_path


def arma3_wine_prefix() -> str | None:
    """Return the path to the Arma 3 Proton/Wine prefix (pfx directory)."""
    a3 = find_arma3()
    if a3 and a3["compatdata"]:
        pfx = Path(a3["compatdata"]) / "pfx"
        if pfx.is_dir():
            return str(pfx.resolve())
    return None


def resolve_workshop_path() -> str | None:
    """Return the Arma 3 workshop content path."""
    a3 = find_arma3()
    if a3:
        workshop = Path(a3["steam_library"]) / "steamapps/workshop/content/107410"
        if workshop.is_dir():
            return str(workshop.resolve())
    return None


# ── CLI ──────────────────────────────────────────────────────────────


def report() -> str:
    """Generate a human-readable report of all discovered paths."""
    lines: list[str] = ["=== AEE Proton Environment ==="]

    a3 = find_arma3()
    if a3:
        lines.append(f"  Arma 3:          {a3['path']}")
        lines.append(f"  Compatdata:      {a3['compatdata'] or '(not a directory)'}")
        if "proton_version" in a3:
            lines.append(f"  Proton version:  {a3['proton_version']}")
    else:
        lines.append("  Arma 3:          NOT FOUND")

    proton = find_proton_binary()
    lines.append(f"  Proton/Wine:     {proton or 'NOT FOUND'}")

    sqfvm = find_sqfvm()
    lines.append(f"  SQFVM:           {sqfvm or 'NOT FOUND'}")

    pfx = arma3_wine_prefix()
    lines.append(f"  Wine prefix:     {pfx or 'N/A'}")

    ws = resolve_workshop_path()
    lines.append(f"  Workshop:        {ws or 'N/A'}")

    lines.append(f"  Libraries:")
    for lib in find_library_folders():
        lines.append(f"    - {lib['path']}")
    return "\n".join(lines)


if __name__ == "__main__":
    if "--path" in sys.argv:
        a3 = find_arma3()
        if a3:
            print(a3["path"])
        else:
            print("Arma 3 not found", file=sys.stderr)
            sys.exit(1)
    elif "--vdf" in sys.argv:
        vdf = _find_config_vdf()
        print(vdf or "VDF not found")
    else:
        print(report())
