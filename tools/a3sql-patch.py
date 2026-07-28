#!/usr/bin/env python3
"""a3sql-patch — Remote CLI client for the a3sql patch system.

Connects to the a3sql TCP listener and manages patch rules via SQL
against the patch_rules table.

Usage:
  a3sql-patch [--host HOST] [--port PORT] [--user USER] [--password PASS] <command> [args]

Commands:
  list                        List all patch rules
  get <id>                    Get a single rule by ID
  add <target_type> <property> <value> [--name NAME] [--active BOOL] [--priority N] [--operator OP] [--group GROUP]
  update <id> [--name NAME] [--active BOOL] [--priority N] [--target-type TYPE] [--property PROP] [--operator OP] [--value VAL] [--group GROUP]
  delete <id>                 Delete a rule
  group <name>                List rules in a group
  group-activate <name>       Activate all rules in a group
  group-deactivate <name>     Deactivate all rules in a group
  presets                     List saved presets
  save-preset <name>          Save current rules as a preset
  load-preset <name>          Load rules from a preset
  delete-preset <name>        Delete a preset
  query <sql>                 Execute arbitrary SQL query against patch_rules
  watch                       Stream changes in real-time (poll every 2 sec)
  version                     Print client version and server version
"""

from __future__ import annotations

import argparse
import json
import socket
import sys
import textwrap
import time
from typing import Any

# ── ANSI color support ──────────────────────────────────────────────────────

_COLORS = {
    "green": "\033[92m",
    "red": "\033[91m",
    "yellow": "\033[93m",
    "cyan": "\033[96m",
    "bold": "\033[1m",
    "dim": "\033[2m",
    "reset": "\033[0m",
}

CLIENT_VERSION = "0.1.0"


def _use_color(args: argparse.Namespace) -> bool:
    if getattr(args, "no_color", False):
        return False
    if not sys.stdout.isatty():
        return False
    return True


def c(s: str, color: str, use: bool) -> str:
    if use:
        return _COLORS.get(color, "") + s + _COLORS["reset"]
    return s


def _echo_ok(msg: str, use: bool) -> None:
    print(c("OK", "green", use) + "  " + msg)


def _echo_err(msg: str, use: bool) -> None:
    print(c("ERR", "red", use) + "  " + msg, file=sys.stderr)


# ── Protocol helpers ────────────────────────────────────────────────────────


class A3sqlError(Exception):
    """Server returned a non-OK status, or a connection/parse error."""

    def __init__(self, code: str, message: str) -> None:
        self.code = code
        self.message = message
        super().__init__(f"[{code}] {message}")


def _recv_line(sock: socket.socket) -> str:
    buf = b""
    while True:
        b = sock.recv(1)
        if not b or b == b"\n":
            break
        buf += b
    return buf.decode()


