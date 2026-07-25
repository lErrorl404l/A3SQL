# Standalone Server

A3SQL can run as an independent TCP server outside of Arma 3. This is useful for testing, external tooling, or as a central database that multiple game clients connect to.

## Usage

```bash
# Default (port 33306, localhost only, in-memory)
a3sql-server

# Custom port
a3sql-server --port 33307

# Persist database to file
a3sql-server --db /path/to/database.bin

# Network-accessible
a3sql-server --bind 0.0.0.0 --port 33306
```

## Connecting

```bash
# From any TCP client (Python, bash, etc.)
echo "SELECT * FROM players" | nc localhost 33306
```

## Connecting from the Game Extension

The game extension can forward queries to a remote a3sql-server instead of executing locally:

```sqf
// Connect to remote server
["connect 192.168.1.100 33306"] call a3sql_fnc_execute;

// Queries now execute on the remote server
_result = ["SELECT * FROM players"] call a3sql_fnc_execute;

// Switch back to local mode
["disconnect"] call a3sql_fnc_execute;
```

When connected to a remote server, all SQL queries are forwarded transparently. The local database is untouched while connected. Switch back to local mode with `disconnect`.

## Server Commands

| Command | Effect |
|---------|--------|
| `PING` | Returns `[0,"OK","PONG"]` |
| `QUIT` | Disconnects client |
| Any SQL | Executes and returns JSON |

## Building from Source

```bash
cargo build --release --bin a3sql-server
./target/release/a3sql-server --help
```
