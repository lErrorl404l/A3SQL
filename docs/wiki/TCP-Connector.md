# TCP Connector

The TCP connector is how tools outside Arma talk to the in-game database:
query players, pull stats, run admin queries, all while the mission is live.
This page is for the server operator and for whoever writes the tool that
connects.

## Enabling

The listener starts automatically when A3SQL loads on the server, as long as
**Enable TCP Listener** is on (CBA setting, default on). It listens on port
`33306` by default (**Listener Port**). The in-game listener always binds to
`127.0.0.1`, so only processes on the same machine as Arma can reach it. To
accept connections from other machines, run the
[standalone server](Standalone-Server) with `--bind 0.0.0.0`.

To control the listener from SQF (if auto-start is off):

```sqf
["listen", ["33307"]] call a3sql_fnc_execute;  // start on port 33307
["stop"] call a3sql_fnc_execute;               // stop
```

## Logging in

The listener is fail-closed: **every connection must `LOGIN` first**. Set
**Listener Username** and **Listener Password** in CBA Settings. Without them
the listener rejects all connections with a message telling you to configure
them. See [Security](Security) for the full model.

```
> LOGIN admin secret123
< [0,"OK","Authenticated"]
```

## The protocol

Plain text, one query per line, one response per query:

```
> PING
< [0,"OK","PONG"]

> CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)
< [0,"OK",""]

> INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)
< [0,"OK",""]

> SELECT name, score FROM players ORDER BY score DESC
< [0,"OK",[["name","score"],["Scarface",1500]]]

> QUIT
< (connection closes)
```

Responses are JSON-style envelopes: `[code, status, payload]`. `0` with
`"OK"` means success; anything else is an error and the payload explains it.

You can batch several statements on one line, separated by `;`. They run in
order and the response carries the accumulated results.

## Commands

| Command | Effect |
|---|---|
| `LOGIN <user> <pass>` | Authenticate. Required on every connection. |
| `PING` | Returns `[0,"OK","PONG"]`. Useful as a keepalive. |
| `QUIT` / `EXIT` | Close the connection. |
| Any SQL | Run it and return the result. |

## Reading responses: do and do not

Each query produces exactly one line. Read it before you do anything else
with the connection:

```python
import socket

def query(sql, user="admin", password="secret123"):
    s = socket.create_connection(("127.0.0.1", 33306), timeout=5)
    s.sendall(f"LOGIN {user} {password}\n".encode())
    print(s.recv(65536).decode())          # [0,"OK","Authenticated"]
    s.sendall((sql + "\n").encode())
    resp = s.recv(65536).decode()
    s.close()
    return resp

print(query("SELECT name, score FROM players ORDER BY score DESC LIMIT 5"))
```

Do: send one query, read its response line, then reuse the socket or close
it. Do not: send a query and close the socket without reading. The response
arrives on the socket, and closing first loses it. From the server's view
that looks like a dropped query.

## Big results: paginate with cursors

A single response is capped at 30 KB, Arma's `callExtension` ceiling. A query
whose result would exceed that fails loudly with a cursor hint, never a
silent truncation. Page through big result sets with cursors:

```
> cursor create all_players SELECT * FROM players ORDER BY score DESC
< [0,"OK","Cursor 'all_players' created"]

> cursor fetch all_players 100
< [0,"OK",[... first 100 rows ...]]

> cursor fetch all_players 100
< [0,"OK",[... next 100 rows ...]]

> cursor drop all_players
< [0,"OK",""]
```

Commands: `cursor create <name> <query>`, `cursor fetch <name> [limit]`,
`cursor drop <name>`.

## Examples

The Python snippet above is the reference client. Same thing from the shell
with netcat:

```bash
echo "SELECT name FROM players" | nc 127.0.0.1 33306
```

or with curl's telnet support:

```bash
printf 'LOGIN admin secret123\nSELECT name FROM players\nQUIT\n' | curl telnet://127.0.0.1:33306
```

Node.js:

```javascript
const net = require("net");
const client = net.createConnection({ port: 33306 }, () => {
    client.write("LOGIN admin secret123\n");
    client.write("SELECT * FROM players ORDER BY score DESC\n");
});
client.on("data", (data) => {
    console.log(data.toString());
    client.end();
});
```

## Concurrency

The listener spawns a thread per connection, so a slow client does not block
others. Queries serialize only on the database itself. Do not open dozens of
connections from one tool; one persistent, reused connection is the right
shape.

## Security note

The listener is loopback-only and fail-closed. On a shared host, any process
running as the same user as Arma can still reach 127.0.0.1 and attempt to
log in, so set a real username and password. Network exposure is a
standalone-server decision; the in-game listener cannot bind beyond
localhost. See [Security](Security).
