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


# Added from the separately retained Root guard-chain references, before their
# policy integration. These functions are test expectations, never callbacks.
def counter(capacity, values):
    value, action, authorized = values
    if not authorized:
        return result(200, (value,))
    if action == 150:
        return result(201, (value,)) if value == capacity else result(None, (value + 1,))
    if action == 151:
        return result(202, (value,)) if value == 0 else result(None, (value - 1,))
    raise ValueError("action outside the declared finite domain")


def budget(capacity, values):
    remaining, amount, authorized = values
    if not authorized:
        return result(200, (remaining,))
    return result(201, (remaining,)) if amount > remaining else result(None, (remaining - amount,))


def register(capacity, values):
    value, version, expected, new, authorized = values
    pre = value, version
    if not authorized:
        return result(200, pre)
    if expected != version:
        return result(201, pre)
    return result(202, pre) if version == capacity else result(None, (new, version + 1))


def slot(capacity, values):
    seen, key, value, new_key, new_value, authorized = values
    pre = seen, key, value
    if not authorized:
        return result(200, pre)
    if seen and key == new_key and value != new_value:
        return result(201, pre)
    return result(None, (1, new_key, new_value))


def retry(capacity, values):
    remaining, closed, finish, authorized = values
    pre = remaining, closed
    if not authorized:
        return result(200, pre)
    if closed:
        return result(201, pre)
    if finish:
        return result(None, (remaining, 1))
    return result(202, pre) if remaining == 0 else result(None, (remaining - 1, 0))


def phase(capacity, values):
    old, reset, authorized = values
    if not authorized:
        return result(200, (old,))
    if reset:
        return result(None, (0,)) if old == capacity else result(201, (old,))
    return result(202, (old,)) if old == capacity else result(None, (old + 1,))


def deadline(capacity, values):
    due, last, reached, now, authorized = values
    pre = due, last, reached
    if not authorized:
        return result(200, pre)
    if now < last:
        return result(201, pre)
    if reached:
        return result(202, pre)
    return result(203, pre) if now < due else result(None, (due, now, 1))


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
    for capacity in range(1, 9):
        units = range(capacity + 1)
        definitions = (
            ("bounded_counter", [units, range(150, 152), [0, 1]], counter),
            ("consumable_budget", [units, units, [0, 1]], budget),
            ("versioned_register", [units, units, units, units, [0, 1]], register),
            ("idempotency_slot", [[0, 1], units, units, units, units, [0, 1]], slot),
            ("retry_budget", [units, [0, 1], [0, 1], [0, 1]], retry),
            ("finite_phase_machine", [units, [0, 1], [0, 1]], phase),
            ("logical_deadline", [units, units, [0, 1], units, [0, 1]], deadline),
        )
        for family, domains, transition in definitions:
            for values in itertools.product(*domains):
                yield {"family": family, "parameters": {"C": capacity},
                       "input": list(values), "expected": transition(capacity, values)}


if __name__ == "__main__":
    rows = list(cases())
    counts = {family: sum(row["family"] == family for row in rows)
              for family in sorted({row["family"] for row in rows})}
    assert counts == {"reservation_pool": 2216, "rate_limiter": 2646, "approval_queue": 1296,
                      "bounded_counter": 176, "consumable_budget": 568,
                      "versioned_register": 30664, "idempotency_slot": 61328,
                      "retry_budget": 352, "finite_phase_machine": 176, "logical_deadline": 8096}
    output = Path(__file__).with_name("component-independent-expected.json")
    assert not output.exists()
    output.write_text(json.dumps({"format": "g1-independent-expected/1", "authority": "none",
                                 "source": "Root reference from public mathematical brief",
                                 "owner_adoption": False, "counts": counts, "cases": rows},
                                sort_keys=True, separators=(",", ":")) + "\n")
    print(json.dumps({"path": str(output), "cases": len(rows), "counts": counts}))
