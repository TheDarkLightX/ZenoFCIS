"""End-to-end check of the ZAL loop workspace with the real zeno-fcis CLI.

It builds nothing: give the CLI (``--cli`` or ``ZENO_FCIS_CLI``, else the debug build in ``CARGO_TARGET_DIR``).
In a scratch directory it:

* opens a workspace on one bounded-counter instance and submits, as the model, a wrong program, the
  same wrong program written another way (answered from memory by meaning), a sketch that completes, a
  sketch with no completion and a question for the owner;
* answers the question through the owner's terminal (``terminal.py loop answer``) and states a rule
  (``terminal.py loop say``);
* drives the MCP server over stdio: the tool list has no answer or say tool, a model document that answers
  is refused, and the existing tools come first unchanged;
* opens a family workspace and submits a family program with a hole, whose fit is checked on the instances
  it was not fitted on;
* routes acts to the contract judges on a second counter workspace, without the catalogue's complete relation
  so that owner questions remain: an invariant feedback cycle (a too-weak
  invariant refuted with its counterexample to induction and the core's decision there, a revised one
  inductive, the same invariant in other words answered from memory), a law that holds and one with a false
  alarm, a scenario that kills the reject-everything core, a teach-back rule with its exceptions, a rule the
  requirements do not yet force becoming an owner question, the freedom question answered by the owner, an
  answer that conflicts with the confirmed requirements refused with nothing changed, and a decomposition
  completed by projection and one explained by an interface counterexample, a negated number routed with
  its loop meaning, and two rules that agree only inside their region judged apart;
* changes a contract after its workspace is initialized: the workspace's judges keep reading its own bound
  copy (an invariant inductive under the old policy stays so, from memory), a new workspace on the changed
  contract gets the fresh refutation, and a changed copy is refused;
* re-parses every transcript event in both surfaces and compares it with the recorded act.

Every verdict comes from the CLI; this script only reads the transcript. Exit 0 when every expectation
holds. ``--out DIR`` keeps the transcripts.

This script is LLM-generated.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import loop_doors  # noqa: E402
import loop_language as Z  # noqa: E402
import loop_workspace as LW  # noqa: E402

ROOT = HERE.parents[1]
WRONG = "authorized ? action - (value == (action == Inc ? 7 : 0) ? 149 : 147) : 0"
WRONG_RESPELLED = "!authorized ? 0 : action - ((action == Dec ? 0 : 7) == value ? 149 : 147)"
SKETCH = "authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0"


class Failed(Exception):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise Failed(message)


def reparse(state: dict) -> int:
    ctx = Z.Context(state["subject"])
    count = 0
    for event in state["events"]:
        for english in (False, True):
            parsed = Z.parse_statement(event["english" if english else "symbolic"], ctx, english)
            expect(parsed.data() == event["act"], f"event {event['seq']} does not re-parse: {event}")
            count += 1
    return count


def terminal(arguments: list, env: dict) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(HERE / "terminal.py"), "loop", *arguments], capture_output=True,
                          text=True, timeout=300, env=env)


def core_workspace(work: Path, cli: Path, env: dict, report: dict) -> dict:
    sys.path.insert(0, str(ROOT / "tools"))
    import instantiate_core
    sys.path.pop(0)
    contract = work / "counter" / "contract"
    contract.parent.mkdir()
    instantiate_core.instantiate("bounded-counter", {"C": 8}, contract, include_proof=False)
    path = work / "counter.json"
    done = terminal(["init", "--contract", str(contract), "--workspace", str(path), "--cli", str(cli)], env)
    expect(done.returncode == 0, f"loop init failed: {done.stderr}")
    workspace = LW.LoopWorkspace(path, cli)
    head = Z.HEADER["symbolic"] + "\n" + Z.subject_line(Z.Context(workspace.load()["subject"]).subject_ref, False)
    events = workspace.submit(head + f"\nauthor model;\nprogram p1 out0 = {WRONG};\n", "model")
    kinds = [e["act"]["kind"] for e in events]
    expect(kinds[:5] == ["program", "refuted", "region", "fact", "site"], f"refuted program replies: {kinds}")
    expect(any("checked repair" in e["english"] for e in events), "the wrong program has a checked repair")
    # the same wrong program, written another way, is answered from memory by meaning
    attempts = len(workspace.load()["events"])
    events = workspace.submit(head + f"\nauthor model;\nprogram p1b out0 = {WRONG_RESPELLED};\n", "model")
    kinds = [e["act"]["kind"] for e in events]
    expect(kinds == ["program", "same"] and events[1]["symbolic"].startswith("same p1b as p1: 36 inputs"),
           f"a respelled refuted program is answered from memory: {[e['symbolic'] for e in events]}")
    expect(len(workspace.load()["events"]) == attempts + 2, "the respelling adds only its program and the reply")
    events = workspace.submit(head + f"\nauthor model;\nprogram p2 out0 = {SKETCH};\n"
                              "program p3 out0 = authorized ? action - 149 : ?c;\n"
                              "ask k1 at value = 3, action = Inc, authorized = false: reject not_authorized | "
                              "reject at_zero;\n", "model")
    kinds = [e["act"]["kind"] for e in events]
    expect("completed" in kinds and "accepted" in kinds, f"the sketch completes: {kinds}")
    expect("unrealizable" in kinds, f"the second sketch is unrealizable with a witness: {kinds}")
    expect(workspace.load()["incumbent"]["nodes"] == 17, "the completed sketch is the new incumbent")
    done = terminal(["answer", "k1", "1", "--workspace", str(path)], env)
    expect(done.returncode == 0 and "answer recorded" in done.stdout, f"terminal answer: {done.stdout}{done.stderr}")
    done = terminal(["say", "rule t1: where value == 0 -> reject at_zero;", "--workspace", str(path)], env)
    expect(done.returncode == 0 and "exception to t1" in done.stdout.lower(), f"terminal say: {done.stdout}")
    report["core_events"] = len(workspace.load()["events"])
    return {"path": path, "head": head}


def mcp_session(path: Path, head: str, env: dict, report: dict) -> None:
    requests = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}},
                {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "zal_loop_submit", "arguments": {
                    "source": head + "\nauthor model;\nanswer k1: any;\n"}}},
                {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "zal_loop_read", "arguments": {}}},
                {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "zal_loop_answer",
                                                                                "arguments": {}}}]
    shared = path.with_name("finite-fsm.json")
    done = subprocess.run([sys.executable, str(HERE / "terminal.py"), "init", "--workspace", str(shared), "--json"],
                          capture_output=True, text=True, timeout=120, env=env)
    expect(done.returncode == 0, f"finite-fsm workspace: {done.stderr}")
    before = path.read_bytes()
    done = subprocess.run([sys.executable, str(HERE / "mcp.py"), "--workspace", str(shared)],
                          input="\n".join(json.dumps(r) for r in requests) + "\n", capture_output=True, text=True,
                          timeout=300, env={**env, "ZAL_LOOP_WORKSPACE": str(path)})
    replies = {r["id"]: r for r in map(json.loads, done.stdout.splitlines())}
    names = [t["name"] for t in replies[2]["result"]["tools"]]
    expect(names[:7] == ["zal_read", "zal_check", "zal_explain", "zal_help", "zal_propose", "zal_compare",
                         "zal_trace"], f"existing tools first and unchanged: {names}")
    expect(names[17:] == ["zal_why_not", "zal_what_if", "zal_narrate", "zal_guide", "zal_ladder_read",
                          "zal_ladder_render", "zal_ladder_parse", "zal_ladder_same", "zal_ladder_lower",
                          "zal_ladder_explain"], f"explanation and ladder tools follow the capture tools: {names}")
    expect(not any(w in n for n in names for w in ("answer", "say", "accept")), f"no answer or say tool: {names}")
    expect(replies[3]["result"]["isError"] and "cannot answer" in replies[3]["result"]["content"][0]["text"],
           "a model document that answers is refused")
    expect(not replies[4]["result"]["isError"], "zal_loop_read works")
    expect(replies[5]["result"]["isError"], "there is no answer tool")
    expect(path.read_bytes() == before, "refused MCP calls change nothing")
    report["mcp_tools"] = names


def historical_incomplete(contract: Path) -> None:
    """Drop the catalogue's independent complete relation (laws 520 and above), as the CLI's judge tests do.

    The complete relation determines every decision, so no owner question would remain to ask."""
    project = contract / "project.zeno"
    source = project.read_text(encoding="ascii")
    expect(source.count("// Independent complete relation") == 1, "the instance names its complete relation once")
    project.write_text(source.split("// Independent complete relation")[0], encoding="ascii")
    policy_path = contract / "v2" / "policy.json"
    policy = json.loads(policy_path.read_text(encoding="ascii"))
    kept = {law: kind for law, kind in policy["law_kinds"].items() if int(law) < 520}
    expect(len(kept) < len(policy["law_kinds"]), "the instance declares complete-relation law kinds")
    policy["law_kinds"] = kept
    policy_path.write_text(json.dumps(policy, indent=2) + "\n", encoding="ascii")


def judges_workspace(work: Path, cli: Path, report: dict) -> Path:
    """Acts routed to the contract judges, on a bounded counter at C = 8 without its complete relation."""
    sys.path.insert(0, str(ROOT / "tools"))
    import instantiate_core
    sys.path.pop(0)
    contract = work / "judged" / "contract"
    contract.parent.mkdir()
    instantiate_core.instantiate("bounded-counter", {"C": 8}, contract, include_proof=False)
    historical_incomplete(contract)
    path = work / "judged.json"
    workspace = LW.LoopWorkspace(path, cli)
    workspace.init(contract=contract)
    ctx = Z.Context(workspace.load()["subject"])
    head = Z.HEADER["symbolic"] + "\n" + Z.subject_line(ctx.subject_ref, False) + "\nauthor model;\n"

    def submit(body: str, author: str = "model") -> list:
        return workspace.submit(head.replace("author model;", f"author {author};") + body, author)

    def judged(events: list) -> list:
        return [e["symbolic"] for e in events if e["author"] == "judge"]

    # the invariant feedback cycle
    weak = judged(submit("invariant i1: value <= 7;\n"))
    expect(len(weak) == 1 and weak[0].startswith("fact i1: false: a reachable state steps outside it: from value = 7 "
                                                 "on action = Inc, authorized = true the core decides accept and "
                                                 "reaches value = 8"), f"the weak invariant's counterexample: {weak}")
    revised = judged(submit("invariant i2: value <= 8;\n"))
    expect(revised[0].startswith("fact i2: inductive: holds on all 9 reachable states"), f"revised: {revised}")
    rounds = workspace.load()["invariants"]
    expect(rounds["i1"]["counterexample_to_induction"]["decision"] == "accept"
           and rounds["i1"]["counterexample_to_induction"]["successor"] == [8]
           and rounds["i2"]["scope"] == {"declared_states": 9, "admitted": 9, "reachable": 9, "exhaustive": True},
           f"feedback rounds recorded: {rounds}")
    same = judged(submit("invariant i3: !(value > 8);\n"))
    expect(same == ["same i3 as i2: 9 states, event 3, inductive;"], f"the invariant in other words: {same}")
    # laws: one holds, one has a false alarm on the core
    laws = judged(submit("law l1 on accept: post.value <= 8;\nlaw l2 on accept: post.value <= 7;\n"))
    expect(laws[0].startswith("fact l1: holds: applies on 16 inputs") and
           laws[1].startswith("fact l2: false alarm: the core decides against it"), f"laws: {laws}")
    # a scenario that one success kills the reject-everything core with
    scenario = judged(submit("scenario s1 from genesis;\nstep s1 1: action = Inc, authorized = true -> accept value = 1;\n"))
    expect(scenario[0].startswith("fact s1: passed: all 1 steps match the core") and
           "the reject-everything core fails it" in scenario[0], f"scenario: {scenario}")
    # teach-back: exceptions, and a rule the requirements do not force becomes an owner question
    rule = judged(submit("rule t2: where action == Inc && value == 8 -> reject at_capacity;\n"))
    expect(rule[0].startswith("exception t2: 1 of 2"), f"teach-back exceptions: {rule}")
    exact = judged(submit("rule t3: where authorized && action == Inc && value == 8 -> reject at_capacity;\n"))
    expect(exact[0] == "exact t3: 1;" and exact[-1].startswith("ask q1 "), f"exact rule and its question: {exact}")
    recorded = judged(workspace.answer("q1", "1"))
    expect(recorded[-1].startswith("fact q1: ") and "requirements recorded from rule t3" in recorded[-1],
           f"the owner requires the rule: {recorded}")
    # the freedom question, answered; then an answer that conflicts changes nothing
    freedom = judged(submit("probe f: freedom;\n"))
    expect(freedom[-1].startswith("ask q2 where !context.authorized"), f"freedom question: {freedom}")
    answered = judged(workspace.answer("q2", "1"))
    expect("requirements recorded" in answered[-1], f"owner answer: {answered}")
    next_question = judged(submit("probe f2: freedom;\n"))
    expect(next_question[-1].startswith("ask q3 ") and "!context.authorized" not in next_question[-1],
           f"the next question leaves the answered inputs out: {next_question}")
    # an owner's rule required as it is, then a wider rule answered with a reject: on the input both name the
    # second answer leaves no admissible decision, so it is refused and nothing changes
    first = judged(workspace.say("rule w1: where authorized && action == Inc && value == 0 -> accept value = value + 1;"))
    expect(first[0] == "holds w1: 1;" and first[-1].startswith("ask "), f"owner rule w1: {first}")
    judged(workspace.answer(first[-1].split()[1], "1"))
    wider = judged(workspace.say("rule w2: where authorized && action == Inc && value < 2 -> accept value = value + 1;"))
    expect(wider[0] == "holds w2: 2;" and wider[-1].startswith("ask "), f"owner rule w2: {wider}")
    qid = wider[-1].split()[1]
    state = workspace.load()
    before = (json.dumps(state["questions"][qid], sort_keys=True), json.dumps(state.get("confirmed", [])))
    options = LW._options_of(state, ctx, qid)
    reject = next((k for k, o in enumerate(options) if o.cls == "Reject"), None)
    expect(reject is not None, f"a reject is offered: {wider}")
    events = judged(workspace.submit(head.replace("author model;", "author owner;") + f"answer {qid}: "
                                     + Z.option_symbolic(options[reject], ctx) + ";\n", "owner"))
    after = workspace.load()
    expect(events[-1].endswith("refused: answer-conflicts-with-confirmed-requirements;"),
           f"a conflicting answer is refused: {events}")
    expect((json.dumps(after["questions"][qid], sort_keys=True), json.dumps(after.get("confirmed", []))) == before,
           "a refused answer leaves the question and the requirements unchanged")
    # decompositions
    done = judged(submit("component guard of d1: reads authorized; decides not_authorized;\n"
                         "component counter of d1: owns value; reads action; decides at_capacity, at_zero;\n"
                         "decompose d1: guard, counter;\n"))
    expect(sum(1 for s in done if s.startswith("case ")) == 6 and any(s.startswith("decomposed d1: 36 tuples")
                                                                      for s in done)
           and done[-1].startswith("fact d1: the composition checker's joint step equals the core on all 36 inputs"),
           f"projected and composed decomposition: {done}")
    blind = judged(submit("component guard of d2: reads authorized; decides not_authorized;\n"
                          "component counter of d2: owns value; decides at_capacity, at_zero;\n"
                          "decompose d2: guard, counter;\n"))
    expect(blind[0].startswith("interface d2 counter at ") and blind[0].endswith(": reads action;"),
           f"interface counterexample: {blind}")
    # a negated number keeps the loop language's meaning when routed: `!value` is `value == 0`
    count = judged(submit("probe p1: count !value;\n"))
    expect(count == ["fact p1: 4 of 36 inputs satisfy it;"], f"a negated number routed exactly: {count}")
    # two rules with the same region and claim, but another successor outside the region, are judged apart,
    # never answered from memory
    m1 = judged(submit("rule m1: where authorized && action == Inc && value < 8 -> accept value = value + 1;\n"))
    m2 = judged(submit("rule m2: where authorized && action == Inc && value < 8 -> "
                       "accept value = action == Inc ? value + 1 : value - 1;\n"))
    expect(m1[0] == "exact m1: 8;" and m2[0] == "holds m2: 8;", f"rules keyed by their whole meaning: {m1} {m2}")
    report["judged_events"] = len(workspace.load()["events"])
    return path


def drift_workspace(work: Path, cli: Path, report: dict) -> Path:
    """A contract that changes after initialization: the workspace's judges keep reading the bound copy, a
    new workspace on the changed contract gets the fresh verdict, and a changed copy is refused."""
    sys.path.insert(0, str(ROOT / "tools"))
    import instantiate_core
    sys.path.pop(0)
    contract = work / "drift" / "contract"
    contract.parent.mkdir()
    instantiate_core.instantiate("bounded-counter", {"C": 8}, contract, include_proof=False)
    policy_path = contract / "v2" / "policy.json"
    normal = policy_path.read_text(encoding="ascii")
    # the increment keeps the value: `value == 0` is inductive there, and false for the normal counter, with the
    # same admitted states (the lemma judge's digest covers those only)
    policy = json.loads(normal)
    for case in policy["cases"]:
        if case["class"] == "Accept" and case["post"].get("110") == "value + 1":
            case["post"]["110"] = "value"
    policy_path.write_text(json.dumps(policy, indent=2) + "\n", encoding="ascii")

    def open_workspace(name: str):
        path = work / f"{name}.json"
        workspace = LW.LoopWorkspace(path, cli)
        workspace.init(contract=contract)
        ctx = Z.Context(workspace.load()["subject"])
        head = Z.HEADER["symbolic"] + "\n" + Z.subject_line(ctx.subject_ref, False) + "\nauthor model;\n"
        return path, workspace, lambda body: [e["symbolic"] for e in workspace.submit(head + body, "model")
                                              if e["author"] == "judge"]

    path, workspace, submit = open_workspace("drift")
    first = submit("invariant original: value == 0;\n")
    expect(first[0].startswith("fact original: inductive"), f"value == 0 when the increment keeps it: {first}")
    policy_path.write_text(normal, encoding="ascii")          # the source contract changes
    again = submit("invariant respelled: !(value != 0);\n")
    expect(again == ["same respelled as original: 9 states, event 1, inductive;"],
           f"the session keeps judging its own subject: {again}")
    _, _, fresh = open_workspace("drift-new")
    changed = fresh("invariant original: value == 0;\n")
    expect(changed[0].startswith("fact original: false"), f"a new workspace sees the changed contract: {changed}")
    copy = workspace.data / "subject" / "v2" / "policy.json"
    copy.chmod(0o644)
    copy.write_text(normal, encoding="ascii")
    refused = submit("invariant third: value <= 8;\n")
    expect(len(refused) == 1 and "contract copy changed" in refused[0], f"a changed copy is refused: {refused}")
    report["drift_events"] = len(workspace.load()["events"])
    return path


def family_workspace(work: Path, cli: Path, report: dict) -> Path:
    path = work / "family.json"
    workspace = LW.LoopWorkspace(path, cli)
    params = [{"C": c} for c in range(1, 9)]
    workspace.init(family="bounded-counter", parameters=params, fit=params[:3])
    head = Z.HEADER["symbolic"] + "\nsubject family bounded-counter;\nauthor model;\n"
    events = workspace.submit(head + "family f1 out0 = " + SKETCH.replace("? ?c : 0) ? ?c : ?c", "? ?c : 0) ? 149 : 147")
                              + ";\nfamily f1 out1 = value + 1;\nfamily f1 out2 = value - 1;\n", "model")
    texts = [e["english"] for e in events if e["author"] == "judge"]
    expect(any("8 of 8 instances accepted" in t for t in texts), f"family verdicts: {texts}")
    expect(any("hole 0 fits parameter C" in t for t in texts), f"the hole fits $C: {texts}")
    expect(any("accepted on 8 of 8 instances; 5 of the checked instances were not fitted on, "
               "and it is accepted on 5 of those" in t for t in texts),
           f"the fit holds on unfitted instances: {texts}")
    report["family_events"] = len(workspace.load()["events"])
    return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--out", type=Path)
    options = parser.parse_args()
    cli = loop_doors.resolve_cli(options.cli)
    env = {**os.environ, "ZENO_FCIS_CLI": str(cli)}
    report = {"cli": str(cli), "authority": "none"}
    with tempfile.TemporaryDirectory(prefix="zal-loop-check-") as temp:
        work = Path(temp)
        try:
            core = core_workspace(work, cli, env, report)
            mcp_session(core["path"], core["head"], env, report)
            family = family_workspace(work, cli, report)
            judges = judges_workspace(work, cli, report)
            drift = drift_workspace(work, cli, report)
            report["reparsed_statements"] = sum(reparse(LW.LoopWorkspace(p, cli).load())
                                                for p in (core["path"], family, judges, drift))
            report["status"] = "passed"
        except (Failed, ValueError, OSError, KeyError) as error:
            report["status"] = "failed"
            report["failure"] = str(error)[:4000]
        if options.out:
            options.out.mkdir(parents=True, exist_ok=True)
            for name in ("counter.json", "family.json", "judged.json"):
                if (work / name).is_file():
                    shutil.copy(work / name, options.out / name)
    print(json.dumps(report, indent=1, sort_keys=True))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
