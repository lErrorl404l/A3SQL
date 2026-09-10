# Security

A3SQL's security model has four parts: parameterized queries against SQL
injection, a TCP login that fails closed, file-access guards around disk
commands, and hard limits that fail loudly instead of silently corrupting or
truncating data. This page is for server operators and mod developers; the
operator-facing rules are the short list at the end.

## 1. Parameterized queries (for mod developers)

The engine's primary defense against SQL injection. Pass user input as
separate `callExtension` arguments instead of building SQL strings:

```sqf
// UNSAFE: string interpolation lets input change the query
_uid = "foo' OR '1'='1";
_result = [format ["DELETE FROM players WHERE uid = '%1'", _uid]] call a3sql_fnc_execute;

// SAFE: $1 is a placeholder the extension escapes
_uid = "foo' OR '1'='1";
_result = ["a3sql", "DELETE FROM players WHERE uid = $1", [_uid]] callExtension;
```

Placeholders `$1`, `$2`, ... are substituted before the SQL is parsed. The
escaping rules:

| Input | Becomes |
|---|---|
| Empty string | `''` |
| `NULL` / `null` | `NULL` |
| Integer / float | As-is |
| `true` / `false` | As-is |
| Everything else | Wrapped in single quotes, `'` doubled to `''` |

## 2. TCP login (for operators)

The TCP listener is **fail-closed**: every connection must log in. In plain
terms:

- Set **Listener Username** and **Listener Password** in CBA Settings (or via
  `set_credentials` before starting the listener).
- A client that sends the correct `LOGIN <user> <pass>` gets
  `[0,"OK","Authenticated"]` and can then run queries.
- A client that fails login is disconnected immediately.
- A client that sends anything other than `LOGIN` first is disconnected with
  `[-1,"ERR_AUTH","LOGIN <user> <pass> required"]`.
- With no credentials configured, the listener refuses all connections with
  `[-1,"ERR_AUTH","No credentials configured - set listener_user/password in CBA settings"]`.
  It never falls back to open access.

```
> LOGIN admin secret123
< [0,"OK","Authenticated"]
> SELECT * FROM players
< [0,"OK",...]

> LOGIN bad wrong
< [-1,"ERR_AUTH","Invalid credentials"]

> SELECT * FROM players
< [-1,"ERR_AUTH","LOGIN <user> <pass> required"]
```

Two implementation details worth knowing:

- **Constant-time comparison.** Username and password are compared with a
  constant-time byte compare that does not exit early on the first mismatch,
  so a local attacker cannot probe credentials byte by byte using response
  timing.
- **Fail-closed is the default.** `listener_require_auth` in `a3sql.toml`
  defaults to `true`. Set it to `false` only for a trusted loopback-only
  deployment that genuinely wants anonymous access.

Credentials protect the network path only. SQF running in the same game
client can always reach the database directly through the extension call
interface; that is accepted by design, because SQF already owns the process.

### Remote server mode

When the game extension forwards queries to a remote server (`connect`), the
forwarding side adds no separate authentication; the remote server applies
its own login rules, so keep its credentials enforced. See
[Standalone Server](Standalone-Server).

## 3. File access (SAVE, LOAD, export_to_file)

All file commands resolve against the data directory (`a3sql_data/` by
default) and refuse to escape it:

- Absolute paths are rejected.
- `..` and `~` in a path are rejected.
- Symlinks are checked both ways: a symlinked subdirectory and a pre-placed
  symlink at the target are both refused if they resolve outside the data
  directory.
- On Unix, writes use `O_NOFOLLOW`, closing the race between the check and
  the write.

Saves are crash-safe: write to a temp file, rename into place, keep the
previous good save as `.bak`, and verify an FNV-1a checksum on load. A
corrupt or missing main save falls back to `.bak`. A failed load leaves the
in-memory database untouched.

## 4. Hard limits that fail loudly

- **30 KB response ceiling.** Arma's `callExtension` buffer is 30 KB. Any
  response that would exceed it returns an explicit error with a cursor hint
  (`cursor create <name> <query>` plus `cursor fetch <name> [limit]`), on
  both the in-game path and the TCP path. There is no silent truncation: a
  response is either complete or an error.
- **Trigger depth.** Recursive triggers are capped at 16 levels of nesting; a
  deeper chain is refused instead of overflowing the stack.
- **SQF literal depth.** Deeply nested array literals are capped at 128
  levels.

## Operator checklist

1. Set Listener Username and Listener Password before the listener matters.
2. Keep the listener loopback-only; use the standalone server's `--bind` for
   network access.
3. Back up the data directory (`a3sql_data/`).
4. In missions, pass player-supplied values through `$1` placeholders.
