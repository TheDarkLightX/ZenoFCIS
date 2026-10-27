"""Inert archived rounding seeds and exact serialization. No search code."""
import dataclasses
from fractions import Fraction
from spot_reference import Command, World, step


def seeds():
    genesis = World(None, (0, 0, 0, 0, 10**9, 10**9, 10**9, 10**9))
    return (
        (genesis, (Command(0, a=100000, b=200000, sqrt=141421),), Command(1, a=51)),
        (genesis, (Command(0, a=156331, b=79390, sqrt=111405),
                   Command(3, a=174046, b=30619, actor=1, recipient=1)),
                   Command(1, direction=1, a=16)),
        (genesis, (Command(0, a=129879, b=61233, sqrt=89178),
                   Command(3, a=191320, b=4395, actor=1, recipient=1)),
                   Command(1, direction=1, a=7)),
    )


def initialized(seed):
    genesis, prefix, _ = seed
    current = genesis
    for command in prefix:
        result = step(current, command)
        if result.reason is not None:
            raise AssertionError("reachable_prefix_refused")
        current = result.post
    return current


def json_value(value):
    if isinstance(value, Fraction):
        return str(value)
    if dataclasses.is_dataclass(value):
        return json_value(dataclasses.asdict(value))
    if isinstance(value, dict):
        return {str(k): json_value(v) for k, v in value.items()}
    if isinstance(value, (tuple, list)):
        return [json_value(v) for v in value]
    return value
