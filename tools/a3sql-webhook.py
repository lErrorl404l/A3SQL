#!/usr/bin/env python3
"""a3sql-webhook — Webhook notification daemon for a3sql.

Monitors the server_commands table via a3sql TCP listener and sends
Discord/Slack webhooks for executed admin commands.

Usage:
  a3sql-webhook --host localhost --port 33306 \\
      --webhook-url "https://discord.com/api/webhooks/..." \\
      [--interval 10] [--table server_commands] \\
      [--user admin] [--password secret] [--once]
"""

import argparse
import json
import socket
import sys
import time
import urllib.error
import urllib.request


def connect(
    host: str, port: int, user: str = "", password: str = "", timeout: int = 5
) -> socket.socket:
    """Connect to a3sql TCP listener and authenticate."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(timeout)
    s.connect((host, port))
    if user:
        s.sendall(f"LOGIN {user} {password}\n".encode())
        resp = s.recv(4096).decode().strip()
        if '"OK"' not in resp:
            s.close()
            raise ConnectionError(f"Login failed: {resp}")
    return s


def recv_response(sock: socket.socket) -> bytes:
    """Read full response from socket."""
    data = b""
    while True:
        chunk = sock.recv(65536)
        if not chunk:
            break
        data += chunk
        if len(chunk) < 65536:
            break
    return data


def query(sock: socket.socket, sql: str) -> list:
    """Send SQL, return parsed JSON response."""
    sock.sendall(f"{sql}\n".encode())
    data = recv_response(sock)
    return json.loads(data.decode().strip())


def send_webhook(url: str, payload: dict) -> int:
    """POST webhook payload, return HTTP status code."""
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
            "User-Agent": "a3sql-webhook/1.0",
        },
    )
    try:
        resp = urllib.request.urlopen(req, timeout=10)
        return resp.status
    except urllib.error.HTTPError as e:
        return e.code


def build_discord_payload(
    cmd_id: int, command: str, params: str, status: str, ts: str
) -> dict:
    """Build Discord embed payload."""
    return {
        "embeds": [
            {
                "title": "Admin Command Executed",
                "color": 3447003,
                "fields": [
                    {"name": "Command", "value": str(command), "inline": True},
                    {"name": "Params", "value": str(params), "inline": True},
                    {"name": "Status", "value": str(status), "inline": True},
                    {"name": "ID", "value": str(cmd_id), "inline": True},
                ],
                "timestamp": ts,
            }
        ]
    }


def build_slack_payload(command: str, params: str, status: str) -> dict:
    """Build Slack fallback payload."""
    return {"text": f"Admin command executed: {command} {params} ({status})"}


def process_rows(
    rows: list[list], sock: socket.socket, table: str, webhook_url: str, col_index: dict
) -> None:
    """Send webhooks for un-notified rows and mark them notified."""
    for row in rows:
        if not isinstance(row, list) or len(row) < 5:
            continue

        cmd_id = row[col_index["id"]]
        command = row[col_index["command"]]
        params = row[col_index["params"]]
        status = row[col_index["status"]]
        ts = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

        # Try Discord embed first
        payload = build_discord_payload(cmd_id, command, params, status, ts)
        code = send_webhook(webhook_url, payload)

        # Fallback to Slack format on non-200
        if code not in (200, 204):
            payload = build_slack_payload(command, params, status)
            code = send_webhook(webhook_url, payload)

        if code in (200, 204):
            query(sock, f"UPDATE {table} SET notified=1 WHERE id={cmd_id}")
            print(f"  Notified: #{cmd_id} {command} {params} ({status})")
        else:
            print(f"  Webhook returned {code} for #{cmd_id} {command}")


def build_col_index(headers: list[str]) -> dict:
    """Map column names to their index for position-independent access."""
    names = [h.lower() for h in headers]
    return {
        "id": names.index("id") if "id" in names else 0,
        "command": names.index("command") if "command" in names else 1,
        "params": names.index("params") if "params" in names else 2,
        "status": names.index("status") if "status" in names else 4,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="a3sql webhook notification daemon",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  a3sql-webhook --webhook-url https://discord.com/api/webhooks/...\n"
            "  a3sql-webhook --host 10.0.0.5 --port 33306 --user admin --password s3cret "
            "--webhook-url https://hooks.slack.com/... --interval 30\n"
            "  a3sql-webhook --once --webhook-url https://discord.com/api/webhooks/...\n"
        ),
    )
    parser.add_argument("--host", default="localhost", help="a3sql TCP host")
    parser.add_argument("--port", type=int, default=33306, help="a3sql TCP port")
    parser.add_argument(
        "--webhook-url", required=True, help="Discord/Slack webhook URL"
    )
    parser.add_argument(
        "--interval", type=int, default=10, help="Poll interval in seconds"
    )
    parser.add_argument("--table", default="server_commands", help="Table to watch")
    parser.add_argument("--user", default="", help="TCP listener username")
    parser.add_argument("--password", default="", help="TCP listener password")
    parser.add_argument(
        "--once", action="store_true", help="Process pending notifications and exit"
    )
    args = parser.parse_args()

    print(
        f"a3sql-webhook: polling {args.host}:{args.port} "
        f"every {'once' if args.once else f'{args.interval}s'}"
    )
    print(f"  Webhook: {args.webhook_url}")
    print(f"  Table: {args.table}")

    while True:
        try:
            s = connect(args.host, args.port, args.user, args.password)
            resp = query(
                s,
                f"SELECT * FROM {args.table} "
                f"WHERE notified IS NULL AND status='executed' ORDER BY id",
            )

            if resp[0] == 0 and len(resp) > 2:
                data = resp[2]
                if isinstance(data, list) and len(data) > 1:
                    headers = data[0] if isinstance(data[0], list) else []
                    rows = data[1:]
                    if rows:
                        col_index = build_col_index(headers)
                        process_rows(rows, s, args.table, args.webhook_url, col_index)

            s.close()

        except (ConnectionError, socket.timeout, OSError) as e:
            print(f"Connection error: {e}", file=sys.stderr)
            if args.once:
                sys.exit(1)
        except json.JSONDecodeError as e:
            print(f"Parse error: {e}", file=sys.stderr)
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            if args.once:
                sys.exit(1)

        if args.once:
            break

        time.sleep(args.interval)


if __name__ == "__main__":
    main()
