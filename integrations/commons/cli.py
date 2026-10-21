#!/usr/bin/env python3
"""Zeno Commons command line. The store comes from --store or ZENO_FCIS_COMMONS; the actor from --actor or
ZENO_FCIS_COMMONS_ACTOR. Output is JSON. Exit codes: 0 ok, 2 refused, 3 resource is leased (lease check)."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from commons import Commons, CommonsError  # noqa: E402


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="commons", description=__doc__)
    parser.add_argument("--store", default=os.environ.get("ZENO_FCIS_COMMONS"))
    parser.add_argument("--actor", default=os.environ.get("ZENO_FCIS_COMMONS_ACTOR", ""))
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("init", help="create the store if needed and print its dashboard")
    p = sub.add_parser("register", help="register an identity: a unique name and a hash ID (defaults to your actor name)")
    p.add_argument("--name", default=""); p.add_argument("--kind", default="agent"); p.add_argument("--role", default="")
    p = sub.add_parser("whoami", help="your registered identity, or another agent's by name or ID")
    p.add_argument("who", nargs="?", default="")
    sub.add_parser("dashboard", help="agents, leases, record counts and your unread messages")
    sub.add_parser("verify", help="recompute the event hash chain")

    p = sub.add_parser("advise", help="read before acting: negative memory, leases and unread messages")
    p.add_argument("action")
    p.add_argument("--tag", action="append", default=[])
    p.add_argument("--text", default="")
    p.add_argument("--limit", type=int, default=10)

    lease = sub.add_parser("lease", help="take, renew, release, check or list leases").add_subparsers(dest="op", required=True)
    p = lease.add_parser("take"); p.add_argument("resource"); p.add_argument("--ttl", type=float, default=3600); p.add_argument("--note", default="")
    p = lease.add_parser("renew"); p.add_argument("resource"); p.add_argument("--ttl", type=float, default=3600)
    p = lease.add_parser("release"); p.add_argument("resource"); p.add_argument("--force-reason", default="")
    p = lease.add_parser("check", help="exit 3 if the resource, or anything inside it, is leased"); p.add_argument("resource")
    lease.add_parser("list")

    p = sub.add_parser("status", help="post your lifecycle state")
    p.add_argument("state", choices=["working", "blocked", "idle", "done"])
    p.add_argument("--step", default="")
    p.add_argument("--question", default="")
    sub.add_parser("agents", help="every agent's latest state and read-through number")

    p = sub.add_parser("send", help="send a numbered message to an agent by name or hash ID, or * for everyone")
    p.add_argument("to"); p.add_argument("body")
    p = sub.add_parser("inbox", help="your unread messages")
    p.add_argument("--all", action="store_true")
    p = sub.add_parser("ack", help="mark messages read through a number")
    p.add_argument("through", type=int)

    p = sub.add_parser("add", help="add a record: hypothesis, claim, evidence, refutation, gap, lesson, decision...")
    p.add_argument("type"); p.add_argument("title")
    p.add_argument("--body", default=""); p.add_argument("--status", default="open")
    p.add_argument("--tag", action="append", default=[]); p.add_argument("--ref", action="append", default=[])
    p.add_argument("--id", default="")
    p = sub.add_parser("update", help="change a record's status, add a note or references")
    p.add_argument("id"); p.add_argument("--status", default=""); p.add_argument("--note", default="")
    p.add_argument("--ref", action="append", default=[])
    p = sub.add_parser("link", help="relate two records")
    p.add_argument("src"); p.add_argument("rel"); p.add_argument("dst")
    p = sub.add_parser("show"); p.add_argument("id")
    p = sub.add_parser("records")
    p.add_argument("--type", default=""); p.add_argument("--status", default=""); p.add_argument("--tag", default="")
    p.add_argument("--limit", type=int, default=50)

    p = sub.add_parser("emit", help="report a factory event")
    p.add_argument("part"); p.add_argument("event"); p.add_argument("subject"); p.add_argument("--body", default="{}")
    p = sub.add_parser("events"); p.add_argument("--after", type=int, default=0); p.add_argument("--kind", default="")
    p.add_argument("--limit", type=int, default=100)
    p = sub.add_parser("import-gaps"); p.add_argument("path")
    return parser


def run(args) -> tuple[int, object]:
    if not args.store:
        raise CommonsError("give --store or set ZENO_FCIS_COMMONS")
    if not args.actor:
        raise CommonsError("give --actor or set ZENO_FCIS_COMMONS_ACTOR")
    c = Commons(args.store, args.actor)
    try:
        cmd = args.command
        if cmd in ("init", "dashboard"):
            return 0, c.dashboard()
        if cmd == "register":
            return 0, c.register(args.name, args.kind, args.role)
        if cmd == "whoami":
            return 0, c.identity(args.who)
        if cmd == "verify":
            out = c.verify()
            return (0 if out["ok"] else 2), out
        if cmd == "advise":
            return 0, c.advise(args.action, args.tag, args.text, args.limit)
        if cmd == "lease":
            if args.op == "take":
                return 0, c.lease_take(args.resource, args.ttl, args.note)
            if args.op == "renew":
                return 0, c.lease_renew(args.resource, args.ttl)
            if args.op == "release":
                return 0, c.lease_release(args.resource, args.force_reason)
            if args.op == "check":
                held = c.lease_check(args.resource)
                return (3 if held else 0), {"resource": args.resource, "leased": held}
            return 0, c.leases()
        if cmd == "status":
            return 0, c.status(args.state, args.step, args.question)
        if cmd == "agents":
            return 0, c.agents()
        if cmd == "send":
            return 0, c.send(args.to, args.body)
        if cmd == "inbox":
            return 0, c.inbox(args.all)
        if cmd == "ack":
            return 0, c.ack(args.through)
        if cmd == "add":
            return 0, c.record_add(args.type, args.title, args.body, args.status, args.tag, args.ref, args.id)
        if cmd == "update":
            return 0, c.record_update(args.id, args.status, args.note, args.ref)
        if cmd == "link":
            return 0, c.link(args.src, args.rel, args.dst)
        if cmd == "show":
            return 0, c.record(args.id)
        if cmd == "records":
            return 0, c.records(args.type, args.status, args.tag, args.limit)
        if cmd == "emit":
            return 0, {"seq": c.emit(args.part, args.event, args.subject, json.loads(args.body))}
        if cmd == "events":
            return 0, c.events(args.after, args.kind, args.limit)
        if cmd == "import-gaps":
            return 0, c.import_gaps(args.path)
        raise CommonsError(f"unknown command {cmd}")
    finally:
        c.close()


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        code, out = run(args)
    except (CommonsError, json.JSONDecodeError, OSError) as error:
        print(json.dumps({"refused": str(error)[:2000]}), file=sys.stderr)
        return 2
    print(json.dumps(out, indent=2, ensure_ascii=False))
    return code


if __name__ == "__main__":
    sys.exit(main())
