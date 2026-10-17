#!/usr/bin/env python3
"""Real-CLI requirements workspace check with owner-channel and replay evidence."""
from pathlib import Path
import argparse
import json
import tempfile
from behavior import Refusal
import loop_language as Z
from loop_workspace import LoopWorkspace
import loop_versions as V


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True)
    args = parser.parse_args()
    specification = {"schema": V.SCHEMA, "inputs": [{"name": "x", "domain": {"kind": "Bool"}}],
                     "outputs": [{"kind": "Bool"}], "sketch": ["x ? ?c : ?c"], "requirements": []}
    with tempfile.TemporaryDirectory(prefix="zeno-versions-workspace-") as temp:
        directory = Path(temp)
        given = directory / "specification.json"
        given.write_text(json.dumps(specification))
        workspace = LoopWorkspace(directory / "workspace.json", args.cli)
        workspace.init(specification=given)
        first = workspace.view()["open_questions"][0]
        ctx = Z.Context(workspace.load()["subject"])
        head = f"zal fcis loop 1;\n{Z.subject_line(ctx.subject_ref, False)}\nauthor model;\n"
        before = workspace.load()["version_space"]["specification"]
        for act in (f"version-answer {first['id']}: [0];", "version-stop owner;", "version-count forged: 1;"):
            try:
                workspace.submit(head + act + "\n", "model")
            except Refusal:
                pass
            else:
                raise AssertionError("model owner/judge act was accepted")
        workspace.submit(head + f"version-suggest {first['id']}: [0];\n", "model")
        assert workspace.load()["version_space"]["specification"] == before
        for _ in range(2):
            q = workspace.view()["open_questions"][0]
            workspace.answer(q["id"], V.tuple_text((q["input"][0],)))
        state = workspace.load()
        assert len(state["version_space"]["specification"]["requirements"]) == 2
        assert not workspace.view()["open_questions"]
        checked = [e for e in state["events"] if e["act"]["kind"] == "version-determined"]
        assert len(checked) == 1
        assert checked[0]["detail"]["detail"]["candidate"]["status"] == "checked"
        try:
            workspace.answer(first["id"], "[0]")
        except Refusal:
            pass
        else:
            raise AssertionError("stale owner answer was accepted")
        requirements = state["version_space"]["specification"]["requirements"]
        workspace.stop_versions()
        assert workspace.load()["version_space"]["specification"]["requirements"] == requirements
        ctx = Z.Context(workspace.load()["subject"])
        for e in workspace.load()["events"]:
            assert Z.parse_statement(e["symbolic"], ctx, False) == Z.parse_statement(e["english"], ctx, True)
    print(json.dumps({"real_cli_workspace": "passed", "authority": "none"}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
