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
#   --dir PATH     Docker test directory (default: ../a3sql-docker relative to repo root)
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
DOCKER_DIR="${DOCKER_DIR:-$(cd "$REPO_ROOT/.." && pwd)/a3sql-docker}"
BUILD_IMAGE="debian:bookworm"
CONTAINER_NAME="a3sql-smoke-test"
SMOKE_TIMEOUT=120 # seconds to wait for smoke output
KEEP_CONTAINER=false
BUILD_ONLY=false

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
# Check if we already have a valid build
NEED_BUILD=true
if [[ -f "$MOD_DIR/a3sql_x64.so" ]]; then
	# Check GLIBC version of existing build
	GLIBC_MAX=$(ldd "$MOD_DIR/a3sql_x64.so" 2>/dev/null | grep -oP 'GLIBC_\K[0-9.]+' | sort -V | tail -1 || echo "0")
	if [[ "$(printf '%s\n' "2.36" "$GLIBC_MAX" | sort -V | tail -1)" == "2.36" ]]; then
		echo "  Existing build GLIBC max: $GLIBC_MAX (within 2.36 limit)"
		NEED_BUILD=false
	else
		echo "  Existing build requires GLIBC $GLIBC_MAX (> 2.36), rebuilding"
	fi
fi

if $NEED_BUILD; then
	echo "  Building in $BUILD_IMAGE container..."
	docker run --rm \
		-v "$REPO_ROOT":/src \
		-v "$MOD_DIR":/output \
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
            cargo build --release 2>&1 | tail -5
            cp target/release/liba3sql.so /output/a3sql_x64.so
            echo "  Build complete: $(ls -lh /output/a3sql_x64.so)"
        '
	echo "  Extension built successfully"
fi

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
 * Waits for CBA initialization, executes all tests, ends mission.
 */

// Wait for CBA to be ready
[] spawn {
    waitUntil {!isNull player && {time > 0} && {[] call CBA_fnc_isFeature}};

    // Run the comprehensive smoke test
    private _handle = execVM "\z\a3sql\addons\main\tests\a3sql_smoke_test.sqf";
    waitUntil {scriptDone _handle};

    // The test logs pass/fail to RPT. End with END1 (success for autotest).
    // If any test failed, diag_log already warned.
    diag_log text "[A3SQL-TEST] Smoke test complete, ending mission";
    endMission "END1";
};
SQF

# Pack with armake
echo "  Packing test.Stratis.pbo..."
(cd "$TEST_MIN" && armake build -A -P . "$TEST_PBO" 2>&1 | tail -3)
echo "  PBO packed: $(ls -lh "$TEST_PBO" 2>/dev/null || echo 'FAILED')"

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
	-e ARMA3_SERVER__PARAMS="-autoInit -noBattlEye" \
	-e ARMA3_HEADLESS__CLIENTS=1 \
	-v "$DOCKER_DIR/config.toml:/arma3/config.toml" \
	-v "$DOCKER_DIR/configs:/arma3/server/configs" \
	-v "$DOCKER_DIR/mods:/arma3/server/mods" \
	-v "$MISSIONS_DIR:/arma3/server/mpmissions" \
	-v "$DOCKER_DIR/server:/arma3/server" \
	--entrypoint '["/bin/sh", "-c", "unset ARMA3_SERVER__CDLC; cp /arma3/server/mods/@a3sql/a3sql_x64.so /arma3/server/a3sql_x64.so 2>/dev/null; exec /usr/local/bin/arma3server"]' \
	ghcr.io/brettmayson/arma3server/arma3server:v3 \
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
