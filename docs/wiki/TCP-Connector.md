# TCP Connector

A3DB exposes a lightweight TCP interface that lets external tools query the in-game database while the game is running.

## Enabling

The TCP listener starts automatically at game boot when `a3sql_listener_enabled` is true (CBA Setting, defaults to on). It binds to `127.0.0.1:33306` by default.

```sqf
// Manual start from SQF (if auto-start is disabled)
["listen", ["33307"]] call a3sql_fnc_execute;

// With explicit binding
["listen 33307"] call a3sql_fnc_execute;

// Stop
["stop"] call a3sql_fnc_execute;
```

## Protocol

Each TCP connection handles one or more SQL queries. The protocol is line-based, newline-delimited:

```
> CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)
< [0,"OK",""]

> INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)
< [0,"OK",""]

> SELECT * FROM players ORDER BY score DESC
< [0,"OK",[["uid","name","score"],[["76561198000000001","Scarface",1500]]]]

> QUIT
< (connection closes)
```

## Control Commands

| Command | Effect |
|---------|--------|
| `PING` | Returns `[0,"OK","PONG"]` (keepalive) |
| `QUIT` / `EXIT` | Disconnect |
| `LOGIN <user> <pass>` | Authenticate (only if credentials configured) |
| Any SQL | Execute and return JSON result |

## Multi-client Support

The TCP listener spawns a thread per connection. Slow queries from one client don't block others — all queries serialize only on the database mutex.

## Authentication

If CBA credentials are set (`a3sql_listener_user` / `a3sql_listener_password`), the first message MUST be `LOGIN`:

```
> LOGIN admin secret123
< [0,"OK","Authenticated"]
> SELECT * FROM players
< [0,"OK",...]
```

See [Security](Security) for details.

## Examples

### Python

```python
import socket

def query(sql):
    s = socket.socket()
    s.settimeout(5)
    s.connect(("127.0.0.1", 33306))
    s.sendall((sql + "\n").encode())
    resp = s.recv(65536).decode()
    s.close()
    return resp

print(query("SELECT name, score FROM players ORDER BY score DESC LIMIT 5"))
```

### Node.js

```javascript
const net = require("net");
const client = net.createConnection({ port: 33306 }, () => {
    client.write("SELECT * FROM players ORDER BY score DESC\n");
});
client.on("data", (data) => {
    console.log(JSON.parse(data));
    client.end();
});
```

### bash / CLI

```bash
echo "SELECT name FROM players" | nc localhost 33306
```

## Security Note

By default the listener binds to `127.0.0.1` (localhost only). To allow network access, change `a3sql_listener_bind` to `0.0.0.0` — but set a username/password first.
