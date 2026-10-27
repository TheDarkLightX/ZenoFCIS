#!/usr/bin/env python3
"""Compare finite distributions and rewards with an independent Fraction oracle."""

import argparse
from collections import Counter
from fractions import Fraction
from itertools import combinations
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]



def fraction(value):
    return Fraction(int(value["numerator"]), int(value["denominator"]))


def reward_reference(cli, app, project, policy):
    """Compute from individual integer outcomes, independently of histogram code."""
    count = 0
    declaration = "distribution 601 reporters on post.100.110 = uniform over {0, 1, 2, 3};"
    for bits in range(4):
        n = 2**bits
        for expression, values in [
            ("context.102.130", list(range(n))),
            ("0 - context.102.130", [-r for r in range(n)]),
            ("2", [2] * n),
        ]:
            mean = sum(map(Fraction, values), Fraction()) / n
            variance = sum((Fraction(v)**2 for v in values), Fraction()) / n - mean**2
            for name, expected, transform in [
                ("expectation", mean, lambda v: Fraction(v)),
                ("variance", variance, lambda v: (Fraction(v) - mean)**2),
                ("threshold 0", Fraction(sum(v >= 0 for v in values), n), lambda v: Fraction(v >= 0)),
                (f"threshold {max(values)}", Fraction(sum(v >= max(values) for v in values), n), lambda v: Fraction(v >= max(values))),
                ("excess 0", sum((Fraction(max(0,v)) for v in values), Fraction()) / n, lambda v: Fraction(max(0,v))),
                ("excess 2", sum((Fraction(max(0,v-2)) for v in values), Fraction()) / n, lambda v: Fraction(max(0,v-2))),
            ]:
                # Equality at the bound plus upper, lower and equality violations.
                for comparison, bound, exit_code in [("=", expected, 0), ("<=", expected-1, 1), (">=", expected+1, 1), ("=", expected+1, 1)]:
                    source = project.replace("Reporter in 0..=3", f"Reporter in {min(0,min(values))}..={max(3,max(values))}")
                    source = source.replace("RandomBits in 0..=3", f"RandomBits in 0..={n-1}")
                    source = source.replace("bits 2", f"bits {bits}").replace("post.100.110 >= 0", f"post.100.110 >= {min(0,min(values))}")
                    reward = f"reward 610 payout random {{600}} on post.100.110 {{ {name} {comparison} {bound.numerator}/{bound.denominator}; }}"
                    source = source.replace(declaration, reward)
                    policy["cases"][0]["post"]["110"] = expression
                    (app / "project.zeno").write_text(source)
                    (app / "v2/policy.json").write_text(json.dumps(policy))
                    result = subprocess.run(["nice", "-n", "19", "ionice", "-c3", str(cli), "contract", "distribution", str(app)], capture_output=True, check=False, timeout=60)
                    assert result.returncode == exit_code, (bits, expression, name, comparison, result.stderr, result.stdout)
                    packet = json.loads(result.stdout)
                    assert packet["premise"] == "given uniform independent random bits, independent of the fixed non-random inputs"
                    assert packet["status"] == ("checked" if exit_code == 0 else "refuted")
                    if exit_code:
                        detail = packet["result"]
                        assert fraction(detail["actual"]) == expected
                        assert fraction(detail["bound"]) == bound
                        rows = detail["outcomes"]
                        contributions = []
                        tapes = []
                        for row in rows:
                            value = int(row["reward"])
                            preimages = [int(t[0]) for t in row["random_values"]]
                            assert all(values[r] == value for r in preimages)
                            mass = Fraction(len(preimages), n)
                            assert fraction(row["mass"]) == mass
                            contribution = mass * (transform(value) - bound)
                            assert fraction(row["difference_contribution"]) == contribution
                            contributions.append(contribution)
                            tapes.extend(preimages)
                        assert sorted(tapes) == list(range(n))
                        assert sum(contributions, Fraction()) == expected - bound
                        assert contributions == sorted(contributions, reverse=expected > bound)
                    count += 1
    return count

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    args = parser.parse_args()
    example = ROOT / "examples/distribution-laws/reporter-choice"
    project = (example / "project.zeno").read_text()
    policy = json.loads((example / "v2/policy.json").read_text())
    checked = 0
    refuted = 0
    with tempfile.TemporaryDirectory(prefix="distribution-reference-") as temporary:
        app = Path(temporary)
        (app / "v2").mkdir()
        for bits in range(4):
            n = 2**bits
            for k in range(1, 6):
                source = project.replace("Reporter in 0..=3", f"Reporter in 0..={k-1}")
                source = source.replace("RandomBits in 0..=3", f"RandomBits in 0..={n-1}")
                source = source.replace("bits 2", f"bits {bits}")
                source = source.replace("{0, 1, 2, 3}", "{" + ", ".join(map(str, range(k))) + "}")
                policy["cases"][0]["post"]["110"] = f"context.102.130 - {k} * div_floor(context.102.130, {k})"
                (app / "project.zeno").write_text(source)
                (app / "v2/policy.json").write_text(json.dumps(policy))
                result = subprocess.run(["nice", "-n", "19", "ionice", "-c3", str(args.cli),
                                         "contract", "distribution", str(app)],
                                        capture_output=True, check=False, timeout=60)
                packet = json.loads(result.stdout)
                counts = Counter(r % k for r in range(n))
                actual = {v: Fraction(c, n) for v, c in counts.items()}
                target = {v: Fraction(1, k) for v in range(k)}
                matches = actual == target
                assert result.returncode == (0 if matches else 1), (bits, k, result.stderr)
                assert packet["status"] == ("checked" if matches else "refuted")
                assert packet["premise"] == "given uniform independent random bits, independent of the fixed non-random inputs"
                if matches:
                    checked += 1
                    assert packet["measured"]["tuples"] == n * k
                else:
                    refuted += 1
                    detail = packet["result"]
                    decode = lambda rows: {int(row["value"]): Fraction(int(row["numerator"]), int(row["denominator"])) for row in rows}
                    assert decode(detail["actual"]) == actual
                    assert decode(detail["expected"]) == target
                    witness = detail["witness"]
                    tapes = [int(values[0]) for values in witness["random_values"]]
                    outcome = int(witness["outcome"])
                    assert len(set(tapes)) == len(tapes) == witness["size"]
                    assert all(0 <= r < n and r % k == outcome for r in tapes)
                    assert Fraction(len(tapes), n) > target.get(outcome, 0)
                    # Brute force every smaller subset and its observed event.
                    for size in range(len(tapes)):
                        for smaller in combinations(range(n), size):
                            outcomes = {r % k for r in smaller}
                            assert Fraction(size, n) <= sum((target[v] for v in outcomes), Fraction())
                    assert packet["measured"]["tuples"] == n
        rewards = reward_reference(args.cli, app, project, policy)
    print(f"reward-reference: PASS ({rewards} exact comparisons; four measures; zero bits, single outcomes, signed rewards, exact-bound equality and ranked witnesses)")
    print(f"distribution-reference: PASS (20 boundary cases; {checked} checked, {refuted} refuted; exact Fraction oracle and brute-force witness minimality)")


if __name__ == "__main__":
    main()
