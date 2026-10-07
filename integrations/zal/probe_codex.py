"""Explicit installed-process/live-model probe; never runs in ordinary tests."""

import argparse
import json
from pathlib import Path
import subprocess
import threading

from behavior import parse
from codex_transport import AppServer
from workflow import PROPOSAL_SCHEMA, Session


def probe(live=False):
    done = threading.Event()
    messages = []
    report = {"schema": "zal/codex-probe/1", "installed_process": "not-run", "live_model": "not-run",
              "domain_agreement": "none", "tool_approval_policy": "requests-refused", "protocol": "stdio JSONL",
              "version": subprocess.check_output(["codex", "--version"], text=True).strip()}
    def event(value):
        messages.append(value)
        if value.get("method") in {"turn/completed", "zal/transportError"}:
            done.set()
    client = AppServer(event)
    try:
        client.connect()
        available = client.request("model/list", {}).get("data", [])
        model = next((m["model"] for m in available if m.get("isDefault") and not m.get("hidden")), None)
        if not model:
            raise ValueError("No available default model")
        client.start_thread(str(Path.cwd()), model)
        report["thread_model"] = model
        report["installed_process"] = "handshake-and-read-only-thread-passed"
        if live:
            session = Session(parse((Path(__file__).parent / "examples/order.zal").read_text()))
            text = session.context("Explain the selected rule in at most 35 words. Return act explain with surface null. No tools or proposal.")
            turn_id = client.start_turn(text, PROPOSAL_SCHEMA)
            if not done.wait(60):
                client.interrupt()
                report["live_model"] = "timeout-interrupted"
            else:
                completed = [m for m in messages if m.get("method") == "turn/completed"
                             and m.get("params", {}).get("threadId") == client.thread_id
                             and m.get("params", {}).get("turn", {}).get("id") == turn_id]
                if completed and completed[-1]["params"]["turn"]["status"] == "completed":
                    outputs = [m["params"]["item"]["text"] for m in messages
                               if m.get("method") == "item/completed"
                               and m.get("params", {}).get("threadId") == client.thread_id
                               and m.get("params", {}).get("turnId") == turn_id
                               and m.get("params", {}).get("item", {}).get("type") == "agentMessage"]
                    if not outputs:
                        raise ValueError("Completed turn has no final message")
                    value = json.loads(outputs[-1])
                    if not isinstance(value, dict) or value.get("act") != "explain" or value.get("surface") is not None:
                        raise ValueError("The explanation probe received another act; no explanation pass")
                    session.agent_act(value)
                    if session.proposal is not None:
                        raise ValueError("The explanation probe staged a candidate; no explanation pass")
                    report["live_model"] = "completed-structured-explanation-passed"
                    report["response"] = value
                    report["revision_unchanged"] = session.model.revision == value["base_revision"]
                else:
                    report["live_model"] = "failed-or-disconnected"
                    report["error"] = "The installed process did not complete the requested turn"
                    if completed:
                        report["turn_error"] = completed[-1]["params"]["turn"].get("error")
    except (ValueError, OSError, KeyError) as error:
        report["error"] = str(error)[:2000]
        report["live_model"] = "unavailable" if live else "not-run"
    finally:
        client.close()
    items = {m.get("params", {}).get("item", {}).get("type") for m in messages if m.get("method") in {"item/started", "item/completed"}}
    tool_items = sorted(items - {None, "userMessage", "agentMessage", "reasoning", "plan"})
    report["tool_items_observed"] = tool_items
    report["tool_execution"] = "none-observed" if not tool_items else "inspect-observed-items"
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true", help="Use the existing account for one bounded explanation turn")
    parser.add_argument("--out", type=Path)
    options = parser.parse_args()
    result = probe(options.live)
    text = json.dumps(result, indent=2) + "\n"
    if options.out:
        options.out.write_text(text)
    print(text)
