# Phase 4: Arma 3 Engine Command Sync
#
# Build:   docker build -t a3sql-command-sync .
# Run:     docker run --rm \
#           -e STEAM_USERNAME=<user> \
#           -e STEAM_PASSWORD=<pass> \
#           [-e STEAM_GUARD_CODE=<code>] \
#           a3sql-command-sync
#
# Produces: /output/commands.json — structured command metadata from the
#            Arma 3 Profiling Branch dedicated server's supportInfo dump.
#
# Base: steamcmd provides a tested Steam client runtime in a container
#       with all the 32-bit libs SteamCMD needs.
FROM steamcmd/steamcmd:ubuntu-22 AS base

LABEL maintainer="a3sql developers"
LABEL description="Arma 3 supportInfo command dumper — syncs engine command metadata"

# ── System dependencies ──────────────────────────────────────────────
# Arma 3 Linux DS requires these 32-bit libs even for the x64 binary.
RUN set -ex && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gosu \
        libstdc++6 \
        lib32stdc++6 \
        libgcc-s1 \
        lib32gcc-s1 \
        libncurses5 \
        lib32ncurses5 \
        libz1 \
        lib32z1 \
        python3 \
        python3-pip \
        screen \
        xz-utils \
        p7zip-full \
    && rm -rf /var/lib/apt/lists/*

# ── User & directories ──────────────────────────────────────────────
RUN useradd -m -s /bin/bash arma3
WORKDIR /home/arma3

# ── Install Arma 3 Dedicated Server (Profiling Branch) ──────────────
# App 233780 = Arma 3 Dedicated Server
# -beta profiling → Profiling Branch (latest engine, has Linux binary)
#
# We use a here-script wrapper so the build doesn't block on Steam
# credentials — credentials are injected at runtime only.
COPY scripts/install_ds.sh /home/arma3/install_ds.sh
RUN chmod +x /home/arma3/install_ds.sh

# Copy all sync scripts
COPY scripts/*.py /home/arma3/
COPY scripts/*.sh /home/arma3/
RUN chmod +x /home/arma3/*.sh

# ── Python dependencies ─────────────────────────────────────────────
# The parser is pure-Python 3.12+ with zero deps, but install
# a minimal environment for future-proofing.
COPY scripts/requirements.txt /home/arma3/requirements.txt
RUN pip3 install --no-cache-dir -r /home/arma3/requirements.txt

# ── Armake for PBO creation ─────────────────────────────────────────
# We need to create a minimal mission PBO to auto-execute supportInfo
# on server start. armake is the standard Linux tool.
RUN set -ex && \
    curl -fsSL https://github.com/KoffeinFlummi/armake/releases/download/v0.10.0/armake_v0.10.0_linux_amd64.tar.gz \
    | tar xz -C /usr/local/bin/ armake && \
    chmod +x /usr/local/bin/armake

# ── Output directory ────────────────────────────────────────────────
RUN mkdir -p /output && chown arma3:arma3 /output /home/arma3

# ── Server config template ──────────────────────────────────────────
# The server.cfg is generated at runtime with the random port.
COPY scripts/server.cfg.template /home/arma3/server.cfg.template

# ── Mission template (init.sqf) ────────────────────────────────────
# This SQF runs on mission start, dumps supportInfo to a file and RPT.
COPY scripts/support_dump_init.sqf /home/arma3/support_dump_init.sqf

# ── Volume for DS installation ──────────────────────────────────────
# Persist between runs to avoid re-downloading the 1.5GB+ DS every time.
VOLUME ["/home/arma3/ds", "/home/arma3/.steam", "/output"]

# ── Entrypoint ──────────────────────────────────────────────────────
ENTRYPOINT ["/home/arma3/entrypoint.sh"]
