"""Retain the unchanged native adapter restriction on initial-state names."""
import argparse
from dataclasses import replace
import json
from pathlib import Path
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--root", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
sys.path.insert(0, str(args.root.resolve() / "integrations/zal"))
from zal_notation import lenses as L

original = L.counter_contract()
L.make_source(original)  # Supported positive control, with initial unseen.
rename = lambda value: "latched" if value == "unseen" else value
inputs = tuple(replace(port, values=tuple(sorted(rename(v) if i == 0 else v for v in port.values)))
               for i, port in enumerate(original.inputs))
outputs = tuple(replace(port, values=tuple(sorted(rename(v) if i == 1 else v for v in port.values)))
                for i, port in enumerate(original.outputs))
rows = {(rename(x[0]), *x[1:]): tuple((y[0], rename(y[1]), *y[2:]) for y in ys)
        for x, ys in original.rows}
contract = L.finite.build(inputs, outputs, rows, (("state", "next_state", "latched"),))
try:
    L.make_source(contract)
except L.finite.Refused as error:
    result = {"schema": "zal/initial-name-restriction-control/1", "positive_initial": "unseen",
              "negative_initial": "latched", "status": "refused", "reason": str(error),
              "scope": "Native finite adapter restriction reproduced; no performance regression or general ZAL authoring restriction inferred."}
else:
    raise RuntimeError("documented initial-name restriction was not reproduced")
with args.output.open("x") as stream:
    stream.write(json.dumps(result, indent=2, sort_keys=True) + "\n")
print(json.dumps(result))
