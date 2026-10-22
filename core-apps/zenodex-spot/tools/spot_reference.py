"""Independent big-integer oracle for the closed two-user Spot example.

This module grants no Authority or publication capability. Snapshots, commands,
results and effects own only immutable scalar tuples. All mutation is fresh
invocation scratch, discarded on refusal and frozen before returning.
"""
from __future__ import annotations

from dataclasses import dataclass

N, J, K, U = 3_000_000_000, 1_000_000_000, 1000, (1 << 128) - 1
KINDS = ("CreatePool", "SwapExactIn", "SwapExactOut", "AddLiquidity", "RemoveLiquidity")
# Complete key order: pool X/Y, ingress X/Y, alice X/Y, bob X/Y.


@dataclass(frozen=True)
class Pool:
    r0: int
    r1: int
    supply: int
    status: int = 0  # ACTIVE=0, FROZEN=1, DISABLED=2.

    def __post_init__(self):
        if any(type(v) is not int for v in (self.r0, self.r1, self.supply, self.status)):
            raise TypeError("pool fields must be immutable exact integers")


@dataclass(frozen=True)
class World:
    pool: Pool | None = None
    balances: tuple[int, ...] = (0,) * 8
    shares: tuple[int, ...] = (0, 0, 0)

    def __post_init__(self):
        if self.pool is not None and type(self.pool) is not Pool: raise TypeError("owned Pool required")
        for values, width in ((self.balances, 8), (self.shares, 3)):
            if type(values) is not tuple or len(values) != width or any(type(v) is not int for v in values):
                raise TypeError("snapshot collections must be owned tuples of exact integers")


@dataclass(frozen=True)
class Command:
    action: int
    direction: int = 0
    a: int = 0
    b: int = 0
    limit0: int = 0
    limit1: int = 0
    sqrt: int = 0
    actor: int = 0
    recipient: int = 0

    def __post_init__(self):
        if any(type(getattr(self,name)) is not int for name in self.__dataclass_fields__):
            raise TypeError("command fields must own immutable exact integers")


@dataclass(frozen=True)
class Effect:
    channel: int  # 300=X transfer, 301=Y transfer, 302=LP delta.
    source: int
    destination: int
    amount: int


@dataclass(frozen=True)
class Decision:
    reason: str | None
    post: World
    effects: tuple[Effect, ...] = ()
    outputs: tuple[int, ...] = ()


def mf(a, b, d): return a * b // d
def mc(a, b, d): return (a * b + d - 1) // d
def bounded(value, maximum): return type(value) is int and 0 <= value <= maximum


def balance_step(balance, debit, credit, ingress=False, expected=0):
    if ingress:
        if debit != 0 or credit != expected: return "INGRESS_ROLE", balance
        if credit > U - balance: return "INGRESS_OVERFLOW", balance
    if balance < debit: return "INSUFFICIENT_BALANCE", balance
    if credit > U - (balance - debit): return "BALANCE_OVERFLOW", balance
    return None, balance - debit + credit


def mirrors(world):
    p = world.pool
    if p is None: return world.balances[:2] == (0, 0) and sum(world.shares) == 0
    return world.balances[:2] == (p.r0, p.r1) and sum(world.shares) == p.supply and world.shares[2] >= K


