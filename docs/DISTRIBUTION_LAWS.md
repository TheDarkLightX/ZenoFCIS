# Finite distribution laws

## Journey

Use `zeno-fcis contract distribution APPLICATION` to check a declarative
contract's output probabilities. Exit 0 means exactly checked over the declared
finite domain, given uniform independent random bits. Exit 1 reports a complete
counterexample; exit 2 is `UNKNOWN`. The packet is deterministic and advisory.
It grants no runtime or publication authority.

The [examples](../examples/distribution-laws/README.md) show reporter selection,
modulo bias and a random payout tie-break. Their directories contain the ordinary
`project.zeno` and `v2/policy.json` files consumed by the contract compiler.

```text
random 600 entropy on context.102.130 bits 2 independent uniform;
distribution 601 reporters on post.100.110 = uniform over {0, 1, 2, 3};
```

Stable IDs and names follow the existing law conventions. An explicit target is
`distribution 601 reporters on post.100.110 = {0: 1/2, 1: 1/2};`.
The outcome `committed-failure` declares the mass of committed failures, as in
`{0: 1/4, 1: 1/4, 2: 1/4, committed-failure: 1/4}` or
`uniform over {0, 1, 2, committed-failure}`.
Targets must have distinct outcomes, nonnegative rational weights and
total mass exactly one. Zero-mass entries are removed. Rational components and
intermediate reduced sums must fit 64 bits. No floating point is used.

## Formal model

Fix every non-random input to a tuple x. For random input widths b1 through bm,
the declared random domain is the Cartesian product R, of size N = 2^(sum bi).
The external premise is that these bits are uniform and mutually independent,
and independent of x. For each declared outcome projection f, the outcome
o_f(d) of a decision d is f of its successor state when d accepts, and the
single outcome `committed-failure` when d is a committed failure, whatever
successor value it writes. A reject has no outcome. Compute
P_x(y) = sum over r in R of [o_f(Authority(x,r)) = y] / N.
Compare this rational distribution exactly with the declared target for every x.
A target that omits `committed-failure` is therefore refuted whenever some
random value commits a failure at x.

Each tuple is evaluated once through the ordinary bound library Authority,
including its invocation laws and resource limits. A single histogram fold per
projection adds exact weight 1/N and retains its random preimages. Other weight
algebras may later reuse this fold shape; this version has no semiring API.
There is no pooling across fixed inputs, sampling, or conditioning on success.

An excess outcome of target weight p requires floor(N*p)+1 preimages to prove
excess. The packet selects the excess outcome with the smallest such witness,
then the least outcome and lexicographically first random assignments. No
smaller positive set of preimages can exceed its target mass. Expected and
actual distributions and the fixed input tuple accompany the witness. The
packet's `decision_mass` gives the exact mass of Accept, Reject and
CommittedFailure decisions at the refuted tuple; Reject is always zero there.
For a checked result it is the mass at every tuple, since each target fixes
the `committed-failure` mass, zero when omitted.

## Difference

Invocation laws constrain each decision. A distribution law constrains the
mass of outputs across all random values at each fixed input. Random inputs and
distribution laws are inert declarations until this separate checker runs;
they do not add entropy acquisition or a runtime invocation law family.
Existing projects retain their canonical bytes. Projects with these declarations
use project-spec encoding version 4, which commits to the premise, widths,
projections and normalized targets, each outcome tagged as a value or
`committed-failure`. Version 4 takes precedence over the version 2 and 3
markers; it keeps the version 2 quantity flag, and every type, including an
unsigned one, remains fully encoded. Packets bind both source files and the
source identities of the checker, compiler and evaluator.

## Scope

Random inputs are scalar command or context fields. An integer bit input must
have precisely domain 0 through 2^b-1, including the zero-bit singleton for
b=0; a Boolean requires b=1. Outcome projections currently name a scalar field
of the successor state, `post.100.FIELD`; integer, Boolean and payload-free
variant values use the contract's numeric scalar representation. Accept and
committed-failure outcomes are counted, and a Reject is UNKNOWN. Targets are
fixed finite sets, independent of the non-random tuple. Multiple projections
check their marginal distributions, not their joint distribution.

The hard tuple cap is 1,048,576 and the hard work cap is 16,777,216; caller
limits can lower them with `--max-tuples` and `--max-work`. One invocation and
each projection/fold charge one work unit. This counts logical checker work,
not CPU time; the Authority separately enforces its invocation resource limits.
A budget stop, Authority refusal or absent successor projection is `UNKNOWN`.
Partial work remains in the packet. No incomplete result carries a checked claim.

The checker does not verify the entropy source or prove unbounded correctness.
Boundary tests include one possible reporter with zero bits. Cross-step
reject-and-retry protocols, counting algebras and min/max-plus are future work.
