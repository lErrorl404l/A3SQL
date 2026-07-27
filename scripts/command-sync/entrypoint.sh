#!/bin/bash
# ── Arma 3 supportInfo dumper — CI entrypoint ──────────────────────────
#
# Orchestrates the full command-sync pipeline inside the Docker container:
#   1. Install / update Arma 3 DS (Profiling Branch) via SteamCMD
#   2. Build a minimal mission with init.sqf that calls supportInfo ""
#   3. Launch the DS headless
#   4. Wait for the RPT log to contain the supportInfo dump
#   5. Kill the server
#   6. Extract the supportInfo block from the RPT
#   7. Parse it with support_info_to_json.py
#   8. Write /output/commands.json
#
# Required env: STEAM_USERNAME, STEAM_PASSWORD
# Optional env: STEAM_GUARD_CODE, STEAM_AUTH_CODE
#
# Idempotent: retries on failure, reuses cached DS installation.

set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────────
STEAM_USERNAME="${STEAM_USERNAME:?STEAM_USERNAME not set}"
STEAM_PASSWORD="${STEAM_PASSWORD:?STEAM_PASSWORD not set}"
STEAM_GUARD_CODE="${STEAM_GUARD_CODE:-}"

DS_DIR="${HOME}/ds"
DS_BINARY="${DS_DIR}/arma3server_x64"
STEAMCMD="${HOME}/steamcmd/steamcmd.sh"
OUTPUT_DIR="/output"
SUPPORT_OUTPUT="${OUTPUT_DIR}/support_info_raw.txt"
COMMANDS_JSON="${OUTPUT_DIR}/commands.json"
CHANGE_FLAG="${OUTPUT_DIR}/.changed"

# SteamCMD + DS paths
STEAMCMD_DIR="${HOME}/steamcmd"

# Mission / server config
MISSION_DIR="/tmp/a3sql_sync_mission"
MISSION_PBO="/tmp/a3sql_sync_mission.pbo"
SERVER_CFG="/tmp/server.cfg"

# RPT paths — Linux DS writes RPT to ~/.local/share/Arma 3/<profile>/
ARMA_PROFILE_DIR="${HOME}/.local/share/Arma 3"
RPT_FILE=""
MAX_WAIT_SEC=300          # max seconds to wait for supportInfo dump
SERVER_STARTUP_SEC=30     # initial server boot time before checking
POLL_INTERVAL_SEC=5       # how often to check RPT

# ── Helpers ─────────────────────────────────────────────────────────────
info()  { echo "[info]  $*"; }
warn()  { echo "[warn]  $*" >&2; }
error() { echo "[error] $*" >&2; }

cleanup() {
    info "Cleaning up..."
    # Kill server if running
    if [ -n "${SERVER_PID:-}" ] && kill -0 "${SERVER_PID}" 2>/dev/null; then
        info "Stopping server (PID ${SERVER_PID})..."
        kill "${SERVER_PID}" 2>/dev/null || true
        sleep 2
        kill -9 "${SERVER_PID}" 2>/dev/null || true
    fi
    # Kill any lingering arma3server processes
    pkill -f "arma3server" 2>/dev/null || true
}
trap cleanup EXIT

# ── Step 1: Install / update Arma 3 DS ─────────────────────────────────
install_or_update_ds() {
    info "Installing/updating Arma 3 Dedicated Server (Profiling Branch)..."

    # SteamCMD doesn't like running as root — use gosu if available
    local steam_cmd
    if command -v gosu &>/dev/null; then
        steam_cmd="gosu arma3 ${STEAMCMD}"
    else
        steam_cmd="${STEAMCMD}"
    fi

    # Build steamcmd arguments
    local args=(
        "+force_install_dir" "${DS_DIR}"
        "+login" "${STEAM_USERNAME}" "${STEAM_PASSWORD}"
    )
    if [ -n "${STEAM_GUARD_CODE}" ]; then
        args+=("${STEAM_GUARD_CODE}")
    fi
    args+=(
        "+app_update" "233780" "-beta" "profiling" "validate"
        "+quit"
    )

    # Retry loop
    local max_attempts=3
    local attempt=1
    while [ ${attempt} -le ${max_attempts} ]; do
        info "SteamCMD attempt ${attempt}/${max_attempts}..."
        if ${steam_cmd} "${args[@]}"; then
            info "SteamCMD succeeded."
            return 0
        fi
        warn "SteamCMD attempt ${attempt} failed. Retrying in 10s..."
        sleep 10
        attempt=$((attempt + 1))
    done

    error "SteamCMD failed after ${max_attempts} attempts."
    return 1
}

