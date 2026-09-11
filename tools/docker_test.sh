#!/bin/bash
# A3SQL headless docker test — self-contained in the repo.
#
# Builds the a3sql mod from source, downloads CBA if needed, runs the
# Arma 3 dedicated server (brettmayson/arma3server) with the test
# mission, captures the RPT output and verifies the phase results.
#
# The Arma 3 server binary is NOT in this repo. Provide it one of two
# ways:
#   1. ARMA3_SERVER_ROOT=/path/to/your/arma3server  (recommended)
#   2. Steam credentials in tests/docker/config.toml [steam] with
#      ARMA3_SERVER__SKIP_INSTALL=false so the container installs it.
#
# Optional mods for the full ballistics mission:
#   MOD_ACE_PATH=/path/to/@ace
#   MOD_MSS_PATH=/path/to/@mss
#
# Usage: tools/docker_test.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOCKER="$ROOT/tests/docker"
MODS="$DOCKER/mods"
CBA_VERSION="v3.19.0"

# Arma 3 server install: user-provided, or a repo-local (gitignored) dir.
export ARMA3_SERVER_ROOT="${ARMA3_SERVER_ROOT:-$DOCKER/server}"
if [ ! -d "$ARMA3_SERVER_ROOT" ] || [ -z "$(ls -A "$ARMA3_SERVER_ROOT" 2>/dev/null)" ]; then
	echo "==> ARMA3_SERVER_ROOT ($ARMA3_SERVER_ROOT) is empty."
	echo "    Point it at your Arma 3 server install, or set steam"
	echo "    credentials in tests/docker/config.toml and export"
	echo "    ARMA3_SERVER__SKIP_INSTALL=false to install via steamcmd."
	exit 2
fi

# ── Teardown on exit ──────────────────────────────────────────────────────
clean_profiles() { docker run --rm -v "$DOCKER/configs:/c" alpine rm -rf /c/profiles 2>/dev/null || true; }
trap 'docker compose -f "$DOCKER/docker-compose.yml" down 2>/dev/null || true; clean_profiles' EXIT

echo "==> hemtt build"
(cd "$ROOT" && hemtt build >/dev/null)

echo "==> assemble @a3sql"
rm -rf "$MODS/@a3sql"
mkdir -p "$MODS/@a3sql/addons"
cp "$ROOT"/.hemttout/build/addons/*.pbo "$MODS/@a3sql/addons/"
cp "$ROOT"/.hemttout/build/mod.cpp "$MODS/@a3sql/mod.cpp"
cp "$ROOT"/.hemttout/build/meta.cpp "$MODS/@a3sql/meta.cpp"
# The a3sql extension must be in the mod dir for the entrypoint to copy
cp "$ROOT"/.hemttout/build/a3sql_x64.so "$MODS/@a3sql/" 2>/dev/null ||
	cp "$ROOT/target/x86_64-unknown-linux-gnu/release/liba3sql.so" "$MODS/@a3sql/a3sql_x64.so" 2>/dev/null ||
	{
		echo "!! extension binary not found (cargo build --release first)"
		exit 1
	}

echo "==> ensure @cba_a3"
if [ ! -d "$MODS/@cba_a3" ]; then
	curl -fsSL "https://github.com/CBATeam/CBA_A3/releases/download/${CBA_VERSION}/CBA_A3_${CBA_VERSION}.zip" -o /tmp/cba.zip
	unzip -oq /tmp/cba.zip -d "$MODS"
	rm /tmp/cba.zip
	[ -d "$MODS/@CBA_A3" ] && mv "$MODS/@CBA_A3" "$MODS/@cba_a3"
fi

echo "==> docker compose up"
docker compose -f "$DOCKER/docker-compose.yml" up -d --force-recreate

echo "==> waiting for results (up to 180 s)"
for _ in $(seq 1 36); do
	if docker compose -f "$DOCKER/docker-compose.yml" logs 2>/dev/null | grep -q "TEST END"; then
		break
	fi
	sleep 5
done

echo "==> capturing log"
docker compose -f "$DOCKER/docker-compose.yml" logs >"$DOCKER/run.log" 2>&1

echo "==> verifying"
python3 "$DOCKER/verify.py" "$DOCKER/run.log" ||
	{
		echo "harness failed; full log at tests/docker/run.log"
		exit 1
	}

echo "==> teardown"
docker compose -f "$DOCKER/docker-compose.yml" down 2>/dev/null || true
clean_profiles
