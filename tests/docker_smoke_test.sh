#!/usr/bin/env bash
# docker_smoke_test.sh — local Docker smoke test for A3SQL extension
#
# Builds the extension in a Debian 12 container (GLIBC 2.36 match),
# packs the smoke test mission PBO, starts Arma 3 dedicated server
# with headless client, and verifies all SQL operations pass.
#
# Usage:
#   ./tests/docker_smoke_test.sh [--keep] [--build-only] [--dir PATH]
#
#   --keep         Keep the Docker container running after test
#   --build-only   Build extension and pack PBO, then exit
#   --dir PATH     Docker test directory (default: tests/docker)
#
# Requirements:
#   - Docker
#   - armake (for PBO packing)
#   - Arma 3 Server install (for mod files and server binary)
#
# Exit codes:
#   0 = all tests passed
#   1 = test failed or error
#   2 = prerequisites missing

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOCKER_DIR="${DOCKER_DIR:-$REPO_ROOT/tests/docker}"
BUILD_IMAGE="debian:bookworm"
CONTAINER_NAME="a3sql-smoke-test"
SMOKE_TIMEOUT=120 # seconds to wait for smoke output
KEEP_CONTAINER=false
BUILD_ONLY=false
# Arma 3 dedicated server install (the arma3server_x64 binary lives here).
# Point ARMA3_SERVER_ROOT at your own server directory; without it the test
# cannot launch Arma.
ARMA3_SERVER_ROOT="${ARMA3_SERVER_ROOT:-/ext/a3sql-docker/server}"

# ── Profiles cleanup ───────────────────────────────────────────────
# The Arma server container runs as root and writes its profile state to
# tests/docker/configs/profiles, leaving a root-owned dir on the host.
# HEMTT's git check walks the whole project tree and hard-fails on an
# unreadable dir, so remove it before building.
clean_profiles() {
	docker run --rm -v "$DOCKER_DIR/configs:/c" alpine:latest rm -rf /c/profiles 2>/dev/null || true
}

# ── Parse args ─────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
	case "$1" in
	--keep)
		KEEP_CONTAINER=true
		shift
		;;
	--build-only)
		BUILD_ONLY=true
		shift
		;;
	--dir)
		DOCKER_DIR="$2"
		shift 2
		;;
	*)
		echo "Unknown option: $1"
		exit 2
		;;
	esac
done

