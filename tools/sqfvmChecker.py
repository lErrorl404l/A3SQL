import os
import sys
import shutil
import platform
import subprocess
import concurrent.futures
import tomllib

addon_base_path = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))

# ── Read HEMTT project config for prefix-based virtual paths ─────
_hemtt_config_path = os.path.join(addon_base_path, ".hemtt", "project.toml")
_mainprefix = "z"  # HEMTT default
_prefix = None

if os.path.isfile(_hemtt_config_path):
    with open(_hemtt_config_path, "rb") as f:
        cfg = tomllib.load(f)
        _mainprefix = cfg.get("mainprefix", "z")
        _prefix = cfg.get("prefix", "")

# ── Build virtual paths dynamically from HEMTT config ────────────
virtual_paths = [
    # Standard Arma 3 P-drive mappings (same resolution as HEMTT)
    "P:/a3|/a3",
    "P:/a3|/A3",
    "P:/x/cba|/x/cba",
]

if _prefix:
    # Dynamic prefix path from project config
    _vroot = "{}/{}".format(_mainprefix, _prefix)
    _vroot_backslash = "{}\\{}".format(_mainprefix, _prefix)
    _fmt = "{}|/{}".format(addon_base_path, _vroot)
    _fmt_bs = "{}|\\{}".format(addon_base_path, _vroot_backslash)
    virtual_paths.extend(
        [
            _fmt,
            _fmt_bs,
        ]
    )

# Also resolve bare project root (for #include "../..." etc.)
virtual_paths.extend(
    [
        "{}|/".format(addon_base_path),
        "{}|".format(addon_base_path),
    ]
)

# ── Platform-aware SQF-VM binary detection ───────────────────────
sqfvm_exe = shutil.which("sqfvm")
if not sqfvm_exe:
    # Fall back to project root with platform-appropriate extension
    ext = ".exe" if platform.system() == "Windows" else ""
    sqfvm_exe = os.path.join(addon_base_path, "sqfvm{}".format(ext))


# ── File discovery ───────────────────────────────────────────────
def get_files_to_process():
    arma_files = []
    addons_path = os.path.join(addon_base_path, "addons")
    if not os.path.isdir(addons_path):
        return arma_files
    for root, _dirs, files in os.walk(addons_path):
        for file in files:
            if file.endswith(".sqf") or file == "config.cpp":
                if file.endswith(".inc.sqf"):
                    continue
                skipPreprocessing = False
                for addonTomlPath in [
                    os.path.join(root, "addon.toml"),
                    os.path.join(os.path.dirname(root), "addon.toml"),
                ]:
                    if os.path.isfile(addonTomlPath):
                        with open(addonTomlPath, "rb") as f:
                            tomlFile = tomllib.load(f)
                            try:
                                skipPreprocessing = tomlFile.get("tools")[
                                    "sqfvm_skipConfigChecks"
                                ]
                            except:
                                pass
                if file == "config.cpp" and skipPreprocessing:
                    continue
                filePath = os.path.join(root, file)
                arma_files.append(filePath)
    return arma_files


# ── SQF-VM check per file ───────────────────────────────────────
def process_file(filePath, skipA3Warnings=True, skipPragmaHemtt=True):
    with open(filePath, "r", encoding="utf-8", errors="ignore") as file:
        content = file.read()
        if content.startswith("//pragma SKIP_COMPILE"):
            return False

    cmd = [sqfvm_exe, "--input", filePath, "--parse-only", "--automated"]
    for v in virtual_paths:
        cmd.append("-v")
        cmd.append(v)

    proc = subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, universal_newlines=True
    )
    try:
        proc.wait(12)
    except Exception:
        print("sqfvm timed out: {}".format(filePath))
        return True

    # Track real errors (not virtual-path include failures)
    realError = False
    while True:
        line = proc.stdout.readline()
        if not line:
            break
        line = line.rstrip()
        if line.startswith("[ERR]"):
            skipIt = "Failed to include" in line and (
                (_prefix and _prefix in line) or "..\\" in line
            )
            if skipIt:
                pass  # HEMTT virtual path — skip as false positive
            else:
                realError = True
                print("  {}".format(line))
        elif not (
            (
                skipA3Warnings
                and line.startswith("[WRN]")
                and ("a3/" in line)
                and (("Unexpected IFDEF" in line) or ("defined twice" in line))
            )
            or (
                skipPragmaHemtt
                and line.startswith("[WRN]")
                and ("Unknown pragma instruction 'hemtt'" in line)
            )
        ):
            print("  {}".format(line))
    return realError


# ── Main ─────────────────────────────────────────────────────────
def main():
    if not sqfvm_exe or not os.path.isfile(sqfvm_exe):
        print("Error: sqfvm not found in PATH or project root")
        return 1

    error_count = 0
    arma_files = get_files_to_process()
    if not arma_files:
        print("Warning: no addons/ directory found — run from project root")
        return 0

    print("Checking {} files".format(len(arma_files)))
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as executor:
        for fileError in executor.map(process_file, arma_files):
            if fileError:
                error_count += 1

    print("Errors: {}".format(error_count))
    return error_count


if __name__ == "__main__":
    sys.exit(main())