# ── Step 2: Build minimal server config ────────────────────────────────
build_server_cfg() {
    info "Generating server.cfg..."
    local random_port
    random_port=$(( 2302 + RANDOM % 1000 ))

    sed "s/{{PORT}}/${random_port}/g" "${HOME}/server.cfg.template" > "${SERVER_CFG}"
    info "Server config: ${SERVER_CFG}"
}

# ── Step 3: Build mission PBO ──────────────────────────────────────────
build_mission_pbo() {
    info "Building supportInfo dump mission PBO..."

    rm -rf "${MISSION_DIR}"
    mkdir -p "${MISSION_DIR}"

    # mission.sqm — minimal arma 3 mission config (empty stratis)
    cat > "${MISSION_DIR}/mission.sqm" << 'EOF'
version=2;
class mission {
    class Intel { class Attributes {}; };
    class Entities {
        items=0;
    };
};
class scenario {
    author="a3sql-sync";
    disableStratIntel=1;
    enableTargetDebug=1;
    enableRandomization=0;
    isBuilding=0;
    isEditable=0;
    isOpen=1;
    isWrecked=0;
    isUseless=0;
    lockObjective=0;
    playable=0;
    showDiameter=0;
    showMarker=false;
    showTitle=false;
    source="a3sql";
};
EOF

    # init.sqf — calls supportInfo and writes to a known file
    cp "${HOME}/support_dump_init.sqf" "${MISSION_DIR}/init.sqf"

    # Build PBO using armake
    armake build -f -p "${MISSION_DIR}" "${MISSION_PBO}"
    info "Mission PBO: ${MISSION_PBO}"
}

# ── Step 4: Launch server headless ─────────────────────────────────────
launch_server() {
    info "Launching Arma 3 DS (Profiling Branch) headless..."

    if [ ! -x "${DS_BINARY}" ]; then
        error "Server binary not found at ${DS_BINARY}"
        ls -la "${DS_DIR}/" 2>/dev/null || true
        return 1
    fi

    # Determine RPT path before starting
    local profile_name="a3sql_sync"
    local rpt_dir="${ARMA_PROFILE_DIR}/${profile_name}"
    mkdir -p "${rpt_dir}"
    # The RPT file naming: Arma3Retail_*.RPT or arma3server*.RPT on Linux
    # Linux DS uses: ~/.local/share/Arma 3/<profile>/<exe_name>.RPT

    # Start server in background
    "${DS_BINARY}" \
        -name="${profile_name}" \
        -config="${SERVER_CFG}" \
        -mod="${MISSION_PBO}" \
        -autoInit \
        -headless \
        -noSound \
        -noPause \
        -world=empty \
        -maxMem=1024 \
        -cpuCount=2 \
        -exThreads=0 \
        -enableHT \
        > /tmp/server_stdout.log 2>&1 &

    SERVER_PID=$!
    info "Server PID: ${SERVER_PID}"

    # Give it time to boot
    info "Waiting ${SERVER_STARTUP_SEC}s for server to boot..."
    sleep "${SERVER_STARTUP_SEC}"
}

# ── Step 5: Wait for supportInfo in RPT ────────────────────────────────
wait_for_support_dump() {
    info "Waiting for supportInfo dump in RPT log..."

    local elapsed=0
    while [ ${elapsed} -lt ${MAX_WAIT_SEC} ]; do
        # Find the most recent RPT file
        local rpt_candidates
        rpt_candidates=$(find "${ARMA_PROFILE_DIR}" -name "*.RPT" -newer "${SERVER_CFG}" 2>/dev/null | sort -t_ -k2 -rn | head -3 || true)

        if [ -z "${rpt_candidates}" ]; then
            # Fallback: check common locations
            rpt_candidates=$(find "${ARMA_PROFILE_DIR}" -name "*.RPT" 2>/dev/null | sort -t_ -k2 -rn | head -3 || true)
        fi

        for rpt in ${rpt_candidates}; do
            if [ -f "${rpt}" ]; then
                RPT_FILE="${rpt}"
                info "Checking RPT: ${RPT_FILE}"

                # Look for the supportInfo dump pattern — lines starting with n:, u:, b:
                if grep -qE '^[nub]:' "${RPT_FILE}" 2>/dev/null; then
                    info "Found supportInfo dump in RPT (after ~${elapsed}s)"
                    return 0
                fi
            fi
        done

        # Also check stdout log as fallback
        if [ -f /tmp/server_stdout.log ] && grep -qE '^[nub]:' /tmp/server_stdout.log 2>/dev/null; then
            RPT_FILE="/tmp/server_stdout.log"
            info "Found supportInfo dump in server stdout (after ~${elapsed}s)"
            return 0
        fi

        sleep "${POLL_INTERVAL_SEC}"
        elapsed=$((elapsed + POLL_INTERVAL_SEC))
    done

    warn "Timed out waiting for supportInfo dump (${MAX_WAIT_SEC}s)"
    warn "Server stdout tail:"
    tail -50 /tmp/server_stdout.log 2>/dev/null || true
    warn "RPT files found:"
    find "${ARMA_PROFILE_DIR}" -name "*.RPT" 2>/dev/null | head -5 || true
    return 1
}

