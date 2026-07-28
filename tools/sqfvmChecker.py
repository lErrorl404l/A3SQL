import os
import sys
import sysconfig
import platform
import subprocess
import concurrent.futures
import tomllib

addon_base_path = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))

# Platform-aware SQF-VM binary detection
_is_linux = platform.system() == "Linux"
sqfvm_exe = None
if _is_linux:
    # Check PATH first (common on Linux), then project root
    sqfvm_in_path = subprocess.run(
        ["which", "sqfvm"], capture_output=True, text=True
    ).stdout.strip()
    if sqfvm_in_path:
        sqfvm_exe = sqfvm_in_path
    else:
        sqfvm_exe = os.path.join(addon_base_path, "sqfvm")
else:
    sqfvm_exe = os.path.join(addon_base_path, "sqfvm.exe")

# If still not found and a known install location works, use it
if not sqfvm_exe or not os.path.isfile(sqfvm_exe):
    alt_path = "/home/matt/.local/bin/sqfvm" if _is_linux else None
    if alt_path and os.path.isfile(alt_path):
        sqfvm_exe = alt_path
virtual_paths = [
    "P:/a3|/a3",
    "P:/a3|/A3",
    "P:/x/cba|/x/cba",
    "{}|/z/a3sql".format(addon_base_path),
    "{}|/".format(addon_base_path),  # resolve \ prefixed includes
    "{}|".format(addon_base_path),  # resolve bare path includes
]


def get_files_to_process(basePath):
    arma_files = []
    for root, _dirs, files in os.walk(os.path.join(addon_base_path, "addons")):
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
                    continue  # ignore configs with __has_include
                filePath = os.path.join(root, file)
                arma_files.append(filePath)
    return arma_files


def process_file(filePath, skipA3Warnings=True, skipPragmaHemtt=True):
    with open(filePath, "r", encoding="utf-8", errors="ignore") as file:
        content = file.read()
        if content.startswith("//pragma SKIP_COMPILE"):
            return False
    cmd = [sqfvm_exe, "--input", filePath, "--parse-only", "--automated"]
    for v in virtual_paths:
        cmd.append("-v")
        cmd.append(v)
    # cmd.append("-V")
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, universal_newlines=True)
    try:
        ret = proc.wait(12)  # max wait - seconds
    except Exception as _e:
        print("sqfvm timed out: {}".format(filePath))
        return True
    # print("{} = {}".format(filePath, ret))

    fileHasError = False
    keepReadingLines = True
    while keepReadingLines:
        line = proc.stdout.readline()
        if not line:
            keepReadingLines = False
        else:
            line = line.rstrip()
            if line.startswith("[ERR]"):
                fileHasError = True
            if not (
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
    return fileHasError


def main():
    if not os.path.isfile(sqfvm_exe):
        print("Error: sqfvm.exe not found in base folder [{}]".format(sqfvm_exe))
        return 1

    error_count = 0
    arma_files = get_files_to_process(addon_base_path)
    print("Checking {} files".format(len(arma_files)))
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as executor:
        for fileError in executor.map(process_file, arma_files):
            if fileError:
                error_count += 1

    print("Errors: {}".format(error_count))
    return error_count


if __name__ == "__main__":
    sys.exit(main())
