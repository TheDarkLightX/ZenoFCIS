"""Revision-bound reviewed declaration export. Cooperative review, no authority."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

from behavior import Refusal, check
from factory import lower, write_new
from shared import SharedWorkspace
from workflow import checker_identity


RECEIPT = "zal-review.json"


def reviewed(session, revision):
    if revision != session.model.revision:
        raise Refusal("Requested revision differs from current meaning; refresh and review")
    if session.agreement != "human-accepted" or session.proposal:
        raise Refusal("Export needs human review of this exact meaning and no pending candidate")
    agreement = next((event for event in reversed(session.events)
                      if event.get("act") == "agree" and event.get("actor") == "human"), None)
    data = agreement.get("data", {}) if agreement else {}
    identity = checker_identity()
    if (data.get("candidate_revision") != revision or data.get("checker") != identity or
            data.get("canonical_paraphrase") != session.model.render(True)):
        raise Refusal("Approval lacks the current model/checker binding; show and review the meaning again")
    evidence = check(session.model, identity)
    if evidence["status"] != "pass-with-scope":
        raise Refusal("Export needs completed supported-profile checks; unresolved requirements do not lower")
    return session.model, evidence, agreement


def declarations(session, revision):
    model, evidence, agreement = reviewed(session, revision)
    files = lower(model)
    receipt = {"schema": "zal/reviewed-declarations/1", "revision": revision,
               "profile": evidence["profile"], "checker": evidence["checker"],
               "agreement_sequence": agreement["sequence"], "input_tuples": evidence["input_tuples"],
               "files": {name: hashlib.sha256(text.encode("ascii")).hexdigest() for name, text in files.items()},
               "scope": "reviewed finite control decisions, reasons, successor and empty deliveries",
               "approval": "trusted cooperative local gesture; not authenticated human approval",
               "authority": "none", "factory_qualification": "not-run"}
    return {**files, RECEIPT: json.dumps(receipt, indent=2) + "\n"}, receipt


def export(workspace, revision, destination):
    def action(session):
        files, receipt = declarations(session, revision)
        write_new(files, destination)
        return {"status": "exported-reviewed-declarations", **receipt}
    return SharedWorkspace(workspace).use(action)


def verify(workspace, revision, destination):
    def action(session):
        expected, receipt = declarations(session, revision)
        if not destination.is_dir() or destination.is_symlink():
            raise Refusal("Export must be a regular directory")
        names = set()
        directories_seen = set()
        count = 0
        def unreadable(error):
            raise Refusal(f"Export directory cannot be read: {error.filename}")
        for parent, directories, files in os.walk(destination, followlinks=False, onerror=unreadable):
            for name in directories + files:
                path = Path(parent) / name
                count += 1
                if count > 64 or path.is_symlink():
                    raise Refusal("Export has excessive entries or a symlink")
            directories_seen.update((Path(parent) / name).relative_to(destination).as_posix() for name in directories)
            names.update((Path(parent) / name).relative_to(destination).as_posix() for name in files)
        expected_directories = {str(parent) for name in expected for parent in Path(name).parents
                                if str(parent) != "."}
        if names != set(expected) or directories_seen != expected_directories:
            raise Refusal("Export file inventory differs from reviewed declarations")
        for name, text in expected.items():
            path = destination / name
            data = text.encode("ascii")
            if path.stat().st_size != len(data) or path.read_bytes() != data:
                raise Refusal(f"Export differs from reviewed declarations: {name}")
        return {"status": "verified-reviewed-declarations", **receipt}
    return SharedWorkspace(workspace).use(action)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["export", "verify"])
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--out", type=Path, required=True)
    options = parser.parse_args()
    try:
        operation = export if options.command == "export" else verify
        print(json.dumps(operation(options.workspace, options.revision, options.out), indent=2))
        return 0
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "refused", "diagnostic": str(error), "authority": "none"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
