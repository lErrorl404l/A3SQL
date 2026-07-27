#!/usr/bin/env bash
# Local SteamCMD command sync — non-Docker fallback.
#
# Runs the same pipeline as the Docker image but using a local SteamCMD
# installation. Use this if Docker isn't available or you want faster
# iteration during development.
#
# Prerequisites:
#   - SteamCMD installed (https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz)
#   - Python 3 with `requirements.txt` deps installed
#   - Steam account that owns Arma 3
#
# Usage (no env vars → uses cached SteamCMD session if valid):
#   bash scripts/command-sync/sync_local.sh
#
# Usage (with credentials):
#   export STEAM_USERNAME="your_username"
#   export STEAM_PASSWORD="your_password"
#   export STEAM_GUARD_CODE="optional_2fa_code"
#   bash scripts/command-sync/sync_local.sh
#
# Output:
#   /tmp/sync_output/commands.json  — parsed supportInfo

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARMA_APP_ID="${ARMA_APP_ID:-233780}"  # Arma 3 Dedicated Server

# ── Auto-detect paths via Steam library VDF ─────────────────────────────
echo "[sync] Discovering Steam install paths..."
PATHS=$(python3 "$SCRIPT_DIR/discover_paths.py" 2>/dev/null || echo '{"steamcmd":null,"arma3":null,"output":"/tmp/sync_output"}')
STEAMCMD="${STEAMCMD:-$(echo "$PATHS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('steamcmd') or '')" 2>/dev/null)}"
ARMA_DIR="${ARMA_DIR:-$(echo "$PATHS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('arma3') or '')" 2>/dev/null)}"
OUTPUT_DIR="${OUTPUT_DIR:-$(echo "$PATHS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('output') or '/tmp/sync_output')" 2>/dev/null)}"

# Fallback defaults if VDF discovery returned nothing
ARMA_DIR="${ARMA_DIR:-$HOME/arma3}"
OUTPUT_DIR="${OUTPUT_DIR:-/tmp/sync_output}"
mkdir -p "$OUTPUT_DIR"

# ── 1. Install SteamCMD if missing ─────────────────────────────────────
if [ -z "$STEAMCMD" ]; then
    if command -v steamcmd &>/dev/null; then
        STEAMCMD="steamcmd"
    elif [ -x "$HOME/steamcmd/steamcmd.sh" ]; then
        STEAMCMD="$HOME/steamcmd/steamcmd.sh"
    else
        echo "[sync] SteamCMD not found — installing to $HOME/steamcmd"
        mkdir -p "$HOME/steamcmd"
        curl -fsSL "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz" \
            | tar -xz -C "$HOME/steamcmd"
        STEAMCMD="$HOME/steamcmd/steamcmd.sh"
    fi
fi

# ── 2. Login — try cached session first, fall back to env credentials ──
LOGIN_CMD=""
if [ -n "${STEAM_USERNAME:-}" ] && [ -n "${STEAM_PASSWORD:-}" ]; then
    echo "[sync] Using env credentials: $STEAM_USERNAME"
    LOGIN_CMD="+login $STEAM_USERNAME $STEAM_PASSWORD ${STEAM_GUARD_CODE:-}"
else
    echo "[sync] No credentials in env — trying cached SteamCMD session..."
    # Check if steamcmd has a cached session by running a simple query
    LOGIN_CMD="+login anonymous"
    # anonymous won't work for Arma 3 DS, but it lets us test session caching
fi

# ── 3. Download / update Arma 3 DS (Profiling Branch) ──────────────────
echo "[sync] Downloading/updating Arma 3 DS (Profiling Branch)..."
if ! $STEAMCMD +force_install_dir "$ARMA_DIR" \
    $LOGIN_CMD \
    +app_update "$ARMA_APP_ID" -beta profiling validate \
    +quit; then
    echo "[sync] Download failed — possibly need to login."
    echo "[sync] Set STEAM_USERNAME, STEAM_PASSWORD, and (if needed) STEAM_GUARD_CODE."
    echo "[sync] Or login interactively first: steamcmd +login YOUR_USERNAME"
    exit 1
fi

