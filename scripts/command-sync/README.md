# Arma 3 Engine Command Sync

Syncs command metadata from the **actual game binary** (Arma 3 Profiling
Branch dedicated server) via `supportInfo ""` dump. Produces structured JSON
matching the `CmdInfo`/`Arity`/`ReturnType` shapes in
`extension/src/engine/sqf/database.rs`.

This is the **authoritative source** — not community-maintained wiki data.

## Pipeline

```
SteamCMD → Arma 3 DS (Profiling Branch) → supportInfo "" → RPT → JSON
```

1. SteamCMD downloads App 233780 (`-beta profiling`)
2. A minimal mission PBO with `init.sqf` calls `supportInfo ""` on startup
3. The server boots headless, runs the mission, logs command metadata to RPT
4. Extraction script greps `n:` / `u:` / `b:` lines from the RPT
5. `support_info_to_json.py` parses the raw text into JSON

## Docker

### Build

```bash
docker build -t a3sql-command-sync -f scripts/command-sync/Dockerfile .
```

### Run

```bash
docker run --rm \
  -e STEAM_USERNAME="$STEAM_USERNAME" \
  -e STEAM_PASSWORD="$STEAM_PASSWORD" \
  [-e STEAM_GUARD_CODE="$STEAM_GUARD_CODE"] \
  -v /tmp/output:/output \
  a3sql-command-sync
```

Output written to `/tmp/output/commands.json`.

## Required Secrets

| Secret | Description |
|--------|-------------|
| `STEAM_USERNAME` | Steam account username (must own Arma 3) |
| `STEAM_PASSWORD` | Steam account password |
| `STEAM_GUARD_CODE` | Steam Guard 2FA code (if Steam Guard is enabled) |

> **Security:** Credentials are never stored in the repo. They are injected
> at runtime via environment variables. The account needs only to own
> Arma 3 (App 107410) to download the dedicated server.

## Python Parser

Standalone script that converts raw supportInfo text to JSON:

```bash
# From file
python3 scripts/command-sync/support_info_to_json.py \
  --file support_info.txt --output commands.json --pretty

# From stdin
cat support_info.txt | python3 scripts/command-sync/support_info_to_json.py
```

### Input format

```
n:true
n:false
u:sqrt
    Type: Number
u:toupper
    Type: String
b:min
    Type: Number
```

### Output format

```json
[
  {"name": "sqrt", "arity": "unary", "ret": "Number", "groups": ["Engine"]},
  {"name": "toupper", "arity": "unary", "ret": "String", "groups": ["Engine"]}
]
```

Maps directly to the Rust `CmdInfo` struct:

| JSON field | Rust type | Notes |
|-----------|-----------|-------|
| `name` | `String` | Lowercased command name |
| `arity` | `Arity` | `"nular"` / `"unary"` / `"binary"` |
| `ret` | `ReturnType` | `"Number"`, `"String"`, `"Boolean"`, `"Array"`, `"Nothing"`, `"Other"` |
| `groups` | `Vec<String>` | Always `["Engine"]` — wiki groups come from arma3-wiki |

## Output files

| File | Description |
|------|-------------|
| `commands_engine.json` | Latest parsed output (checked into repo) |
| `commands_previous.json` | Snapshot from previous run (for change detection) |

## GitHub Actions

The `command-sync.yml` workflow runs:

- **Scheduled:** Every Monday at 06:00 UTC
- **Manual:** Via `workflow_dispatch` (with optional `pretest` mode)

On detecting changes, it creates a PR titled
"Sync: engine command metadata update (YYYY-MM-DD)".

## Change detection

The pipeline compares the new output against the previous snapshot
(`commands_previous.json`) and sets a `.changed` flag. The workflow only
creates a PR when actual differences exist — no noise for identical dumps.

## Edge cases handled

| Case | Handling |
|------|----------|
| Multi-line entries | Continuation lines (indented) collected and parsed for `Type:` |
| Deprecated commands | Flagged with `"flags": ["deprecated"]` |
| Unknown arity | Skipped with warning |
| Empty input | Exits gracefully with error |
| Deduplication | Last arity wins per command name |
| SteamCMD failure | Retried up to 3 times with exponential backoff |
| Server timeout | 5-minute max wait, graceful kill |

## Limitations

- Requires a Steam account that owns Arma 3
- Steam Guard may require manual 2FA code
- Server boot takes 1-3 minutes
- The Linux DS produces a slightly different RPT format than Windows — the
  extraction handles both but may need tuning for new Arma versions
- Groups are always `["Engine"]` since supportInfo doesn't include wiki
  category groups — those still come from the arma3-wiki crate
