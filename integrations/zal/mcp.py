"""Pull-based stdio MCP bridge for existing coding harnesses. No accept tool."""

import argparse
import os
import json
from pathlib import Path
import sys

from behavior import Refusal, check, explain, help_topic, parse, replay, semantic_diff
from constructs import guide
import explain as explanations
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

# The ZAL loop workspace tools (loop_workspace.py, profile zal/fcis-loop/1), on the workspace named by
# ZAL_LOOP_WORKSPACE. There is no answer or say tool: only the owner's terminal answers and states rules.
LOOP_NOTE = " Tool permission is not the owner's agreement; every verdict comes from the zeno-fcis checkers."
TOOLS += [
    {"name": "zal_loop_read", "description": "Read the loop workspace: subject, incumbent size, open questions for the owner (only the owner answers them) and the latest events in English and symbolic ZAL." + LOOP_NOTE,
     "inputSchema": schema({}, [])},
    {"name": "zal_loop_packet", "description": "The subject card: fields and domains with variant names, outputs, case table, declared laws, incumbent program, and the three header lines every document must start with." + LOOP_NOTE,
     "inputSchema": schema({"surface": {"type": "string", "enum": ["symbolic", "english"]}}, [])},
    {"name": "zal_loop_submit", "description": "Submit a ZAL loop document written by the model (header, subject and 'author model' lines, then statements): programs (holes make a sketch), family programs, questions for the owner. Answers are refused: a model cannot answer for the owner. Returns the judge's replies in both surfaces." + LOOP_NOTE,
     "inputSchema": schema({"source": STRING}, ["source"])},
    {"name": "zal_loop_translate", "description": "Parse a ZAL loop document against the workspace's vocabulary and render it in the other surface, without submitting it." + LOOP_NOTE,
     "inputSchema": schema({"source": STRING, "to": {"type": "string", "enum": ["english", "symbolic"]}}, ["source", "to"])},
    {"name": "zal_loop_legend", "description": "The paired symbolic and controlled-English statement forms of the zal/fcis-loop/1 profile." + LOOP_NOTE,
     "inputSchema": schema({}, [])},
]


# Black-box capture uses separate example-only endpoints. No owner provisioning,
# private status, answer or say tool is exposed. Existing tools stay unchanged.
CAPTURE_NOTE = " Only Rust transform check and receipt replay decide captured-exact. Tool permission is not owner agreement."
TOOLS += [
    {"name": "zal_capture_read", "description": "Read the public capture transcript in both ZAL surfaces." + CAPTURE_NOTE,
     "inputSchema": schema({}, [])},
    {"name": "zal_capture_packet", "description": "Read only public ABI, round, selected examples and proposal headers; no reference source or hidden table." + CAPTURE_NOTE,
     "inputSchema": schema({}, [])},
    {"name": "zal_capture_submit", "description": "Submit one complete model capture proposal. Owner answers, source provisioning and judge-authored claims are refused." + CAPTURE_NOTE,
     "inputSchema": schema({"source": STRING}, ["source"])},
    {"name": "zal_capture_translate", "description": "Translate a model capture proposal between symbolic and controlled English without submitting it." + CAPTURE_NOTE,
     "inputSchema": schema({"source": STRING, "to": {"type": "string", "enum": ["english", "symbolic"]}}, ["source", "to"])},
    {"name": "zal_capture_legend", "description": "Show paired capture acts for zal/fcis-loop/1." + CAPTURE_NOTE,
     "inputSchema": schema({}, [])},
]


