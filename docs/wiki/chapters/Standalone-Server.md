# Standalone Server

`a3sql-server` runs the same database engine without Arma 3. Same SQL, same
save/load, same everything: it shares the code path with the in-game
extension. Use it to test SQL before shipping a mission, to run a database
your tools can reach across the network, or as a scratch server while
developing.

## Running

```bash
# Default: port 33306, localhost only, in-memory
a3sql-server

# Custom port
a3sql-server --port 33307

# Save to a file: load at start, auto-save every 30 s, save on shutdown
a3sql-server --db data.bin

# Reachable from other machines
a3sql-server --bind 0.0.0.0 --port 33306

# Interactive prompt
a3sql-server --interactive
```

| Flag | What it does |
|---|---|
| `--port, -p <PORT>` | Listen port (default `33306`) |
| `--bind, -b <IP>` | Bind address (default `127.0.0.1`) |
| `--db, -d <PATH>` | Persist to file: load at start, auto-save every 30 s, final save on shutdown. Absolute and relative paths both work. |
| `--interactive, -i` | Read SQL from the console instead of only serving TCP |
| `--help, -h` | Show options, config file location, and the auth requirement |

## Interactive mode

`a3sql-server --interactive` drops you into a REPL. Type SQL or a server
command (`PING`, `SAVE`, `LOAD`) and press Enter; the result prints on the
next line.

```
$ a3sql-server --interactive
a3sql> CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)
[0,"OK",""]
a3sql> INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)
[0,"OK",""]
a3sql> SELECT * FROM players
[0,"OK",[...]]
a3sql> :q
```

`a3sql> ` is the prompt. `:help` lists the commands; `:exit`, `:quit`, `:q`
(or plain `exit` / `quit`) leave the REPL. Ctrl+C shuts the server down
cleanly and saves the database if `--db` is set.

## Configuration

The server reads `$A3SQL_CONFIG` or, failing that, `./a3sql.toml` in the
working directory. Config is optional; a missing file is not an error.

| Key | What it does |
|---|---|
| `listener_require_auth` | Require `LOGIN` on every connection. Default `true` (fail-closed). Set `false` for anonymous access on a trusted loopback deployment. |
| `data_dir` | Directory for file commands (`SAVE`, `LOAD`, `export_to_file`). Default `./a3sql_data/`. |
| `game_version` | Expected Arma 3 version, e.g. `"2.22"`. When set, the engine compares it to the arma3-wiki command data version and logs a warning on mismatch. The SQF command database is sourced from the [arma3-wiki](https://crates.io/crates/arma3-wiki) crate (acemod/arma3-wiki `dist-2` branch, 6-hour cache, fallback to embedded data). Useful when you run the engine against a newer game than the wiki data covers. |

```toml
# a3sql.toml
listener_require_auth = true
data_dir = "./a3sql_data"
game_version = "2.22"
```

A fully-commented template ships as `a3sql.toml.example` in the repository
root — copy it next to the binary and edit.

The server and the in-game extension read the same config format.

## Connecting

The server speaks the same TCP protocol as the in-game listener, including
`LOGIN` (required by default), `PING`, and cursor commands. See
[TCP Connector](TCP-Connector).

```bash
printf 'LOGIN admin secret123\nSELECT name FROM players\nQUIT\n' | nc 127.0.0.1 33306
```

## Connecting from the game

The game extension can point at a standalone server instead of executing
locally:

```sqf
["connect 192.168.1.100 33306"] call a3sql_fnc_execute;
_result = ["SELECT * FROM players"] call a3sql_fnc_execute;
["disconnect"] call a3sql_fnc_execute;
```

While connected, SQL is forwarded to the remote server and the local database
is untouched. `disconnect` switches back to local mode.

## Building from source

*For developers. Server operators should use a release binary.*

```bash
cargo build --release --manifest-path extension/Cargo.toml --bin a3sql-server
./extension/target/release/a3sql-server --help
```
