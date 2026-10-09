"""Independent expected decisions from the public G1 mathematical brief.

Authored by Root before reading any family template or lowering implementation.
This is a test oracle, not a shipped functional core, owner-adoption receipt,
authentication mechanism, or machine-checked family theorem.
"""

import itertools
import json
from pathlib import Path


def result(reason, post):
    return {"class": "Accept" if reason is None else "Reject",
            "reason": reason, "post": list(post), "deliveries": []}


def reservation(capacity, request_max, values):
    available, reserved, action, quantity, authorized = values
    pre = available, reserved
    if available + reserved > capacity:
        return result(204, pre)
    if not authorized:
        return result(200, pre)
    if action == 150:
        return (result(201, pre) if quantity > available else
                result(None, (available - quantity, reserved + quantity)))
    if action == 151:
        return (result(202, pre) if quantity > reserved else
                result(None, (available + quantity, reserved - quantity)))
    if action == 152:
        return (result(202, pre) if quantity > reserved else
                result(None, (available, reserved - quantity)))
    if action == 153:
        return (result(203, pre) if available + reserved + quantity > capacity else
                result(None, (available + quantity, reserved)))
    raise ValueError("action outside the declared finite domain")


def rate(window, limit, values):
    start, used, action, now, authorized = values
    assert action == 150
    pre = start, used
    if not authorized:
        return result(200, pre)
    if now < start:
        return result(201, pre)
    if now - start >= window:
        return result(None, (now, 1))
    if used == limit:
        return result(202, pre)
    return result(None, (start, used + 1))


def approval(quorum, values):
    phase, *rest = values
    votes = rest[:3]
    action, principal, authorized = rest[3:]
    count = sum(votes)
    pre = phase, *votes
    if (phase == 150 and count != 0) or (phase == 152 and count < quorum):
        return result(200, pre)
    if not authorized:
        return result(201, pre)
    if phase == 152 or (action == 160 and phase != 150) or (action != 160 and phase != 151):
        return result(202, pre)
    if action == 160:
        return result(None, (151, False, False, False))
    if action == 161:
        if votes[principal]:
            return result(203, pre)
        updated = [v or i == principal for i, v in enumerate(votes)]
        return result(None, (151, *updated))
    if action == 162:
        return result(204, pre) if count < quorum else result(None, (152, *votes))
    raise ValueError("action outside the declared finite domain")


def cases():
    for capacity in range(1, 5):
        for maximum in range(1, min(capacity, 3) + 1):
            domains = [range(capacity + 1), range(capacity + 1), range(150, 154),
                       range(1, maximum + 1), [False, True]]
            for values in itertools.product(*domains):
                yield {"family": "reservation_pool", "parameters": {"C": capacity, "Q": maximum},
                       "input": list(values), "expected": reservation(capacity, maximum, values)}
    for window in range(1, 4):
        for limit in range(1, 4):
            domains = [range(7), range(limit + 1), [150], range(7), [False, True]]
            for values in itertools.product(*domains):
                yield {"family": "rate_limiter", "parameters": {"W": window, "N": limit},
                       "input": list(values), "expected": rate(window, limit, values)}
    for quorum in range(1, 4):
        domains = [range(150, 153), [False, True], [False, True], [False, True],
                   range(160, 163), range(3), [False, True]]
        for values in itertools.product(*domains):
            yield {"family": "approval_queue", "parameters": {"K": quorum},
                   "input": list(values), "expected": approval(quorum, values)}


if __name__ == "__main__":
    rows = list(cases())
    counts = {family: sum(row["family"] == family for row in rows)
              for family in ("reservation_pool", "rate_limiter", "approval_queue")}
    assert counts == {"reservation_pool": 2216, "rate_limiter": 2646, "approval_queue": 1296}
    output = Path(__file__).with_name("component-independent-expected.json")
    assert not output.exists()
    output.write_text(json.dumps({"format": "g1-independent-expected/1", "authority": "none",
                                 "source": "Root reference from public mathematical brief",
                                 "owner_adoption": False, "counts": counts, "cases": rows},
                                sort_keys=True, separators=(",", ":")) + "\n")
    print(json.dumps({"path": str(output), "cases": len(rows), "counts": counts}))