# Deterministic explanation tools on the same exact FSM revision, and revision-bound
# ladder statements. Read-only; rendering and lowering keep authority none.
CONTEXT = {"type": "object", "additionalProperties": {"type": "boolean"}}
EXPLAIN_NOTE = " Deterministic typed-model logic, no LLM; read-only and no agreement."
LADDER_NOTE = " Pass the exact revision from zal_ladder_read; a stale revision is refused. Authority: none; no acceptance."
RUNG = {"type": "string", "enum": ["formula", "typed", "formal", "precise", "intuitive", "plain", "question"]}
LADDER_TEXT = {"type": "string", "maxLength": 2000}
TOOLS += [
    {"name": "zal_why_not", "description": "Find the fewest input changes that give a wanted outcome, searching a stated finite scope: context (state and event fixed) or every declared input. Says plainly when no input gives it." + EXPLAIN_NOTE,
     "inputSchema": schema({"revision": STRING, "state": STRING, "event": STRING, "context": CONTEXT,
                            "want": schema({"class": {"type": "string", "enum": ["Accept", "Reject"]}, "state": STRING,
                                            "reason": STRING, "rule": STRING}, []),
                            "scope": {"type": "string", "enum": ["context", "input"]}},
                           ["revision", "state", "event", "context", "want"])},
    {"name": "zal_what_if", "description": "Apply named changes to one exact input and compare the two decisions rule by rule. Every named change is applied and reported; undeclared names are refused." + EXPLAIN_NOTE,
     "inputSchema": schema({"revision": STRING, "state": STRING, "event": STRING, "context": CONTEXT,
                            "change": schema({"state": STRING, "event": STRING, "context": CONTEXT}, [])},
                           ["revision", "state", "event", "context", "change"])},
    {"name": "zal_narrate", "description": "Tell a checker counterexample (current model or pending candidate) or an assumed trace as a numbered controlled-English story paired with symbolic lines. Every sentence is a template filled from the witness and re-derived from the model." + EXPLAIN_NOTE,
     "inputSchema": schema({"revision": STRING, "source": {"type": "string", "enum": ["check", "candidate", "trace"]},
                            "inputs": {"type": "array", "maxItems": 64, "items": schema({"event": STRING, "context": CONTEXT}, ["event", "context"])}},
                           ["revision", "source"])},
    {"name": "zal_guide", "description": "Construct guide (zal/help/2) generated from the construct registry and the parser's statement patterns: meaning, symbolic and English forms, uses in this model. Same topics and typed IDs as zal_help." + EXPLAIN_NOTE,
     "inputSchema": schema({"revision": STRING, "topic": {"type": "string", "maxLength": 128}}, ["revision", "topic"])},
    {"name": "zal_ladder_read", "description": "List the ladder and temporal ladder statements with their exact revisions, profiles and scope identities. Refresh before other ladder calls." + LADDER_NOTE,
     "inputSchema": schema({}, [])},
    {"name": "zal_ladder_render", "description": "Render one statement on all seven rungs (or one), with its scope strip, legend, definitions and evidence status." + LADDER_NOTE,
     "inputSchema": schema({"statement": STRING, "revision": STRING, "rung": RUNG}, ["statement", "revision"])},
    {"name": "zal_ladder_parse", "description": "Parse text at one rung in a statement's context: exactly one expanded normalized tree with its uniqueness counts, or a named refusal." + LADDER_NOTE,
     "inputSchema": schema({"statement": STRING, "revision": STRING, "rung": RUNG, "text": LADDER_TEXT},
                           ["statement", "revision", "rung", "text"])},
    {"name": "zal_ladder_same", "description": "Whether two statements, or a statement and a text at a rung, have the same meaning: identical normal forms and identical scope, with the evidence. Different normal forms are not a proof of different meaning." + LADDER_NOTE,
     "inputSchema": schema({"statement": STRING, "revision": STRING, "other_statement": STRING, "other_revision": STRING,
                            "rung": RUNG, "text": LADDER_TEXT}, ["statement", "revision"])},
    {"name": "zal_ladder_lower", "description": "Lower a temporal statement to its inert .zeno claim for the existing Rust parser, with its obligations. Evaluates and admits nothing." + LADDER_NOTE,
     "inputSchema": schema({"statement": STRING, "revision": STRING}, ["statement", "revision"])},
    {"name": "zal_ladder_explain", "description": "Explain which rung admits a text (or the statement's own formula), why its parse is unique, and what each scope strip field means." + LADDER_NOTE,
     "inputSchema": schema({"statement": STRING, "revision": STRING, "rung": RUNG, "text": LADDER_TEXT}, ["statement", "revision"])},
]


# Ontology requests are inert documents with their own strict bounded validator.
# Keep its opaque JSON and integer vocabulary separate from the FSM schemas.
from zal_notation.ontology_tools import tool_definitions as ontology_definitions