def send_command(
    host: str,
    port: int,
    command: str,
    user: str | None = None,
    password: str | None = None,
    timeout: int = 10,
) -> list[Any]:
    """Send a raw command to the a3sql TCP listener.

    Returns the parsed JSON response as [status, code, data].
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        sock.connect((host, port))
    except (socket.timeout, ConnectionRefusedError, OSError) as exc:
        raise A3sqlError("CONN", f"Cannot connect to {host}:{port} — {exc}") from exc

    # ── Auth phase ──────────────────────────────────────────────────────
    if user is not None and password is not None:
        sock.sendall(f"LOGIN {user} {password}\n".encode())
        resp = _recv_line(sock)
        try:
            parsed: list[Any] = json.loads(resp)
            if parsed[0] != 0:
                raise A3sqlError(str(parsed[1]), str(parsed[2]))
        except (json.JSONDecodeError, IndexError) as exc:
            raise A3sqlError("AUTH", f"Auth failed: {resp}") from exc

    # ── Command ─────────────────────────────────────────────────────────
    sock.sendall((command + "\n").encode())
    response = sock.recv(65536).decode().strip()
    sock.close()

    if not response:
        raise A3sqlError("EMPTY", "Empty response from server")

    try:
        parsed = json.loads(response)
    except json.JSONDecodeError as exc:
        raise A3sqlError("PARSE", f"Invalid JSON: {response}") from exc

    if not isinstance(parsed, list) or len(parsed) < 3:
        raise A3sqlError("PARSE", f"Unexpected format: {response}")

    return parsed


def send_sql(
    host: str,
    port: int,
    sql: str,
    user: str | None = None,
    password: str | None = None,
) -> list[Any]:
    """Execute an arbitrary SQL statement via live_patch query."""
    return send_command(host, port, f"live_patch query {sql}", user, password)


def send_live_patch(
    host: str,
    port: int,
    cmd: str,
    user: str | None = None,
    password: str | None = None,
) -> list[Any]:
    """Send a live_patch sub-command."""
    return send_command(host, port, f"live_patch {cmd}", user, password)


# ── SQL escaping (ponytail: minimal — only single-quote) ─────────────────


def _sqlesc(s: Any) -> str:
    return str(s).replace("'", "''")


# ── Table helpers ───────────────────────────────────────────────────────────


def _rule_rows(resp: list[Any]) -> list[list[Any]]:
    """Extract rule rows from a select-all response."""
    data = resp[2]
    if not data or not isinstance(data, list) or len(data) < 2:
        return []
    return data[1:]  # skip header row


def _rule_dict(row: list[Any], use: bool) -> str:
    """Format a single patch_rules row for display."""
    rid = row[0]
    active = row[2]
    name = row[1]
    target = row[7]
    prop = row[8]
    op = row[9]
    val = row[10]

    active_str = c("A", "green", use) if active == 1 else c("I", "red", use)
    return (
        f"{c(f'#{rid}', 'bold', use):>5} {active_str}  "
        f"name={c(name, 'cyan', use)}  "
        f"{target}:{prop}  {op}({val})"
    )


def _preset_row(row: list[Any], use: bool) -> str:
    """Format a patch_presets row for display."""
    pid = row[0]
    name = row[1]
    created = row[3] if len(row) > 3 else ""
    return (
        f"{c(f'#{pid}', 'bold', use):>5}  "
        f"name={c(name, 'cyan', use)}  "
        f"created={created}"
    )


# ── Command implementations ────────────────────────────────────────────────


def cmd_list(args: argparse.Namespace) -> int:
    """List all patch rules."""
    try:
        resp = send_live_patch(args.host, args.port, "list", args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    rows = _rule_rows(resp)
    if not rows:
        _echo_ok("No patch rules found", args.use_color)
        return 0

    print(c(f"\n  Patch Rules ({len(rows)}):\n", "bold", args.use_color))
    for row in rows:
        print("  " + _rule_dict(row, args.use_color))
    print()
    return 0


def cmd_get(args: argparse.Namespace) -> int:
    """Get a single rule by ID."""
    sql = f"SELECT * FROM patch_rules WHERE id = {args.id}"
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    rows = _rule_rows(resp)
    if not rows:
        _echo_err(f"Rule #{args.id} not found", args.use_color)
        return 1

    row = rows[0]
    labels = [
        "id",
        "name",
        "active",
        "priority",
        "match_type",
        "match_value",
        "target_type",
        "property",
        "operator",
        "value",
        "created_at",
    ]
    print(c(f"\n  Rule #{args.id}:\n", "bold", args.use_color))
    for i, lbl in enumerate(labels):
        val = row[i] if i < len(row) else ""
        print(f"    {c(lbl, 'cyan', args.use_color)}: {val}")
    print()
    return 0


def cmd_add(args: argparse.Namespace) -> int:
    """Add a new patch rule."""
    name = args.name or f"cli_{int(time.time())}"
    active = 1 if args.active else 0
    priority = args.priority or 0
    operator = args.operator or "set"

    if args.group:
        name = f"{args.group}/{name}"

    sql = (
        "INSERT INTO patch_rules (name, active, priority, target_type, property, operator, value) "
        f"VALUES ('{_sqlesc(name)}', {active}, {priority}, '{_sqlesc(args.target_type)}', "
        f"'{_sqlesc(args.property)}', '{_sqlesc(operator)}', '{_sqlesc(args.value)}')"
    )
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Rule '{name}' added", args.use_color)
    return 0


def cmd_update(args: argparse.Namespace) -> int:
    """Update an existing rule."""
    sets: list[str] = []

    if args.name is not None:
        sets.append(f"name = '{_sqlesc(args.name)}'")
    if args.active is not None:
        sets.append(f"active = {1 if args.active else 0}")
    if args.priority is not None:
        sets.append(f"priority = {args.priority}")
    if args.target_type is not None:
        sets.append(f"target_type = '{_sqlesc(args.target_type)}'")
    if args.property is not None:
        sets.append(f"property = '{_sqlesc(args.property)}'")
    if args.operator is not None:
        sets.append(f"operator = '{_sqlesc(args.operator)}'")
    if args.value is not None:
        sets.append(f"value = '{_sqlesc(args.value)}'")

    if not sets:
        _echo_err("No fields to update", args.use_color)
        return 1

    sql = f"UPDATE patch_rules SET {', '.join(sets)} WHERE id = {args.id}"
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Rule #{args.id} updated", args.use_color)
    return 0


def cmd_delete(args: argparse.Namespace) -> int:
    """Delete a rule."""
    sql = f"DELETE FROM patch_rules WHERE id = {args.id}"
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Rule #{args.id} deleted", args.use_color)
    return 0


def cmd_group(args: argparse.Namespace) -> int:
    """List rules in a group (name prefix convention)."""
    sql = (
        f"SELECT * FROM patch_rules "
        f"WHERE name LIKE '{_sqlesc(args.name)}/%' OR name LIKE '%/{_sqlesc(args.name)}' OR name = '{_sqlesc(args.name)}' "
        f"ORDER BY priority"
    )
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    rows = _rule_rows(resp)
    if not rows:
        _echo_ok(f"No rules in group '{args.name}'", args.use_color)
        return 0

    print(c(f"\n  Group '{args.name}' ({len(rows)} rules):\n", "bold", args.use_color))
    for row in rows:
        print("  " + _rule_dict(row, args.use_color))
    print()
    return 0


def cmd_group_activate(args: argparse.Namespace) -> int:
    """Activate all rules in a group."""
    sql = (
        f"UPDATE patch_rules SET active = 1 "
        f"WHERE (name LIKE '{_sqlesc(args.name)}/%' OR name LIKE '%/{_sqlesc(args.name)}' OR name = '{_sqlesc(args.name)}') "
        f"AND active != 1"
    )
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Group '{args.name}' activated", args.use_color)
    return 0


def cmd_group_deactivate(args: argparse.Namespace) -> int:
    """Deactivate all rules in a group."""
    sql = (
        f"UPDATE patch_rules SET active = 0 "
        f"WHERE (name LIKE '{_sqlesc(args.name)}/%' OR name LIKE '%/{_sqlesc(args.name)}' OR name = '{_sqlesc(args.name)}') "
        f"AND active != 0"
    )
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Group '{args.name}' deactivated", args.use_color)
    return 0


def cmd_presets(args: argparse.Namespace) -> int:
    """List saved presets."""
    try:
        resp = send_sql(
            args.host,
            args.port,
            "SELECT * FROM patch_presets ORDER BY id",
            args.user,
            args.password,
        )
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    data = resp[2]
    if not data or not isinstance(data, list) or len(data) < 2 or not data[1]:
        _echo_ok("No presets found", args.use_color)
        return 0

    rows = data[1:]
    print(c(f"\n  Presets ({len(rows)}):\n", "bold", args.use_color))
    for row in rows:
        print("  " + _preset_row(row, args.use_color))
    print()
    return 0


def cmd_save_preset(args: argparse.Namespace) -> int:
    """Save current rules as a preset (stored as JSON array in data column)."""
    try:
        resp = send_live_patch(args.host, args.port, "list", args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    rows = _rule_rows(resp)
    if not rows:
        _echo_err("No rules to save", args.use_color)
        return 1

    labels = [
        "name",
        "active",
        "priority",
        "match_type",
        "match_value",
        "target_type",
        "property",
        "operator",
        "value",
    ]
    # id at 0, created_at at 11 — strip both
    rules_list: list[dict[str, Any]] = []
    for row in rows:
        d: dict[str, Any] = {}
        for i, lbl in enumerate(labels):
            idx = i + 1  # skip id at 0
            v = row[idx] if idx < len(row) else ""
            d[lbl] = v
        rules_list.append(d)

    preset_data = json.dumps(rules_list)

    # UPSERT: check existence, then insert or update
    name_esc = _sqlesc(args.name)
    try:
        check = send_sql(
            args.host,
            args.port,
            f"SELECT id FROM patch_presets WHERE name = '{name_esc}'",
            args.user,
            args.password,
        )
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    exists = bool(_rule_rows(check))
    data_esc = _sqlesc(preset_data)

    if exists:
        sql = f"UPDATE patch_presets SET data = '{data_esc}' WHERE name = '{name_esc}'"
    else:
        sql = f"INSERT INTO patch_presets (name, data) VALUES ('{name_esc}', '{data_esc}')"

    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    action = "updated" if exists else "saved"
    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Preset '{args.name}' {action} ({len(rules_list)} rules)", args.use_color)
    return 0


def cmd_load_preset(args: argparse.Namespace) -> int:
    """Load rules from a preset. Replaces all existing rules."""
    name_esc = _sqlesc(args.name)
    sql = f"SELECT data FROM patch_presets WHERE name = '{name_esc}'"
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    rows = _rule_rows(resp)
    if not rows:
        _echo_err(f"Preset '{args.name}' not found", args.use_color)
        return 1

    try:
        preset_data: list[dict[str, Any]] = json.loads(str(rows[0][0]))
    except (json.JSONDecodeError, IndexError) as exc:
        _echo_err(f"Invalid preset data: {exc}", args.use_color)
        return 1

    # Replace all rules
    try:
        send_sql(
            args.host, args.port, "DELETE FROM patch_rules", args.user, args.password
        )
    except A3sqlError as exc:
        _echo_err(f"Failed to clear rules: {exc}", args.use_color)
        return 1

    inserted = 0
    for rule in preset_data:
        n = rule.get("name", "")
        a = rule.get("active", 1)
        p = rule.get("priority", 0)
        mt = rule.get("match_type", "exact")
        mv = rule.get("match_value", "")
        tt = rule.get("target_type", "")
        pr = rule.get("property", "")
        op = rule.get("operator", "set")
        v = rule.get("value", "")

        insert_sql = (
            "INSERT INTO patch_rules (name, active, priority, match_type, match_value, "
            "target_type, property, operator, value) VALUES ("
            f"'{_sqlesc(n)}', {a}, {p}, '{_sqlesc(mt)}', '{_sqlesc(mv)}', "
            f"'{_sqlesc(tt)}', '{_sqlesc(pr)}', '{_sqlesc(op)}', '{_sqlesc(v)}')"
        )
        try:
            send_sql(args.host, args.port, insert_sql, args.user, args.password)
            inserted += 1
        except A3sqlError:
            pass

    if args.json:
        print(json.dumps([0, "OK", f"Loaded {inserted} rules"]))
        return 0

    _echo_ok(f"Preset '{args.name}' loaded ({inserted} rules)", args.use_color)
    return 0


def cmd_delete_preset(args: argparse.Namespace) -> int:
    """Delete a preset."""
    sql = f"DELETE FROM patch_presets WHERE name = '{_sqlesc(args.name)}'"
    try:
        resp = send_sql(args.host, args.port, sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    _echo_ok(f"Preset '{args.name}' deleted", args.use_color)
    return 0


def cmd_query(args: argparse.Namespace) -> int:
    """Execute arbitrary SQL."""
    try:
        resp = send_sql(args.host, args.port, args.sql, args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(json.dumps(resp))
        return 0

    status, code, data = resp
    if status == 0:
        _echo_ok(f"[{code}]", args.use_color)
        if data:
            print(json.dumps(data, indent=2))
    else:
        _echo_err(f"[{code}] {data}", args.use_color)
    return 0


def cmd_watch(args: argparse.Namespace) -> int:
    """Stream changes by polling every 2 seconds."""
    print(
        c(
            f"Watching patch_rules on {args.host}:{args.port} (poll 2s)...",
            "bold",
            args.use_color,
        ),
        file=sys.stderr,
    )
    print(c("Ctrl+C to stop.\n", "dim", args.use_color), file=sys.stderr)

    prev: set[int] = set()

    try:
        while True:
            try:
                resp = send_live_patch(
                    args.host, args.port, "list", args.user, args.password
                )
                rows = _rule_rows(resp)
                cur = {r[0] for r in rows}

                new_ids = cur - prev
                if new_ids:
                    for r in rows:
                        if r[0] in new_ids:
                            ts = time.strftime("%H:%M:%S")
                            print(
                                f"[{ts}] {c('ADDED', 'green', args.use_color)}  {_rule_dict(r, args.use_color)}"
                            )

                gone = prev - cur
                if gone:
                    for rid in gone:
                        ts = time.strftime("%H:%M:%S")
                        print(f"[{ts}] {c('REMOVED', 'red', args.use_color)}  #{rid}")

                prev = cur
            except A3sqlError as exc:
                ts = time.strftime("%H:%M:%S")
                print(
                    f"[{ts}] {c('POLL-ERR', 'red', args.use_color)} {exc}",
                    file=sys.stderr,
                )

            time.sleep(2)
    except KeyboardInterrupt:
        print(file=sys.stderr)
        return 0

    return 0  # never reached


def cmd_version(args: argparse.Namespace) -> int:
    """Print client version and server info."""
    try:
        resp = send_command(args.host, args.port, "PING", args.user, args.password)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1

    if args.json:
        print(
            json.dumps({"client": f"a3sql-patch {CLIENT_VERSION}", "server": resp[2]})
        )
        return 0

    print(c(f"a3sql-patch {CLIENT_VERSION}", "bold", args.use_color))
    print(f"  Server: {args.host}:{args.port}  —  {resp[2]}")
    return 0


# ── CLI argument parsing ────────────────────────────────────────────────────


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="a3sql-patch",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        description="Remote CLI client for the a3sql patch system.",
        epilog=textwrap.dedent("""\
            Examples:
              a3sql-patch list
              a3sql-patch add weapon reloadTime 2.5 --name "M4 buff"
              a3sql-patch --host 10.0.0.5 --port 33306 --user admin --password s3cret list
              a3sql-patch query "SELECT COUNT(*) FROM patch_rules"
              a3sql-patch watch
        """),
    )

    # ── Global options ──────────────────────────────────────────────────
    parser.add_argument(
        "--host", default="localhost", help="TCP listener host (default: localhost)"
    )
    parser.add_argument(
        "--port", type=int, default=33306, help="TCP listener port (default: 33306)"
    )
    parser.add_argument("--user", help="Auth username")
    parser.add_argument("--password", help="Auth password")
    parser.add_argument(
        "--json", action="store_true", help="Machine-readable JSON output"
    )
    parser.add_argument("--no-color", action="store_true", help="Disable ANSI colors")
    parser.add_argument("-v", "--verbose", action="store_true", help="Verbose output")

    sub = parser.add_subparsers(dest="command", required=True)

    # list
    sub.add_parser("list", help="List all patch rules")

    # get
    p_get = sub.add_parser("get", help="Get a single rule by ID")
    p_get.add_argument("id", type=int, help="Rule ID")

    # add
    p_add = sub.add_parser("add", help="Add a new patch rule")
    p_add.add_argument(
        "target_type", help="Target scope (weapon, vehicle, man, unit, texture, etc.)"
    )
    p_add.add_argument("property", help="Property/variable name")
    p_add.add_argument("value", help="Value to set")
    p_add.add_argument("--name", help="Rule name (auto-generated if omitted)")
    p_add.add_argument(
        "--active",
        type=int,
        default=1,
        choices=[0, 1],
        help="Active state (default: 1)",
    )
    p_add.add_argument(
        "--priority", type=int, default=0, help="Priority (default: 0, higher = first)"
    )
    p_add.add_argument(
        "--operator",
        default="set",
        help="Operator: set|inc|sub|mul|div|mod|cat|default|toggle|call|sqf_exec|add|remove (default: set)",
    )
    p_add.add_argument("--group", help="Group name (prefixes rule name)")

    # update
    p_upd = sub.add_parser("update", help="Update an existing rule")
    p_upd.add_argument("id", type=int, help="Rule ID")
    p_upd.add_argument("--name", help="New rule name")
    p_upd.add_argument("--active", type=int, choices=[0, 1], help="Active state")
    p_upd.add_argument("--priority", type=int, help="Priority")
    p_upd.add_argument("--target-type", dest="target_type", help="Target scope")
    p_upd.add_argument("--property", help="Property/variable name")
    p_upd.add_argument("--operator", help="Operator")
    p_upd.add_argument("--value", help="Value")
    p_upd.add_argument("--group", help="Group name")

    # delete
    p_del = sub.add_parser("delete", help="Delete a rule")
    p_del.add_argument("id", type=int, help="Rule ID")

    # group
    p_grp = sub.add_parser("group", help="List rules in a group")
    p_grp.add_argument("name", help="Group name")

    # group-activate
    p_ga = sub.add_parser("group-activate", help="Activate all rules in a group")
    p_ga.add_argument("name", help="Group name")

    # group-deactivate
    p_gd = sub.add_parser("group-deactivate", help="Deactivate all rules in a group")
    p_gd.add_argument("name", help="Group name")

    # presets
    sub.add_parser("presets", help="List saved presets")

    # save-preset
    p_sp = sub.add_parser("save-preset", help="Save current rules as a preset")
    p_sp.add_argument("name", help="Preset name")

    # load-preset
    p_lp = sub.add_parser("load-preset", help="Load rules from a preset")
    p_lp.add_argument("name", help="Preset name")

    # delete-preset
    p_dp = sub.add_parser("delete-preset", help="Delete a preset")
    p_dp.add_argument("name", help="Preset name")

    # query
    p_q = sub.add_parser("query", help="Execute arbitrary SQL against patch_rules")
    p_q.add_argument("sql", help="SQL query string")

    # watch
    sub.add_parser("watch", help="Stream changes in real-time (poll every 2 sec)")

    # version
    sub.add_parser("version", help="Print client version and server version")

    return parser


# ── Main ────────────────────────────────────────────────────────────────────


def main() -> int:
    parser = _build_parser()
    args = parser.parse_args()
    args.use_color = _use_color(args)

    # ── Cross-reference the password flag ───────────────────────────────
    # If --password is given without --user (or vice versa), treat login as
    # omitted rather than partial.
    if args.user is None or args.password is None:
        args.user = None
        args.password = None

    dispatch: dict[str, Any] = {
        "list": cmd_list,
        "get": cmd_get,
        "add": cmd_add,
        "update": cmd_update,
        "delete": cmd_delete,
        "group": cmd_group,
        "group-activate": cmd_group_activate,
        "group-deactivate": cmd_group_deactivate,
        "presets": cmd_presets,
        "save-preset": cmd_save_preset,
        "load-preset": cmd_load_preset,
        "delete-preset": cmd_delete_preset,
        "query": cmd_query,
        "watch": cmd_watch,
        "version": cmd_version,
    }

    fn = dispatch.get(args.command)
    if fn is None:
        parser.print_help()
        return 1

    try:
        return fn(args)
    except A3sqlError as exc:
        _echo_err(str(exc), args.use_color)
        return 1
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    sys.exit(main())
