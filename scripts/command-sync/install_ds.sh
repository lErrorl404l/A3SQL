#!/bin/bash
# ── Arma 3 DS Install Helper ──────────────────────────────────────────
#
# Installs/updates Arma 3 Dedicated Server (Profiling Branch) via SteamCMD.
# Called during Docker build with anonymous login (runtime login for actual
# download happens in entrypoint.sh).
#
# Build-time:  downloads + caches DS in the image layer
# Runtime:     entrypoint.sh runs the full pipeline with credentials

set -euo pipefail

DS_DIR="${1:-/home/arma3/ds}"
STEAMCMD="${2:-/home/arma3/steamcmd/steamcmd.sh}"

# At build time we can only do anonymous login — the DS download requires
# a Steam account, so this is a NO-OP placeholder. The real download
# happens at runtime via entrypoint.sh with credentials from secrets.
echo "[install_ds] Build-time DS install placeholder."
echo "[install_ds] DS_DIR=${DS_DIR}"
echo "[install_ds] Full download deferred to runtime (entrypoint.sh)"
echo "[install_ds] To pre-cache: run with STEAM_USERNAME and STEAM_PASSWORD"

# Create DS directory so the volume mount works
mkdir -p "${DS_DIR}"
