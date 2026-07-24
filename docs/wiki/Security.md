# Security

A3DB provides two layers of security: **parameterized queries** for SQL injection prevention and **TCP login authentication** for network access.

## Parameterized Queries

The primary SQL injection prevention mechanism. Pass user input as separate `callExtension` arguments rather than interpolating into SQL:

```sqf
// ❌ Unsafe — string interpolation allows injection
_uid = "foo' OR '1'='1";
_result = [format ["DELETE FROM players WHERE uid = '%1'", _uid]] call a3db_fnc_execute;

// ✅ Safe — $1 placeholder gets escaped by the extension
_uid = "foo' OR '1'='1";
_result = ["a3db", "DELETE FROM players WHERE uid = $1", [_uid]] call a3db_fnc_execute;
```

**Escaping rules** (applied by the Rust extension before SQL parsing):

| Input type | Behavior |
|-----------|----------|
| Empty string | Rendered as `''` (empty string literal) |
| `NULL` / `null` | Rendered as bare `NULL` |
| Integer / float | Passed through as-is |
| `true` / `false` | Passed through as-is |
| Already quoted | Passed through as-is (`'foo'` stays `'foo'`) |
| Everything else | Wrapped in single quotes with `'` doubled to `''` |

The extension processes placeholders (`$1`, `$2`, etc.) **before SQL parsing** — the substituted SQL is never exposed to the caller.

## TCP Login Authentication

When the TCP listener is enabled, external connections can be protected with username/password authentication.

### Setup

Set credentials via CBA Settings or in SQF before starting the listener:

```sqf
// In fn_settings.sqf (PreInit) or from a mission
["a3db_listener_user", "admin"] call CBA_fnc_setVar;
["a3db_listener_password", "secret123"] call CBA_fnc_setVar;
["a3db_listener_enabled", true] call CBA_fnc_setVar;
```

When credentials are set, the listener requires every TCP client to authenticate first.

### Login Protocol

```
> LOGIN admin secret123
< [0,"OK","Authenticated"]
```

If authentication fails:

```
> LOGIN bad wrong
< [-1,"ERR_AUTH","Invalid credentials"]
```

Clients that fail authentication are disconnected immediately. Clients that don't send `LOGIN` at all:

```
> SELECT * FROM players
< [-1,"ERR_AUTH","LOGIN <user> <pass> required"]
```

### Anonymous Access

Leave both `a3db_listener_user` and `a3db_listener_password` empty (default) for anonymous access — no `LOGIN` command needed.

### Remote Server Mode

When using the extension's `connect` command to forward queries to a remote a3db-server, the TCP connection itself is unauthenticated (the remote server's local credentials check applies if configured). Future versions may add credential forwarding.

### Response Format

All security errors follow the standard response format:

```
[-1,"ERR_AUTH","description of error"]
```
