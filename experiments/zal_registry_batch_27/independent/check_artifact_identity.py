"""Check own-source receipts and refuse cross-source packets and receipts."""
from __future__ import annotations

import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys


def need(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--own-packet", required=True, type=Path)
    parser.add_argument("--other-packet", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(args.root.resolve() / "integrations/zal"))
    from zal_notation import core, lens_library as V, library_lens_browser as B
    result = {"schema": "zal/independent-artifact-identity-controls/1", "pass": False, "controls": []}
    with args.output.open("x") as stream:
        try:
            for role, folder in (("own", args.own_packet), ("other", args.other_packet)):
                request = core.loads((folder / "request.json").read_text())
                packet = core.loads((folder / "data/packet.json").read_text())
                for subject in packet["subjects"]:
                    for view in subject["lenses"]:
                        pair = view.get("_receipt")
                        if not pair or pair["receipt"]["status"] != "checked":
                            continue
                        app, receipt = pair["application"], pair["receipt"]
                        before = deepcopy((request["library"], app, receipt))
                        fresh = V.check(request["library"], app["entry_digest"], app, receipt["budget"])
                        verified = V.verify_receipt(request["library"], app["entry_digest"], app, receipt)
                        need((fresh["status"] == "checked" and verified) if role == "own"
                             else (fresh["status"] == "unsupported" and not verified),
                             "incorrect source-specific receipt admission")
                        need((request["library"], app, receipt) == before, "checking mutated caller evidence")
                        result["controls"].append({"origin": role, "lens": view["id"],
                            "status": fresh["status"], "verified": verified, "reason": fresh.get("reason")})
                try:
                    replay = B.replay(folder, packet["requestSha256"])
                except core.NotationError as error:
                    need(role == "other", "own-source replay refused")
                    result["controls"].append({"origin": role, "packet_replay": "refused", "reason": str(error)})
                else:
                    need(role == "own" and replay["status"] == "replayed", "cross-source packet replay accepted")
                    result["controls"].append({"origin": role, "packet_replay": replay["status"]})
            need(any(x.get("verified") is True for x in result["controls"]), "no positive receipt control")
            need(any(x.get("verified") is False for x in result["controls"]), "no negative receipt control")
            result["pass"] = True
        except BaseException as error:
            result["failure"] = {"type": type(error).__name__, "message": str(error)}
        finally:
            stream.write(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