def step(world, cmd):
    def reject(reason): return Decision(reason, world)
    if type(world) is not World or type(cmd) is not Command: return reject("DOMAIN")
    if not all(bounded(v, U) for v in world.balances) or not all(bounded(v, N) for v in world.shares): return reject("DOMAIN")
    p = world.pool
    if p is not None and (not all(bounded(v, N) for v in (p.r0, p.r1, p.supply)) or not bounded(p.status, 2)): return reject("DOMAIN")
    if (not bounded(cmd.action, 4) or not bounded(cmd.direction, 1) or not bounded(cmd.actor, 5) or not bounded(cmd.recipient, 5)
            or not all(bounded(v, N) for v in (cmd.a, cmd.b, cmd.limit0, cmd.limit1)) or not bounded(cmd.sqrt, J) or cmd.a == 0):
        return reject("DOMAIN")
    action, a, b = cmd.action, cmd.a, cmd.b
    create, swap, add, remove = action == 0, action in (1, 2), action == 3, action == 4
    if (create or add) and (a > J or b == 0 or b > J or cmd.recipient != cmd.actor): return reject("DOMAIN")
    if add and (cmd.limit0 > J or cmd.limit1 > J): return reject("DOMAIN")
    if not (create or add) and b != 0: return reject("DOMAIN")
    if not create and cmd.sqrt != 0: return reject("DOMAIN")
    if not swap and cmd.direction != 0: return reject("DOMAIN")
    if create and (cmd.limit0 or cmd.limit1): return reject("DOMAIN")
    if swap and cmd.limit1 != 0: return reject("DOMAIN")
    if create and p is not None: return reject("POOL_EXISTS")
    if not create and p is None: return reject("NO_POOL")
    if p is not None:
        if p.status != 0: return reject("POOL_INACTIVE")
        if p.r0 == 0 or p.r1 == 0 or p.supply < K: return reject("EMPTY_POOL")
    if not mirrors(world): return reject("DOMAIN")
    if remove and cmd.actor == 3: return reject("RESERVED_LP_LOCK")
    if cmd.actor >= 2 or ((swap or remove) and cmd.recipient >= 2): return reject("RESERVED_ROLE")
    effects = []
    def transfer(asset, source, destination, quantity):
        if quantity: effects.append(Effect(300 + asset, source, destination, quantity))
    def lp(owner, quantity): effects.append(Effect(302, owner, owner, quantity))
    if create:
        if not cmd.sqrt ** 2 <= a * b < (cmd.sqrt + 1) ** 2: return reject("BAD_SQRT")
        if cmd.sqrt <= K: return reject("INITIAL_TOO_SMALL")
        post_pool = Pool(a, b, cmd.sqrt)
        transfer(0, cmd.actor, 2, a); transfer(1, cmd.actor, 2, b)
        lp(cmd.actor, cmd.sqrt - K); lp(3, K)
        outputs = (cmd.sqrt - K, K, 0, 0, 0, 0, 0, 0)
    elif swap:
        x, y = (p.r0, p.r1) if cmd.direction == 0 else (p.r1, p.r0)
        if action == 2:
            if a >= y: return reject("DRAIN")
            t = mc(x, a, y - a)
            gross = mc(t, 10000, 9970)
            if gross > N or x + gross > N: return reject("POST_DOMAIN")
        else:
            gross = a
            if x + gross > N: return reject("POST_DOMAIN")
        fee = mc(gross, 30, 10000)
        net, protocol = gross - fee, mf(fee, 2000, 10000)
        if action == 1 and net == 0: return reject("ZERO_NET")
        quote = mf(y, net, x + net)
        if action == 1:
            if quote == 0: return reject("ZERO_OUTPUT")
            if quote < cmd.limit0: return reject("SLIPPAGE")
            paid, gap = quote, 0
        else:
            if net == 0 or quote < a: return reject("INSUFFICIENT_QUOTE")
            if gross > cmd.limit0: return reject("SLIPPAGE")
            gap = mc(quote - a, 10000, a)
            if gap > 200: return reject("QUOTE_GAP")
            paid = a
        nx, ny = x + gross - protocol, y - paid
        post_pool = Pool(nx, ny, p.supply) if cmd.direction == 0 else Pool(ny, nx, p.supply)
        if not (0 < paid < y and nx * ny >= x * y): return reject("INVARIANT")
        transfer(cmd.direction, cmd.actor, 2, gross - protocol)
        transfer(cmd.direction, cmd.actor, 4 + cmd.direction, protocol)
        transfer(1 - cmd.direction, 2, cmd.recipient, paid)
        outputs = (gross, paid, fee, protocol, fee - protocol, net, quote if action == 2 else 0, gap)
    elif add:
        if a * p.r1 <= b * p.r0: u0, u1 = a, mf(a, p.r1, p.r0)
        else: u0, u1 = mf(b, p.r0, p.r1), b
        if u0 == 0 or u1 == 0: return reject("ZERO_USED")
        if u0 < cmd.limit0 or u1 < cmd.limit1: return reject("SLIPPAGE")
        mint = min(mf(u0, p.supply, p.r0), mf(u1, p.supply, p.r1))
        if mint == 0: return reject("ZERO_MINT")
        post_pool = Pool(p.r0 + u0, p.r1 + u1, p.supply + mint)
        transfer(0, cmd.actor, 2, u0); transfer(1, cmd.actor, 2, u1); lp(cmd.actor, mint)
        outputs = (u0, u1, mint, a - u0, b - u1, 0, 0, 0)
    else:
        if world.shares[cmd.actor] < a or a > p.supply - K: return reject("INSUFFICIENT_LP")
        o0, o1 = mf(a, p.r0, p.supply), mf(a, p.r1, p.supply)
        if o0 == 0 or o1 == 0: return reject("ZERO_PAYOUT")
        if o0 < cmd.limit0 or o1 < cmd.limit1: return reject("SLIPPAGE")
        post_pool = Pool(p.r0 - o0, p.r1 - o1, p.supply - a)
        lp(cmd.actor, -a); transfer(0, 2, cmd.recipient, o0); transfer(1, 2, cmd.recipient, o1)
        outputs = (o0, o1, 0, 0, 0, 0, 0, 0)
    if not all(bounded(v, N) for v in (post_pool.r0, post_pool.r1, post_pool.supply)): return reject("POST_DOMAIN")
    debits, credits, shares = [0] * 8, [0] * 8, list(world.shares)
    def key(account, asset):
        if account == 2: return asset
        if account >= 4: return 2 + asset
        return 4 + 2 * account + asset
    for effect in effects:
        if effect.channel == 302:
            shares[2 if effect.source == 3 else effect.source] += effect.amount
        else:
            asset = effect.channel - 300
            debits[key(effect.source, asset)] += effect.amount
            credits[key(effect.destination, asset)] += effect.amount
    balances = []
    for index, balance in enumerate(world.balances):
        expected = outputs[3] if swap and index == 2 + cmd.direction else 0
        reason, value = balance_step(balance, debits[index], credits[index], index in (2, 3), expected)
        if reason is not None: return reject(reason)
        balances.append(value)
    if any(sum(debits[i::2]) != sum(credits[i::2]) for i in (0, 1)): return reject("UNBALANCED_EFFECTS")
    if sum(shares) - sum(world.shares) != post_pool.supply - (0 if p is None else p.supply): return reject("UNBALANCED_EFFECTS")
    candidate = World(post_pool, tuple(balances), tuple(shares))
    if not mirrors(candidate): return reject("POOL_BALANCE_MISMATCH")
    if any(sum(world.balances[i::2]) != sum(candidate.balances[i::2]) for i in (0, 1)): return reject("UNBALANCED_EFFECTS")
    return Decision(None, candidate, tuple(effects), outputs)


def limbs(value):
    if not bounded(value,U): raise ValueError("u128 encoding refuses before narrowing")
    return tuple((value >> (32 * i)) & ((1 << 32) - 1) for i in range(4))


def state_values(world):
    p = world.pool
    header = (0, 0, 0, 0, 0) if p is None else (p.r0, p.r1, p.supply, 1, p.status)
    return header + world.shares + tuple(limb for value in world.balances for limb in limbs(value))


def command_values(command):
    return tuple(getattr(command, name) for name in Command.__dataclass_fields__)
