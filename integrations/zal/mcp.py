"""Pull-based stdio MCP bridge for existing coding harnesses. No accept tool."""

import argparse
import json
from pathlib import Path
import sys

from behavior import Refusal, check, explain, help_topic, parse, replay, semantic_diff
from shared import SharedWorkspace
from workflow import checker_identity


def schema(properties, required):
    return {"type": "object", "additionalProperties": False,
            "properties": properties, "required": required}


STRING = {"type": "string", "maxLength": 65536}
TOOLS = [
    {"name": "zal_read", "description": "Read the current exact revision, typed IDs, paired meaning, candidate and evidence. Refresh before proposing.", "inputSchema": schema({}, [])},
    {"name": "zal_check", "description": "Run scoped finite checks on the current exact revision. No authority or agreement.", "inputSchema": schema({"revision": STRING}, ["revision"])},
    {"name": "zal_explain", "description": "Explain one exact decision and every guard from the typed logic, without an LLM or guessed context. Read-only.",
     "inputSchema": schema({"revision": STRING, "state": STRING, "event": STRING,
                            "context": {"type": "object", "additionalProperties": {"type": "boolean"}}},
                           ["revision", "state", "event", "context"])},
    {"name": "zal_help", "description": "Get exact built-in grammar or typed-object help for the current revision. No LLM and no state change.",
     "inputSchema": schema({"revision": STRING, "topic": {"type": "string", "maxLength": 128}}, ["revision", "topic"])},
    {"name": "zal_propose", "description": "Propose a complete meaning for an exact base and declared object IDs. Human terminal review is required. Never accepts.",
     "inputSchema": schema({"base_revision": STRING, "object_ids": {"type": "array", "items": STRING, "maxItems": 32}, "message": STRING,
                            "syntax": {"type": "string", "enum": ["symbolic", "english"]}, "surface": STRING},
                           ["base_revision", "object_ids", "message", "syntax", "surface"])},
    {"name": "zal_compare", "description": "Compare a proposed surface against the current exact revision, including concrete witnesses and excluded observations.",
     "inputSchema": schema({"revision": STRING, "syntax": {"type": "string", "enum": ["symbolic", "english"]}, "surface": STRING}, ["revision", "syntax", "surface"])},
    {"name": "zal_trace", "description": "Replay up to64 assumed inputs from the declared initial state on the exact current revision.",
     "inputSchema": schema({"revision": STRING, "inputs": {"type": "array", "maxItems": 64, "items": schema({"event": STRING, "context": {"type": "object", "additionalProperties": {"type": "boolean"}}}, ["event", "context"])}}, ["revision", "inputs"])},
]


def validate(value, specification, path="arguments"):
    """Enforce the small published tool-schema vocabulary before any action."""
    kind = specification["type"]
    types = {"object": dict, "array": list, "string": str, "boolean": bool}
    if type(value) is not types[kind]:
        raise Refusal(f"{path} must be {kind}")
    if "enum" in specification and value not in specification["enum"]:
        raise Refusal(f"{path} has an unsupported value")
    if kind == "string" and len(value) > specification.get("maxLength", 65536):
        raise Refusal(f"{path} exceeds its string bound")
    if kind == "array":
        if len(value) > specification.get("maxItems", 32):
            raise Refusal(f"{path} exceeds its item bound")
        for item in value:
            validate(item, specification["items"], path + "[]")
    if kind == "object":
        properties = specification.get("properties", {})
        if not set(specification.get("required", [])) <= set(value):
            raise Refusal(f"{path} is missing required fields")
        for key, item in value.items():
            if not isinstance(key, str):
                raise Refusal(f"{path} needs string keys")
            field = properties.get(key, specification.get("additionalProperties", False))
            if field is False:
                raise Refusal(f"{path} has unknown fields")
            validate(item, field, path + "." + key)


def valid_request_id(value):
    return type(value) is int or isinstance(value, str) and len(value) <= 256


class Bridge:
    def __init__(self, workspace):
        self.workspace = workspace

    def call(self, name, arguments):
        tool = next((tool for tool in TOOLS if tool["name"] == name), None)
        if not tool:
            raise Refusal("Unknown tool")
        validate(arguments, tool["inputSchema"])
        def action(session):
            if name == "zal_read":
                return {**session.view(), "object_ids": sorted(session.objects()), "authority": "none",
                        "workflow": "Ask the person to review in the ZAL terminal. Never accept via shell/tool approval. Refresh this workspace after their acceptance."}
            revision = arguments.get("revision", arguments.get("base_revision"))
            if revision != session.model.revision:
                raise Refusal("Stale revision; call zal_read again")
            if name == "zal_check":
                return check(session.model, checker_identity())
            if name == "zal_explain":
                return explain(session.model, arguments["state"], arguments["event"], arguments["context"], checker_identity())
            if name == "zal_help":
                return help_topic(session.model, arguments["topic"], checker_identity())
            if name == "zal_trace":
                return {"revision": revision, "trace": replay(session.model, arguments["inputs"])}
            if name == "zal_compare":
                return semantic_diff(session.model, parse(arguments["surface"], arguments["syntax"]))
            session.agent_act({"act": "propose", **arguments})
            return session.view()
        return self.workspace.use(action)

    def handle(self, request):
        if not isinstance(request, dict) or request.get("jsonrpc") != "2.0" or not isinstance(request.get("method"), str):
            raise Refusal("Expected a JSON-RPC2.0 request")
        if "id" not in request:
            # JSON-RPC notifications cannot call tools or change workspace state.
            return None
        if not valid_request_id(request["id"]):
            raise Refusal("Request id must be a nonnull string or integer")
        method, params = request["method"], request.get("params", {})
        if not isinstance(params, dict):
            raise Refusal("Request params must be an object")
        if method == "initialize":
            return {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                    "serverInfo": {"name": "zenofcis-zal", "version": "0.1.0"},
                    "instructions": "Behavior authoring only. Refresh exact revision before proposals. Human acceptance is absent from tools and command approval grants no agreement."}
        if method == "ping":
            return {}
        if method == "tools/list":
            return {"tools": TOOLS}
        if method == "tools/call":
            try:
                if set(params) - {"name", "arguments", "_meta"} or not isinstance(params.get("name"), str):
                    raise Refusal("Tool call needs a name and valid argument fields")
                result = self.call(params["name"], params.get("arguments", {}))
                return {"content": [{"type": "text", "text": json.dumps(result, indent=2)}], "isError": False}
            except (ValueError, TypeError, KeyError, OSError) as error:
                return {"content": [{"type": "text", "text": str(error)[:2000]}], "isError": True}
        raise Refusal("Unknown method")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, required=True)
    options = parser.parse_args()
    bridge = Bridge(SharedWorkspace(options.workspace))
    while True:
        data = sys.stdin.buffer.readline(512 * 1024 + 1)
        if not data:
            return
        if len(data) > 512 * 1024 or not data.endswith(b"\n"):
            print("MCP input exceeds512KiB or is incomplete", file=sys.stderr)
            return
        request = None
        try:
            request = json.loads(data)
            result = bridge.handle(request)
            if result is None:
                continue
            response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        except (ValueError, TypeError, KeyError) as error:
            request_id = request.get("id") if isinstance(request, dict) else None
            response = {"jsonrpc": "2.0", "id": request_id if valid_request_id(request_id) else None,
                        "error": {"code": -32600, "message": str(error)[:2000]}}
        print(json.dumps(response, separators=(",", ":")), flush=True)


if __name__ == "__main__":
    main()
