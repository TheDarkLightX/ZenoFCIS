"""Zeno Commons stdio MCP server. Same operations and refusals as the command line.

Start: python3 integrations/commons/mcp.py --store DIR --actor NAME
The actor is fixed per server process, so an agent cannot write as another agent.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from commons import AGENT_STATES, LINK_RELATIONS, RECORD_STATUSES, RECORD_TYPES, Commons, CommonsError  # noqa: E402

TEXT = {"type": "string", "maxLength": 65536}
NAMES = {"type": "array", "items": {"type": "string", "maxLength": 128}, "maxItems": 32}
REFS = {"type": "array", "items": {"type": "string", "maxLength": 4096}, "maxItems": 64}


def schema(properties, required):
    return {"type": "object", "additionalProperties": False, "properties": properties, "required": required}


TOOLS = [
    ("commons_register", "Register your identity once: your server's actor name plus kind and role. Returns your hash ID. Others address you by name or ID.",
     schema({"kind": TEXT, "role": TEXT}, [])),
    ("commons_whoami", "Your identity, or another agent's by name or hash ID.", schema({"who": TEXT}, [])),
    ("commons_advise", "Read before acting: lessons, refutations, gaps and risks relevant to an action, plus leases on any paths named in text and your unread messages.",
     schema({"action": TEXT, "tags": NAMES, "text": TEXT}, ["action"])),
    ("commons_dashboard", "Agents and their states, active leases, record counts and your unread message count.", schema({}, [])),
    ("commons_lease_take", "Take a lease on a resource (a path, branch, port or name) before using it. Refused if another actor holds it or anything inside it. A stale lease (its holder stopped renewing) still blocks: confirm the holder finished, then pass force_reason.",
     schema({"resource": TEXT, "ttl_seconds": {"type": "number"}, "note": TEXT, "force_reason": TEXT}, ["resource"])),
    ("commons_lease_renew", "Renew your lease (heartbeat).", schema({"resource": TEXT, "ttl_seconds": {"type": "number"}}, ["resource"])),
    ("commons_lease_release", "Release your lease when you no longer need the resource.", schema({"resource": TEXT}, ["resource"])),
    ("commons_lease_check", "Unreleased leases, active or stale, that cover a resource or lie inside it. Never change or delete a leased resource; a stale lease means ask its holder, not that it is free.", schema({"resource": TEXT}, ["resource"])),
    ("commons_status", "Post your lifecycle state: working (with step), blocked (with question), idle, or done (only after releasing leases).",
     schema({"state": {"type": "string", "enum": list(AGENT_STATES)}, "step": TEXT, "question": TEXT}, ["state"])),
    ("commons_agents", "Every agent's latest state and read-through message number.", schema({}, [])),
    ("commons_send", "Send a numbered message to an agent by name or hash ID, or '*' for everyone.", schema({"to": TEXT, "body": TEXT}, ["to", "body"])),
    ("commons_inbox", "Your unread messages. Report 'read through N' when you report.", schema({"include_read": {"type": "boolean"}}, [])),
    ("commons_ack", "Mark your messages read through a message number.", schema({"through": {"type": "integer"}}, ["through"])),
    ("commons_record_add", "Write when you learn: add a hypothesis, claim, evidence, refutation, counterexample, result, gap, lesson, decision, question, answer, risk or note. Cite evidence in refs.",
     schema({"type": {"type": "string", "enum": list(RECORD_TYPES)}, "title": TEXT, "body": TEXT,
             "status": {"type": "string", "enum": list(RECORD_STATUSES)}, "tags": NAMES, "refs": REFS}, ["type", "title"])),
    ("commons_record_update", "Change a record's status (for example supported or refuted) with a note and evidence "
     "references, or correct its title (the old title is kept in a dated note).",
     schema({"id": TEXT, "status": {"type": "string", "enum": list(RECORD_STATUSES)}, "note": TEXT, "refs": REFS,
             "title": TEXT}, ["id"])),
    ("commons_link", "Relate two records: " + ", ".join(LINK_RELATIONS) + ".",
     schema({"src": TEXT, "rel": {"type": "string", "enum": list(LINK_RELATIONS)}, "dst": TEXT}, ["src", "rel", "dst"])),
    ("commons_record", "One record with its links.", schema({"id": TEXT}, ["id"])),
    ("commons_records", "Search records by type, status or tag, newest first.",
     schema({"type": TEXT, "status": TEXT, "tag": TEXT, "limit": {"type": "integer"}}, [])),
    ("commons_emit", "Report a factory event (part, event, subject, body).",
     schema({"part": TEXT, "event": TEXT, "subject": TEXT, "body": {"type": "object"}}, ["part", "event", "subject"])),
]


class Bridge:
    def __init__(self, store: Commons):
        self.store = store

    def call(self, name: str, a: dict):
        s = self.store
        table = {
            "commons_register": lambda: s.register("", a.get("kind", "agent"), a.get("role", "")),
            "commons_whoami": lambda: s.identity(a.get("who", "")),
            "commons_advise": lambda: s.advise(a["action"], a.get("tags"), a.get("text", "")),
            "commons_dashboard": s.dashboard,
            "commons_lease_take": lambda: s.lease_take(a["resource"], a.get("ttl_seconds", 3600), a.get("note", ""),
                                                       a.get("force_reason", "")),
            "commons_lease_renew": lambda: s.lease_renew(a["resource"], a.get("ttl_seconds", 3600)),
            "commons_lease_release": lambda: s.lease_release(a["resource"]),
            "commons_lease_check": lambda: s.lease_check(a["resource"]),
            "commons_status": lambda: s.status(a["state"], a.get("step", ""), a.get("question", "")),
            "commons_agents": s.agents,
            "commons_send": lambda: s.send(a["to"], a["body"]),
            "commons_inbox": lambda: s.inbox(a.get("include_read", False)),
            "commons_ack": lambda: s.ack(a["through"]),
            "commons_record_add": lambda: s.record_add(a["type"], a["title"], a.get("body", ""), a.get("status", "open"), a.get("tags"), a.get("refs")),
            "commons_record_update": lambda: s.record_update(a["id"], a.get("status", ""), a.get("note", ""), a.get("refs"),
                                                             a.get("title", "")),
            "commons_link": lambda: s.link(a["src"], a["rel"], a["dst"]),
            "commons_record": lambda: s.record(a["id"]),
            "commons_records": lambda: s.records(a.get("type", ""), a.get("status", ""), a.get("tag", ""), a.get("limit", 50)),
            "commons_emit": lambda: {"seq": s.emit(a["part"], a["event"], a["subject"], a.get("body"))},
        }
        if name not in table:
            raise CommonsError(f"unknown tool {name}")
        return table[name]()

    def handle(self, request):
        if not isinstance(request, dict) or request.get("jsonrpc") != "2.0" or not isinstance(request.get("method"), str):
            raise CommonsError("expected a JSON-RPC 2.0 request")
        if "id" not in request:
            return None
        method, params = request["method"], request.get("params", {}) or {}
        if method == "initialize":
            return {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                    "serverInfo": {"name": "zenofcis-commons", "version": "0.1.0"},
                    "instructions": "Shared state of the software factory. Register once (commons_register). Read before acting (commons_advise), write when "
                                    "you learn (commons_record_add), acknowledge messages and lease what you use. "
                                    "Records cite evidence; they do not prove anything by themselves."}
        if method == "ping":
            return {}
        if method == "tools/list":
            return {"tools": [{"name": n, "description": d, "inputSchema": sch} for n, d, sch in TOOLS]}
        if method == "tools/call":
            try:
                args = params.get("arguments", {}) or {}
                if not isinstance(args, dict):
                    raise CommonsError("arguments must be an object")
                result = self.call(params.get("name", ""), args)
                return {"content": [{"type": "text", "text": json.dumps(result, indent=2, ensure_ascii=False)}], "isError": False}
            except (CommonsError, KeyError, TypeError, ValueError) as error:
                return {"content": [{"type": "text", "text": str(error)[:2000]}], "isError": True}
        raise CommonsError("unknown method")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--store", required=True)
    parser.add_argument("--actor", required=True)
    options = parser.parse_args()
    bridge = Bridge(Commons(options.store, options.actor))
    while True:
        data = sys.stdin.buffer.readline(512 * 1024 + 1)
        if not data:
            return 0
        if len(data) > 512 * 1024 or not data.endswith(b"\n"):
            print("MCP input exceeds 512 KiB or is incomplete", file=sys.stderr)
            return 1
        request = None
        try:
            request = json.loads(data)
            result = bridge.handle(request)
            if result is None:
                continue
            response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        except (ValueError, TypeError, KeyError, RecursionError) as error:
            rid = request.get("id") if isinstance(request, dict) else None
            response = {"jsonrpc": "2.0", "id": rid if isinstance(rid, (str, int)) else None,
                        "error": {"code": -32600, "message": str(error)[:2000]}}
        print(json.dumps(response, separators=(",", ":"), ensure_ascii=False), flush=True)


if __name__ == "__main__":
    sys.exit(main())