# ── 4. Create minimal mission for supportInfo dump ────────────────────
MISSION_DIR="$ARMA_DIR/mpmissions/__support_dump__"
mkdir -p "$MISSION_DIR"
cat > "$MISSION_DIR/init.sqf" << 'SQFEOM'
// Boot trigger — runs once after mission starts
[] spawn {
    waitUntil { time > 0 };
    systemChat "[sync] Starting supportInfo dump...";
    private _dump = supportInfo "";
    // Write to RPT — extracted from logs later
    diag_log text "[SYNC_DUMP_BEGIN]";
    diag_log text _dump;
    diag_log text "[SYNC_DUMP_END]";
    // Shut down cleanly
    diag_log text "[sync] Dump complete, exiting.";
    endMission "END1";
};
SQFEOM

cat > "$MISSION_DIR/mission.sqm" << 'SQMEOM'
version=12;
class Mission { addOns[] = {}; addOnsAuto[] = {}; randomSeed = 0; };
class Intel { briefingName = "support_dump"; };
SQMEOM

# ── 5. Boot server and capture output ─────────────────────────────────
echo "[sync] Booting Arma 3 DS Profiling Branch..."
RPT_DIR="$ARMA_DIR"
SERVER_CFG="$ARMA_DIR/server.cfg"
cat > "$SERVER_CFG" << 'CFGEOM'
hostname = "CMD_SYNC";
password = "";
passwordAdmin = "";
maxPlayers = 0;
headlessClients[] = {};
localClient[] = {0};
CFGEOM

# Run server headless, capture RPT output
timeout 300 "$ARMA_DIR/arma3server" \
    -config="$SERVER_CFG" \
    -port=2302 \
    -world=empty \
    -name=sync \
    -profiles=sync \
    -cfg="$ARMA_DIR/basic.cfg" \
    -mod= \
    -serverMod= \
    -nosplash \
    -noSound \
    -skipIntro \
    -autoInit \
    2>&1 | tee "$OUTPUT_DIR/server.log" || true

# ── 6. Extract supportInfo from RPT ───────────────────────────────────
echo "[sync] Extracting supportInfo from server log..."
RPT_FILE=$(ls -t "$ARMA_DIR/sync/*.rpt" 2>/dev/null | head -1)
if [ -z "$RPT_FILE" ]; then
    # Fallback: search in common locations
    RPT_FILE=$(find "$ARMA_DIR" -name "*.rpt" -newer "$OUTPUT_DIR/server.log" 2>/dev/null | head -1)
fi
if [ -z "$RPT_FILE" ]; then
    # Last resort: extract from stdout log
    RPT_FILE="$OUTPUT_DIR/server.log"
fi

echo "[sync] RPT source: $RPT_FILE"

# Extract lines between BEGIN/END markers, or grep for n:/u:/b: patterns
if grep -q "SYNC_DUMP_BEGIN" "$RPT_FILE" 2>/dev/null; then
    sed -n '/SYNC_DUMP_BEGIN/,/SYNC_DUMP_END/p' "$RPT_FILE" \
        | grep -E '^[[:space:]]*(n:|u:|b:|Type:)' \
        > "$OUTPUT_DIR/support_info_raw.txt"
else
    # Fallback: grep all n:/u:/b: lines
    grep -E '^[[:space:]]*(n:|u:|b:)' "$RPT_FILE" \
        > "$OUTPUT_DIR/support_info_raw.txt" 2>/dev/null || true
fi

# ── 7. Parse to JSON ──────────────────────────────────────────────────
echo "[sync] Parsing supportInfo to JSON..."
if [ -s "$OUTPUT_DIR/support_info_raw.txt" ]; then
    python3 "$SCRIPT_DIR/support_info_to_json.py" \
        --file "$OUTPUT_DIR/support_info_raw.txt" \
        --output "$OUTPUT_DIR/commands.json" \
        --pretty
    echo "[sync] Done — commands written to $OUTPUT_DIR/commands.json"
    echo "[sync] Total commands: $(python3 -c "import json; print(len(json.load(open('$OUTPUT_DIR/commands.json'))))")"
else
    echo "[sync] WARNING: No supportInfo data captured. Try running manually with longer timeout."
    exit 1
fi