# ── Step 6: Extract supportInfo block from RPT ─────────────────────────
extract_support_info() {
    info "Extracting supportInfo from ${RPT_FILE}..."

    # The supportInfo dump appears as a block of n:/u:/b: lines in the RPT.
    # Extract contiguous blocks of these lines. We capture all matching lines
    # even if interleaved with other RPT output (less likely — supportInfo
    # typically comes as one contiguous block).
    grep -E '^[nub]:' "${RPT_FILE}" > "${SUPPORT_OUTPUT}" 2>/dev/null || {
        warn "No n:/u:/b: lines found in RPT. Falling back to stdout."
        grep -E '^[nub]:' /tmp/server_stdout.log > "${SUPPORT_OUTPUT}" 2>/dev/null || {
            error "No supportInfo lines found anywhere."
            return 1
        }
    }

    local line_count
    line_count=$(wc -l < "${SUPPORT_OUTPUT}")
    info "Extracted ${line_count} supportInfo lines to ${SUPPORT_OUTPUT}"

    if [ "${line_count}" -lt 10 ]; then
        warn "Very few supportInfo lines (${line_count}) — may be incomplete."
    fi
}

# ── Step 7: Parse to JSON ──────────────────────────────────────────────
parse_to_json() {
    info "Running support_info_to_json.py..."

    python3 "${HOME}/support_info_to_json.py" \
        --file "${SUPPORT_OUTPUT}" \
        --output "${COMMANDS_JSON}" \
        --pretty

    local cmd_count
    cmd_count=$(python3 -c "
import json
with open('${COMMANDS_JSON}') as f:
    cmds = json.load(f)
print(len(cmds))
" 2>/dev/null || echo "0")

    info "Wrote ${cmd_count} commands to ${COMMANDS_JSON}"
}

# ── Step 8: Check for changes ──────────────────────────────────────────
check_changes() {
    if [ -f "${OUTPUT_DIR}/commands_previous.json" ]; then
        if ! diff -q "${OUTPUT_DIR}/commands_previous.json" "${COMMANDS_JSON}" >/dev/null 2>&1; then
            info "Command metadata CHANGED since last run."
            touch "${CHANGE_FLAG}"
        else
            info "Command metadata unchanged."
            rm -f "${CHANGE_FLAG}"
        fi
    else
        info "No previous snapshot — first run."
        touch "${CHANGE_FLAG}"
    fi
    cp "${COMMANDS_JSON}" "${OUTPUT_DIR}/commands_previous.json"
}

# ── Main Pipeline ──────────────────────────────────────────────────────
main() {
    info "=== Arma 3 Command Sync Pipeline ==="
    info "Output: ${OUTPUT_DIR}"

    # Ensure SteamCMD is installed
    if [ ! -f "${STEAMCMD}" ]; then
        info "SteamCMD not found at ${STEAMCMD}, installing..."
        mkdir -p "${STEAMCMD_DIR}"
        cd /tmp
        curl -fsSL "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz" \
            -o steamcmd.tar.gz
        tar xzf steamcmd.tar.gz -C "${STEAMCMD_DIR}"
        rm steamcmd.tar.gz
    fi

    install_or_update_ds
    build_server_cfg
    build_mission_pbo
    launch_server

    # Wait for server to produce supportInfo dump
    if ! wait_for_support_dump; then
        error "Failed to capture supportInfo dump."
        exit 1
    fi

    # Kill server before parsing (RPT is flushed)
    info "Stopping server..."
    kill "${SERVER_PID}" 2>/dev/null || true
    sleep 2
    SERVER_PID=""

    extract_support_info
    parse_to_json
    check_changes

    info "=== Pipeline complete ==="
    info "Output: ${COMMANDS_JSON}"
    if [ -f "${CHANGE_FLAG}" ]; then
        info "Changes detected — flag set."
    else
        info "No changes detected."
    fi
}

main "$@"