TOOLS += [{"name": tool["name"], "description": tool["description"] + " Authority: none; no behavioral acceptance.",
           "inputSchema": tool["input_schema"]} for tool in ontology_definitions()]

from arithmetic_handoff_tools import tool_definitions as arithmetic_definitions

TOOLS += arithmetic_definitions()


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


def explain_call(session, name, arguments, identity):
    E, a = explanations, arguments
    if name == "zal_why_not":
        return E.why_not(session.model, a["state"], a["event"], a["context"], a["want"], a.get("scope", "context"), identity)
    if name == "zal_what_if":
        return E.what_if(session.model, a["state"], a["event"], a["context"], a["change"], identity)
    if name == "zal_guide":
        return guide(session.model, a["topic"], identity)
    if a["source"] == "trace":
        return E.narrate_trace(session.model, a.get("inputs", []), identity)
    if "inputs" in a:
        raise Refusal("Only a trace narrative takes inputs")
    if a["source"] == "candidate":
        if not session.proposal:
            raise Refusal("No pending candidate to narrate")
        candidate = session.proposal["model"]
        return {**E.narrate_check(candidate, check(candidate, identity), identity), "candidate_revision": candidate.revision}
    return E.narrate_check(session.model, check(session.model, identity), identity)


def valid_request_id(value):
    return type(value) is int or isinstance(value, str) and len(value) <= 256


class Bridge:
    def __init__(self, workspace):
        self.workspace = workspace

    def call(self, name, arguments):
        tool = next((tool for tool in TOOLS if tool["name"] == name), None)
        if not tool:
            raise Refusal("Unknown tool")
        if name.startswith("zal_ontology_"):
            from zal_notation.ontology_tools import dispatch
            return dispatch(name, arguments)
        if name.startswith("zal_arithmetic_"):
            from arithmetic_handoff_tools import dispatch
            return dispatch(name, arguments)
        validate(arguments, tool["inputSchema"])
        if name.startswith("zal_capture_"):
            from capture_workspace import mcp_call as capture_call
            from loop_doors import disable_automatic_knowledge
            with disable_automatic_knowledge():
                return capture_call(name, arguments)
        if name.startswith("zal_ladder_"):
            from ladder_tools import mcp_call as ladder_call
            return ladder_call(name, arguments, checker_identity())
        if name.startswith("zal_loop_"):
            from loop_workspace import mcp_call
            from loop_doors import disable_automatic_knowledge
            with disable_automatic_knowledge():
                return mcp_call(name, arguments)
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
            if name in {"zal_why_not", "zal_what_if", "zal_narrate", "zal_guide"}:
                return explain_call(session, name, arguments, checker_identity())
            if name == "zal_trace":
                trace = replay(session.model, arguments["inputs"])
                return {"revision": revision, "trace": trace,
                        "supplied_assumptions": {"steps": [row["supplied_assumptions"] for row in trace]},
                        "authenticated_facts": []}
            if name == "zal_compare":
                candidate = parse(arguments["surface"], arguments["syntax"])
                return {**semantic_diff(session.model, candidate), "owner_examples": session.example_comparison(candidate)}
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
            except RecursionError:
                # The parsers refuse deep nesting first; this keeps the server alive if anything else recurses.
                return {"content": [{"type": "text", "text": "input nested too deeply"}], "isError": True}
        raise Refusal("Unknown method")


def main():
    # MCP has no ambient private-KB write path. Explicit knowledge queries use
    # the separate read-only tools; ordinary local loop invocations retain env configuration.
    os.environ.pop("ZENO_FCIS_KB", None)
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
        except (ValueError, TypeError, KeyError, RecursionError) as error:
            request_id = request.get("id") if isinstance(request, dict) else None
            message = "input nested too deeply" if isinstance(error, RecursionError) else str(error)[:2000]
            response = {"jsonrpc": "2.0", "id": request_id if valid_request_id(request_id) else None,
                        "error": {"code": -32600, "message": message}}
        print(json.dumps(response, separators=(",", ":")), flush=True)


if __name__ == "__main__":
    main()
