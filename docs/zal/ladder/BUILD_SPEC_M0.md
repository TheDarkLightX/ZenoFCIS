# M0 ladder contract

Profile `zal/ladder/0` is experimental and has authority none. Statements declare
`steps-over-integers`, their domains, symbols, acyclic definitions, kind,
policy status and example tables. Missing or unsupported theories are refused.

The language supports universal, existential and exactly-one quantifiers,
Boolean connectives, integer comparisons, predicate calls, state-function
calls, integer arithmetic, floor, ceiling and finite counts. A prime reads the
state after the declared step. It means neither negation nor a derivative.
The M0 table evaluator requires positive divisors and mathematical integers;
its results do not discharge checked native arithmetic obligations.

Templates and vocabulary are data. Precise English renders each constructor.
Intuitive adds exact registered shapes, including restricted quantification,
cardinality and state-expression nondecrease. Plain can replace an expression
with an acyclic, capture-avoiding definition. Definitions add names, never
arithmetic identities. No statistical language model supplies a reading.

The complete Earley grammar retains all derivations. Acceptance requires
exactly one distinct expanded normalized tree. Multiple derivations of the
same tree are harmless. Ambiguity and exhausted budgets refuse with a named
reason; input is never truncated or accepted from its first parse.

Normalization expands definitions and exactly-one, renames bound variables,
flattens and sorts conjunction/disjunction, orients comparisons, and removes
double negation. It does not prove arithmetic equivalence. Mixed conjunction
and disjunction, nested implication and compound count bodies retain grouping.
Material implication is true when its premise is false. Empty universal
domains are vacuous; empty existential domains have no witness.

The legend begins with the theory, then covers exactly the used notation and
declared names in first-appearance order. Questions preserve polarity and show
a separate search task. Finding no witness is UNKNOWN unless the declared
finite domain was exhausted or suitable proof evidence was checked.

Input limits remain 2,000 characters and 400 tokens. Tree limits are 1,024
nodes and depth 80; complete parsing additionally has a deterministic 250,000
item budget. Parser scratch is local to one invocation. Context data and
returned tuple trees are transitively immutable.

Five [sample statements](../../../integrations/zal/examples/ladders) cover
reserve-product preservation, an existential search, per-asset free gain,
fee rounding and active-record cardinality. Their policy statuses and table
scope remain visible. A stipulated table establishes neither execution
membership, reachability, authenticated identity nor native refinement.