# ── Prerequisites ──────────────────────────────────────────────────
missing=()
command -v docker >/dev/null 2>&1 || missing+=("docker")
command -v armake >/dev/null 2>&1 || missing+=("armake")
if [[ ${#missing[@]} -gt 0 ]]; then
	echo "ERROR: missing prerequisites: ${missing[*]}"
	exit 2
fi

# ── Paths ──────────────────────────────────────────────────────────
EXTENSION_SRC="$REPO_ROOT/extension"
MOD_DIR="$DOCKER_DIR/mods/@a3sql"
MISSIONS_DIR="$DOCKER_DIR/missions"
TEST_MIN="$MISSIONS_DIR/test.Min"
TEST_PBO="$MISSIONS_DIR/test.Stratis.pbo"
SERVER_CFG="$DOCKER_DIR/configs/server.cfg"
COMPOSE_FILE="$DOCKER_DIR/docker-compose.yml"
echo "═══════════════════════════════════════════════════════════════"
echo " A3SQL Docker Smoke Test"
echo "═══════════════════════════════════════════════════════════════"
echo "  Repo:       $REPO_ROOT"
echo "  Docker dir: $DOCKER_DIR"
echo "  Extension:  $EXTENSION_SRC"
echo "  Mod dir:    $MOD_DIR"
echo "  Missions:   $MISSIONS_DIR"
echo ""

# ── Step 1: Build extension in Debian 12 container ─────────────────
echo "── Step 1: Build extension (GLIBC 2.36) ──────────────────────"
clean_profiles
# Check if we already have a valid build (the .so HEMTT will pack)
EXT_SO="$EXTENSION_SRC/target/release/liba3sql.so"
NEED_BUILD=true
if [[ -f "$EXT_SO" ]]; then
	# Check GLIBC version of existing build (objdump -T is robust)
	GLIBC_MAX=$(objdump -T "$EXT_SO" 2>/dev/null | grep -oP 'GLIBC_\K[0-9.]+' | sort -V | tail -1 || echo "0")
	if [[ -n "$GLIBC_MAX" ]] && [[ "$(printf '%s\n' "2.36" "$GLIBC_MAX" | sort -V | tail -1)" == "2.36" ]]; then
		echo "  Existing build GLIBC max: $GLIBC_MAX (within 2.36 limit)"
		NEED_BUILD=false
	else
		echo "  Existing build requires GLIBC ${GLIBC_MAX:-unknown} (> 2.36), rebuilding"
	fi
fi

if $NEED_BUILD; then
	echo "  Building in $BUILD_IMAGE container (clean target dir)..."
	docker run --rm \
		-v "$REPO_ROOT":/src \
		"$BUILD_IMAGE" bash -c '
            set -e
            export DEBIAN_FRONTEND=noninteractive
            apt-get update -qq
            apt-get install -y -qq curl build-essential pkg-config \
                libgit2-dev libssl-dev cmake git >/dev/null 2>&1
            curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | \
                sh -s -- -y --default-toolchain stable --profile minimal >/dev/null 2>&1
            export PATH="$HOME/.cargo/bin:$PATH"
            cd /src/extension
            # Clean CARGO_TARGET_DIR so no host-built artifacts leak in
            export CARGO_TARGET_DIR=/tmp/ctarget
            cargo build --release 2>&1 | tail -5
            cp /tmp/ctarget/release/liba3sql.so /src/extension/target/release/liba3sql.so
            echo "  Build complete: $(ls -lh /src/extension/target/release/liba3sql.so)"
        '
	echo "  Extension built successfully"
fi

# ── Step 1b: Build mod with HEMTT (packs the GLIBC-2.36 .so + PBOs) ─
echo "── Step 1b: HEMTT build (packs PBOs + extension) ─────────────"
(cd "$REPO_ROOT" && hemtt build 2>&1 | tail -3)
# The mod dir may be root-owned from a previous container run; assemble it
# inside docker so the rm/copy work, then hand ownership back to the host user.
docker run --rm -v "$DOCKER_DIR:/d" -v "$REPO_ROOT/.hemttout/build:/build:ro" alpine:latest \
	sh -c 'rm -rf /d/mods/@a3sql && mkdir -p /d/mods/@a3sql && cp -r /build/* /d/mods/@a3sql/ && chmod +x /d/mods/@a3sql/a3sql_x64.so && chown -R 1000:1000 /d/mods/@a3sql'
echo "  Mod assembled: $(ls "$MOD_DIR" | tr '\n' ' ')"

# ── Step 2: Pack mission PBO ───────────────────────────────────────
echo ""
echo "── Step 2: Pack mission PBO ──────────────────────────────────"

# Create mission source files
mkdir -p "$TEST_MIN"

cat >"$TEST_MIN/mission.sqm" <<'SQM'
version=54;
class EditorData
{
	moveGridStep=2;
	angleGridStep=0.2617994;
	scaleGridStep=1;
	autoGroupingDist=10;
	toggles=517;
	class ItemIDProvider
	{
		nextID=2;
	};
	class Camera
	{
		pos[]={5,5,-10};
		dir[]={-0.5,-0.3,0.8};
		up[]={-0.3,0.9,-0.2};
		aside[]={-0.8,0,0.5};
	};
};
binarizationWanted=0;
sourceName="test.Stratis";
addons[]=
{
	"A3_Characters_F"
};
randomSeed=1234567;
class Mission
{
	class Intel
	{
		startWeather=0;
		startWind=0.1;
		startWaves=0.1;
		forecastWeather=0;
		forecastWind=0.1;
		forecastWaves=0.1;
		forecastLightnings=0.1;
		wavesForced=1;
		windForced=1;
		year=2035;
		day=1;
		hour=12;
		minute=0;
		startFogDecay=0.014;
		forecastFogDecay=0.014;
	};
	class Entities
	{
		items=0;
	};
};
SQM

cat >"$TEST_MIN/description.ext" <<'EXT'
class Header
{
	gameType = "Coop";
	minPlayers = 0;
	maxPlayers = 8;
	playerCountMultipleOf = 1;
};
EXT

# Create init.sqf that runs the comprehensive smoke test
cat >"$TEST_MIN/init.sqf" <<'SQF'
/* A3SQL Docker smoke test — runs the comprehensive test suite
 * Runs on server, logs pass/fail to RPT, ends mission.
 */

if (!isServer) exitWith {};

[] spawn {
    waitUntil {time > 2};

    private _handle = execVM "smoke_test.sqf";
    waitUntil {scriptDone _handle};

    diag_log text "[A3SQL-TEST] Smoke test complete, ending mission";
    endMission "END1";
};
SQF

# Copy the smoke test suite into the mission (the mod PBOs do not carry it)
cp "$REPO_ROOT/tests/a3sql_smoke_test.sqf" "$TEST_MIN/smoke_test.sqf"

# Pack with armake (v0.6.x uses -f -p, not -A -P)
echo "  Packing test.Stratis.pbo..."
(cd "$TEST_MIN" && armake build -f -p . "$TEST_PBO" 2>&1 | tail -3)
echo "  PBO packed: $(ls -lh "$TEST_PBO" 2>/dev/null || echo 'FAILED')"

# ── CBA mod — needed for XEH + CBA_fnc_parseJSON. Download if missing ──
MODS="$DOCKER_DIR/mods"
CBA_DIR="$MODS/@cba_a3"
if [[ ! -d "$CBA_DIR" ]] || [[ ! -d "$CBA_DIR/addons" ]]; then
	echo "  Downloading CBA_A3 v3.19.0..."
	mkdir -p "$MODS"
	curl -fsSL "https://github.com/CBATeam/CBA_A3/releases/download/v3.19.0/CBA_A3_v3.19.0.zip" -o /tmp/cba.zip
	rm -rf "$CBA_DIR" /tmp/cba-x
	mkdir -p /tmp/cba-x
	unzip -oq /tmp/cba.zip -d /tmp/cba-x
	rm -f /tmp/cba.zip
	# The release zip contains a single @CBA_A3 folder; normalise the case
	# for the case-sensitive linux filesystem and flatten any nesting.
	SRC=$(find /tmp/cba-x -maxdepth 2 -type d -name "@cba_a3" -o -maxdepth 2 -type d -name "@CBA_A3" | head -1)
	if [[ -n "$SRC" ]]; then
		mv "$SRC" "$CBA_DIR"
	else
		echo "  ERROR: CBA zip did not contain @cba_a3 folder"
		exit 1
	fi
	rm -rf /tmp/cba-x
	[[ -d "$CBA_DIR/addons" ]] || {
		echo "  ERROR: CBA @cba_a3 has no addons/"
		exit 1
	}
fi
echo "  CBA: $CBA_DIR"

if $BUILD_ONLY; then
	echo ""
	echo "Build-only mode. Extension and PBO ready."
	echo "  Extension: $MOD_DIR/a3sql_x64.so"
	echo "  Mission:   $TEST_PBO"
	exit 0
fi

# ── Step 3: Start Docker container ─────────────────────────────────
echo ""
echo "── Step 3: Start Arma 3 server ───────────────────────────────"
# Stop any existing container
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true

docker run -d \
	--name "$CONTAINER_NAME" \
	--network host \
	--platform linux/amd64 \
	-e ARMA3_CONFIG_FILE=/arma3/config.toml \
	-e ARMA3_SERVER__SKIP_INSTALL=true \
	-e ARMA3_SERVER__CONFIG=server.cfg \
	-e ARMA3_SERVER__PROFILE=a3sqltest \
	-e ARMA3_SERVER__WORLD=Stratis \
	-e 'ARMA3_SERVER__CDLC=[]' \
	-e ARMA3_SERVER__PARAMS="-autoInit -noBattlEye -mod=mods/@a3sql;mods/@cba_a3" \
	-e ARMA3_HEADLESS__CLIENTS=0 \
	-v "$DOCKER_DIR/config.smoke.toml:/arma3/config.toml" \
	-v "$DOCKER_DIR/configs:/arma3/server/configs" \
	-v "$DOCKER_DIR/mods:/arma3/server/mods" \
	-v "$MISSIONS_DIR:/arma3/server/mpmissions" \
	-v "$ARMA3_SERVER_ROOT:/arma3/server" \
	--mount type=tmpfs,target=/arma3/server/configs/profiles \
	--entrypoint /bin/sh \
	ghcr.io/brettmayson/arma3server/arma3server:v3 \
	-c 'unset ARMA3_SERVER__CDLC; cp /arma3/server/mods/@a3sql/a3sql_x64.so /arma3/server/a3sql_x64.so 2>/dev/null; for m in /arma3/server/mods/@*; do [ -e "$m" ] || continue; ln -sfn "$m" "/arma3/server/$(basename "$m")"; done; exec /usr/local/bin/arma3server' \
	2>/dev/null

echo "  Container started: $CONTAINER_NAME"

# ── Step 4: Wait for smoke test output ─────────────────────────────
echo ""
echo "── Step 4: Waiting for smoke test (timeout: ${SMOKE_TIMEOUT}s) ──"
ELAPSED=0
PASS=false
FAIL=false

while [[ $ELAPSED -lt $SMOKE_TIMEOUT ]]; do
	# Check if container is still running
	if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
		echo "  Container exited unexpectedly"
		docker logs "$CONTAINER_NAME" 2>&1 | tail -20
		FAIL=true
		break
	fi

	# Check for smoke test results in logs
	LOGS=$(docker logs "$CONTAINER_NAME" 2>&1)

	if echo "$LOGS" | grep -q "\[A3SQL-TEST\] Smoke test complete"; then
		echo "  Smoke test completed!"
		# Check for failures
		FAIL_COUNT=$(echo "$LOGS" | grep -c "\[A3SQL\] ✗" || true)
		PASS_COUNT=$(echo "$LOGS" | grep -c "\[A3SQL\] ✓" || true)
		echo "  Results: $PASS_COUNT passed, $FAIL_COUNT failed"

		if [[ $FAIL_COUNT -eq 0 ]] && [[ $PASS_COUNT -gt 0 ]]; then
			PASS=true
		else
			FAIL=true
			echo ""
			echo "  Failed tests:"
			echo "$LOGS" | grep "\[A3SQL\] ✗" | sed 's/^/    /'
		fi
		break
	fi

	# Check for A3SQL load failure
	if echo "$LOGS" | grep -q "CallExtension 'a3sql' could not be found"; then
		echo "  FAILED: Extension not loaded"
		FAIL=true
		break
	fi

	sleep 2
	ELAPSED=$((ELAPSED + 2))
	echo -ne "\r  Waiting... ${ELAPSED}s / ${SMOKE_TIMEOUT}s"
done

if ! $PASS && ! $FAIL; then
	echo ""
	echo "  TIMEOUT: Smoke test did not complete within ${SMOKE_TIMEOUT}s"
	FAIL=true
fi

# ── Step 5: Collect logs and cleanup ───────────────────────────────
echo ""
echo ""
echo "── Step 5: Server log (last 30 lines) ───────────────────────"
docker logs "$CONTAINER_NAME" 2>&1 | tail -30 | sed 's/^/  /'

if ! $KEEP_CONTAINER; then
	echo ""
	echo "── Cleanup ──────────────────────────────────────────────────"
	docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
	echo "  Container removed"
fi

# ── Result ─────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
if $PASS; then
	echo " RESULT: PASS ✓"
	echo "═══════════════════════════════════════════════════════════════"
	exit 0
else
	echo " RESULT: FAIL ✗"
	echo "═══════════════════════════════════════════════════════════════"
	exit 1
fi
