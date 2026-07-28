#!/usr/bin/env python3
"""a3sql-sync — Multi-server sync daemon for a3sql.

Connects to primary and secondary a3sql TCP listeners, fetches all rows
from configured tables on the primary, and replaces the secondary's data
in a single transaction per table.

Usage:
  a3sql-sync --host-primary host1:33306 --host-secondary host2:33306
  a3sql-sync --host-primary 10.0.0.5:33306 --host-secondary 10.0.0.6:33306 --table patch_rules --table players --interval 60
  a3sql-sync --host-primary localhost:33306 --host-secondary localhost:33307 --once --dry-run
"""

import argparse
import json
import socket
import sys
import time


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
    """Read full response from socket until connection closes or buffer empties."""
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
    """Send SQL query, return parsed JSON response."""
    sock.sendall(f"{sql}\n".encode())
    data = recv_response(sock)
    return json.loads(data.decode().strip())


def sync_table(
    primary: socket.socket,
    secondary: socket.socket,
    table: str,
    args: argparse.Namespace,
) -> None:
    """Sync a single table from primary to secondary.

    Fetches all rows from the primary, deletes existing rows on the
    secondary, and inserts the fetched rows in a single transaction.
    Rolls back on any failure.
    """
    print(f"[{time.strftime('%H:%M:%S')}] Syncing {table}...")

    # Fetch from primary
    resp = query(primary, f"SELECT * FROM {table} ORDER BY 1")
    if resp[0] != 0:
        print(f"  ERROR fetching from primary: {resp}")
        return
    rows = resp[2] if len(resp) > 2 else []
    columns = rows[0] if rows else []
    data = rows[1:] if len(rows) > 1 else []

    if args.dry_run:
        print(f"  [DRY-RUN] Would sync {len(data)} rows to {table}")
        return

    # Begin transaction on secondary
    resp = query(secondary, "BEGIN")
    if resp[0] != 0:
        print(f"  ERROR starting transaction: {resp}")
        return

    try:
        # Clear existing data
        resp = query(secondary, f"DELETE FROM {table}")
        if resp[0] != 0:
            raise RuntimeError(f"DELETE failed: {resp}")

        # Insert new data in batches
        if data and columns:
            batch_size = 50
            for i in range(0, len(data), batch_size):
                batch = data[i : i + batch_size]
                values = []
                for row in batch:
                    vals = []
                    for v in row:
                        if v is None:
                            vals.append("NULL")
                        elif isinstance(v, (int, float)):
                            vals.append(str(v))
                        else:
                            escaped = str(v).replace("'", "''")
                            vals.append(f"'{escaped}'")
                    values.append("(" + ", ".join(vals) + ")")
                sql = f"INSERT INTO {table} VALUES {', '.join(values)}"
                resp = query(secondary, sql)
                if resp[0] != 0:
                    raise RuntimeError(f"INSERT failed: {resp}")

        # Commit
        resp = query(secondary, "COMMIT")
        if resp[0] != 0:
            raise RuntimeError(f"COMMIT failed: {resp}")

        print(f"  Synced {len(data)} rows to {table}")

    except Exception:
        # Rollback on failure
        try:
            query(secondary, "ROLLBACK")
        except Exception:
            pass
        raise


def parse_host(hostspec: str) -> tuple[str, int]:
    """Parse host:port string, defaulting to 33306."""
    parts = hostspec.rsplit(":", 1)
    host = parts[0]
    port = int(parts[1]) if len(parts) > 1 else 33306
    return host, port


def main() -> None:
    parser = argparse.ArgumentParser(
        description="a3sql multi-server sync daemon",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  a3sql-sync --host-primary host1:33306 --host-secondary host2:33306\n"
            "  a3sql-sync --host-primary 10.0.0.5:33306 --host-secondary 10.0.0.6:33306 "
            "--table patch_rules --table players --interval 60\n"
            "  a3sql-sync --host-primary localhost:33306 --host-secondary localhost:33307 "
            "--once --dry-run\n"
        ),
    )
    parser.add_argument(
        "--host-primary", required=True, help="Primary server host:port"
    )
    parser.add_argument(
        "--host-secondary", required=True, help="Secondary server host:port"
    )
    parser.add_argument(
        "--table",
        action="append",
        default=[],
        help="Table to sync (repeatable, default: patch_rules)",
    )
    parser.add_argument(
        "--interval",
        type=int,
        default=30,
        help="Poll interval in seconds (default: 30)",
    )
    parser.add_argument("--user", default="", help="TCP listener username")
    parser.add_argument("--password", default="", help="TCP listener password")
    parser.add_argument("--once", action="store_true", help="Run once and exit")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be synced without applying",
    )
    parser.add_argument(
        "--no-color", action="store_true", help="Disable ANSI color output"
    )
    args = parser.parse_args()

    # Default table if none specified
    if not args.table:
        args.table = ["patch_rules"]

    primary_host, primary_port = parse_host(args.host_primary)
    secondary_host, secondary_port = parse_host(args.host_secondary)

    print(
        f"a3sql-sync: {primary_host}:{primary_port} -> {secondary_host}:{secondary_port}"
    )
    print(f"  Tables: {', '.join(args.table)}")
    print(f"  Interval: {'once' if args.once else f'{args.interval}s'}")

    while True:
        try:
            p = connect(primary_host, primary_port, args.user, args.password)
            s = connect(secondary_host, secondary_port, args.user, args.password)

            for table in args.table:
                try:
                    sync_table(p, s, table, args)
                except (RuntimeError, ConnectionError) as e:
                    print(f"  Sync failed for {table}: {e}", file=sys.stderr)

            p.close()
            s.close()

        except ConnectionError as e:
            print(f"Connection error: {e}", file=sys.stderr)
            if args.once:
                sys.exit(1)
        except Exception as e:
            print(f"Sync error: {e}", file=sys.stderr)
            if args.once:
                sys.exit(1)

        if args.once:
            break

        time.sleep(args.interval)
        print(f"[{time.strftime('%H:%M:%S')}] Next sync in {args.interval}s...")


if __name__ == "__main__":
    main()
