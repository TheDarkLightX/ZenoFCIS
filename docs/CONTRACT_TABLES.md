# Contract tables

## Journey

The ask: "members borrow and return copies of a book, a member holds at most
two copies, and no copy is lent twice." With six copies, the state needs one
holder per copy, and every law is about each copy, or about how many copies
one member holds. Written field by field, that is six fields and laws that
repeat themselves six times. A table says it once:

```text
type 106 index CopyNumber of 110;
table 110 100 copies 105 size 6;
field 121 101 copy 106;
law 500 at_most_two_per_member on commit, genesis =
  forall i in table 110 where post.100.110[i] != 0 {
    count j in table 110 { post.100.110[j] == post.100.110[i] } <= 2
  };
law 502 other_copies_unchanged on commit =
  forall i in table 110 where i != command.101.121 { post.100.110[i] == pre.100.110[i] };
```

The [library loans example](../examples/contract-tables/library-loans/README.md)
is this contract in full. To see exactly what is checked, print the expansion:

```text
zeno-fcis check examples/contract-tables/library-loans/project.zeno --expand
```

It prints the project with every table form replaced by the declarations and
formulas it stands for, and reports the size of the input domain on stderr:
10 inputs and 294,912 input tuples for the example. `--format json` puts the
same text, the tables and every input's number of values in one document.
The command parses the printed text again and prints nothing unless it gives
the same canonical project as the original.

Everything else is unchanged. `check`, `generate contract`, `contract review`,
`contract lemma`, the other judges and `new --contract` read the table source
and see its expansion. The policy can use table forms, expanded against the project declarations
when it loads; `ui.json` still names entry fields by ID.
`--lemma` accepts one table-aware formula against the project declarations,
including braces, newlines and comments. It consumes the entire text and
refuses trailing declarations. A table-form lemma and its printed expansion
have the same verdict.

## Formal model

Expansion happens while the project is parsed, before elaboration, so a
project with tables and its printed expansion have the same canonical bytes,
the same identity and the same checks. The rules are:

- `table ID OWNER name TYPE size N;` declares, for each entry k from 0 to N-1,
  the field `ID + k` of record OWNER, named `name_k`, of type TYPE. N is 1 to
  64. Entry IDs follow from the table's ID alone, not from the order of the
  declarations. The IDs `ID` to `ID + N - 1` must be free: a table is refused
  when any of them is the ID of another declaration of any kind, or of the
  project.
- `type ID index Name of T;` is `type ID int Name in 0..=N-1;` for a table T
  of N entries.
- `ROOT.OWNER.T[k]` is `ROOT.OWNER.(T + k)` for a literal or bound index k.
  Computed integer positions use the checked Select expansion described below.
- `forall i in table T { F }` is `F_0 && F_1 && ... && F_N-1`, joined left to
  right, where F_k is F with i replaced by the integer k. With `where C`, each
  entry is `(C_k -> F_k)`.
- `exists i in table T { F }` is `F_0 || ... || F_N-1`; with `where C`, each
  entry is `(C_k && F_k)`.
- Parenthesized scalar expressions retain arithmetic precedence in comparisons.
  A table sum or value count can be multiplied on either side of a factor.
- `choose(C, A, B)` is a scalar Select value: evaluate C, A and B eagerly,
  then select A when C is true and B otherwise. Both arms use the existing
  scalar-kind checks. An overflow, bad cast or division trap in the unselected
  arm still refuses evaluation. It lowers to the existing library Select:
  the condition, both arms and Select incur their ordinary node/work charges,
  including work measured before a refusal; there is no branch work discount.
- `sum i in table T { V }` is `V_0 + ... + V_N-1`, joined left to right.
  `where C` keeps static index/constant filtering compatible with earlier
  sources. When C reads an input, each entry becomes `choose(C_k, V_k, 0)`;
  V_k is eager even when C_k is false.
- A count in a value position, including one compared with a state or command
  value, is `sum i in table T { choose(C, 1, 0) }`. It has N Select terms
  and N-1 additions, with one occurrence of the condition per entry. For example:
  `count j in table 110 { post.100.110[j] == command.101.122 } <= pre.100.120`.
  Counts can also appear on the right of a comparison or inside arithmetic.
- For compatibility, an isolated `count i in table T { C } OP K`, with OP one of `<=`, `<`, `>=`, `>`, `==`,
  `!=` and K an integer constant, compares the number of entries for which C
  holds with K. `count <= K` is the conjunction, over every set of K+1 entries
  in ascending order, of `!(C_a && C_b && ...)`. `count >= K` is the
  disjunction, over every set of K entries, of `(C_a && C_b && ...)`.
  `< K` is `<= K-1`, `> K` is `>= K+1`, `== K` is `>= K && <= K`, and `!= K`
  is its negation. Each clause list is joined in a balanced tree, left half
  first. A count beyond 0..N is `true` or `false` as the comparison requires,
  and a count whose conditions read only constants is evaluated.

So the example's law 500 is six implications, one per copy k: when field
110+k is not 0, none of the 20 sets of three copies all have copy k's holder.

Errors point at the table source: at the `table` declaration, the `[` of an
entry, or the start of a table form. A diagnostic inside a form's body is
reported once, not once per entry. A table of more than 64 entries, or an
expansion that would exceed the formula-node limit of 1,000,000, stops with
the resource-limit diagnostic `ZENO-E0207` at the form; for a `count`, the
diagnostic gives the number of clauses it would need.

### What a count costs

`count i in table T { C } <= K` is C(N, K+1) clauses of K+1 copies of C,
over a table of N entries; a count inside a `forall` is expanded once per
entry of the `forall`. `check --expand` prints each count's clauses and how
many times it is expanded, so the cost is visible before it reaches the limit:

| N | K | Clauses for `<= K` |
|---|---|---|
| 6 | 1 | 15 |
| 6 | 2 | 20 |
| 16 | 2 | 560 |
| 32 | 2 | 4,960 |
| 64 | 1 | 2,016 |
| 64 | 2 | 41,664 |
| 64 | 3 | 635,376 |

`>= K` costs C(N, K) clauses of K copies, and `==` and `!=` cost both.
These constant-bound forms retain their existing expansion and identities.
For a linear constant-bound form, write
`sum i in table T { choose(C, 1, 0) } <= K` explicitly. At N = 6, 16 and
64 it has 6, 16 and 64 Select terms instead of 20, 560 and 41,664 clauses
for K = 2. A nonconstant-bound count uses this linear form automatically.
`check --expand` distinguishes Select terms from combinatorial clauses.

### Computed positions in laws

`ROOT.OWNER.T[e]`, `constant_name[e]` and completed derived entries accept
integer expressions. For example, after a tier field has the declared range
0..=3, `fee_bps[command.101.120 + 0]` reads the corresponding policy constant.
The compiler checks an interval for e and refuses unless it is contained in
0..N-1. Numeric input types and fields must be declared before this use.
Ranges come from the project declarations, not a rule condition or example.

Checked addition, subtraction, multiplication, floor/ceiling division, numeric
casts and `choose` propagate conservative bounds. Both Select arms contribute
to the range. Unknown bounds, incompatible kinds, possible bound overflow,
division through zero or a range outside the table produce a source diagnostic.
Correlations between repeated inputs are not assumed; rewrite the expression
or narrow its declared type when the interval cannot establish containment.

A literal or substituted bound index retains its direct entry. Other reads
lower to `choose(e == 0, T[0], choose(e == 1, T[1], ... T[N-1]))`.
Unsigned positions compare with unsigned constants. The last arm is safe
because the independent range check has already excluded every other index;
there is no out-of-range value. Every entry and condition is eager, retaining
the existing Select's traps and ordinary measured work. Boolean reads use the
same eager Select in a checked Boolean circuit. No runtime table is retained.
The selection has N entries and N-1 Selects for a one-dimensional table with
N >= 2. A computed position on a singleton keeps one same-entry Select, so
evaluating the index still incurs its traps and work. Literal/bound reads keep
their direct entry.
Resource limits still apply to the entries and copied index expressions.
Active recursive self references retain literal or bound indices. The
[fee-tier journey](../examples/contract-tables/fee-tiers/README.md) uses one
constant schedule in both its law and decision rule.

## Recursive tables

The ask: "a member may delegate a vote, but delegation must never form a
cycle." A derived table computes a Boolean relation from the declared inputs;
it adds no stored state. For three members, where stored target 0 means no
delegate and target j+1 names member j:

```text
derived table 700 reach[i][j] in table 110 rounds 9 {
  when true then post.100.110[i] == j + 1 ||
    exists k in table 110 { reach[i][k] && reach[k][j] };
  default false;
};
law 500 no_cycles on commit, genesis =
  forall i in table 110 { !reach[i][i] };
```

The [delegation example](../examples/contract-tables/delegation/README.md)
also computes the prospective relation from the state and command, so the
decision refuses a cycle before committing. The
[role inheritance example](../examples/contract-tables/role-inheritance/README.md)
propagates a permission through parents, including cycles with no grant.

### Declaration and safe fragment

`derived table ID name[i] in table T rounds M { ROWS default V; };` declares
one Boolean entry per index of T. Use `name[i][j]` for a relation over pairs
of that same table's indices. ID must be unused. Active self-reference indices
are literals or bound table indices; completed entries also admit checked
computed positions. M is explicit and must equal the number of Boolean
entries: N for one index, N*N for a pair.

Each row is `when G then V;`. The first true guard selects its value; when
none holds, the explicit default supplies the value.

- Guards read scalar state, command, context, stored tables and earlier
  derived declarations. They may negate this lower data. They cannot read
  the table being defined, including inside a quantifier's `where` guard.
- Values and the default combine positive self entries with `&&`, `||`,
  bounded `exists` and `forall` over fixed tables, constants and lower data.
  Lower data may be negated. Self negation, comparisons and counts are
  refused, even when a particular expression would happen to be monotone.
- A derived declaration is one stratum. A reference to a later declaration,
  mutual recursion, an unbounded quantifier or an invalid index is refused
  with a reason at the source. Laws are a higher stratum and may negate or
  count the completed results.

### Meaning, work and expansion

Every entry starts false. A round applies the selected rule to every entry
using only the preceding round. For defined lower data, positivity makes
this map monotone. With M
Boolean entries, every strict increase adds a true entry, so its Mth iterate
is the least fixed point. The compiler unrolls this finite bound and shares
round results in an acyclic Boolean circuit. It never substitutes recursive
formulas into an exponentially growing tree. For an expanded per-entry rule
of size B, at most O(M*M*B) circuit operations are needed; existing bounded
quantifier and count expansion costs remain subject to their own limits.

`check --expand` prints the circuit and checks canonical roundtrip equality.
It uses backward node references, ordinary formula leaves and Boolean
operations. The contract compiler lowers them into the existing scalar and
law graphs; the existing Step and Read meters charge their work. Parser
limits and runtime budgets remain independent checks. The parser folds
constant results when it can evaluate their lower formulas exactly. Undefined
lower expressions keep the existing refusal semantics. Evaluation retains no
mutable table or cache between requests.

Derived names keep the observation roots written in their rules. A state
lemma or a genesis law uses a definition over `post.`; a transition lemma
may also read `pre.`, command and context. The role example includes both
views explicitly. Probes can read a definition over `pre.` on every decision;
a `post.` query is undefined on rejection, as for ordinary post-state fields.
Decision expressions can consume the Boolean circuit printed by expansion.

These checks are exact over the declared finite domains and within the
existing formula, source, tuple and work limits. Larger domains remain
inconclusive at their check limits. Derived tables have one or two indices over one fixed-size table; they
are not keyed collections.

### Constant policy tables

A hierarchy fixed by the policy can be declared without stored state:

```text
const table 110 parents 105 size 3 [0,1,2];
```

The scalar type 105 must be declared earlier. Entries are Bool, int or u128
literals within that type's bounds; the size is 1 through 64. The table
adds an index set, not fields or input-domain dimensions. Read `parents[i]`
with a bound table index, in-range literal or checked computed position.
IDs and names must be distinct.
Constant and derived declarations are macros: the expanded project contains
their effective scalar formulas; packets also bind the original source hash.

A closed recursive stratum whose lower formulas are statically known is
computed once while parsing the policy. Its owned final lookup becomes
literals in later strata, so the existing compiled graph identity binds the
effective values. A stratum that reads state, command or context retains its
metered shared graph. No result from an invocation is retained in a cache.
The constant pass preserves eager definedness and leaves unrecognized lower
arithmetic in the existing evaluator.

The [constant-hierarchy journey](../examples/contract-tables/constant-role-hierarchy/README.md)
keeps only three grant flags as state: eight states and 24 input tuples.
Comparison with the stored-hierarchy version projects away the fixed parent
fields and checks class, reason, grant successor and empty outbox. Stored
schemas and policy identities intentionally change under that projection.

### Permission sets as finite Boolean algebras

`type 106 bits Permissions width 3;` declares the subsets of three named-by-bit
atoms, represented by unsigned values 0 through 7. Widths 1 through 128 are
supported. For a word-valued definition, add `bits K` before `rounds`:

```text
derived table 700 permissions[i] in table 110 bits 3 rounds 6 {
  when true then join(pre.100.210[i], join j in table 110
    where pre.100.110[i] == j { permissions[j] });
  default u128(0);
};
```

Here table 110 has two entries. `join(A,B)` is union, `meet(A,B)` is
intersection, and bounded `join j in table T { B }` / `meet j in table T { B }`
join or meet the entries, with optional lower-data `where` guards. Values
are in-range unsigned literals, bounded unsigned projections, earlier
word-valued entries, or these algebra operations. `complement(A)` is permitted
on lower data; self complement is refused before simplification. Widths and
indices must match. Guards still cannot read the entry being defined.

Laws use explicit-width helpers `join(K,A,B)`, `meet(K,A,B)`,
`complement(K,A)` and `subset(K,A,B)`. Inner operations inherit the helper's
width. Equality uses `==` on the resulting unsigned values. Numeric `<=`
keeps its numeric meaning and does not test set inclusion.

The compiler projects a word to K Boolean atoms and uses the same shared
round compiler. All words start at zero; M entries require exactly M*K
declared rounds. Every strict increase adds an atom, so that bound reaches
the least fixed point. Packing uses checked unsigned arithmetic and eager
Select operations; width 128 includes the top atom and the full unsigned mask.
No second runtime evaluator is introduced. Existing expansion and metering
limits still apply; this does not promise that every table/width combination
fits those limits. Larger or multiword sets are outside this implementation.

The [permission-word example](../examples/contract-tables/permission-words/README.md)
keeps read, edit and publish distinct through inheritance. Its JSON decision
rule contains the printed circuit expansion, retaining the existing table-rule
parser's separate index, depth and work checks.

The semantics follow van Emden and Kowalski (1976), *The Semantics of Predicate
Logic as a Programming Language*, Journal of the ACM 23(4), 733–742. The
stratification follows Apt, Blair and Walker (1988), *Towards a Theory of
Declarative Knowledge*, in *Foundations of Deductive Databases and Logic
Programming*, 89–148.

## Difference

Before tables, a contract with six copies spelled out six fields and repeated
each law per copy; "at most two per member" had to be written as a condition
on every triple of copies. Tables change only how the project is written.
Existing sources retain their expansion and canonical bytes. Select adds a
new scalar encoding tag, leaving earlier tags intact; projects that use it
have the identity of that expanded formula. It uses the existing library
Select evaluator. Finite results remain scoped to the declared domain.

## Background

Alfred V. Aho, Ravi Sethi and Jeffrey D. Ullman, *Compilers: Principles,
Techniques, and Tools*, Addison-Wesley, 1986, describes syntax-directed
translation and intermediate representations. Ronald L. Graham, Donald E.
Knuth and Oren Patashnik, *Concrete Mathematics: A Foundation for Computer
Science*, second edition, Addison-Wesley, 1994, covers finite sums and
binomial coefficients. These are the background for finite expansion and
indicator sums; they do not qualify an implementation of this feature.

Maarten H. van Emden and Robert A. Kowalski, “The Semantics of Predicate
Logic as a Programming Language,” Journal of the ACM 23(4), 1976, studies
fixed-point semantics for logic programs. Krzysztof R. Apt, Howard A. Blair
and Adrian Walker, “Towards a Theory of Declarative Knowledge,” in
*Foundations of Deductive Databases and Logic Programming*, Morgan Kaufmann,
1988, covers stratified logic programs. These are background for the bounded
positive rules; the compiler, meters and finite tests qualify this implementation.

## What is new here

This release adds table syntax, standalone table-form formulas and
project-aware policy preparation to the existing checked scalar language.
Recursive Boolean/word tables and constant-policy preparation join those additions.
The comparative assessment of that combination is **prior-art search pending**.
Ordinary table forms keep the existing scalar formula encoding. Recursive
tables add a shared circuit formula, which lowers to existing metered graph
operations. Results retain their scope: exact checks over the declared
finite domains, with the existing resource limits.

## Scope

- A table is a fixed number of fields. Its entries count toward the 64
  decision inputs a contract may have, and every check still enumerates the
  whole expanded domain. The example's 294,912 tuples are within the default
  cap of 1,048,576 input tuples of the finite checks. Beyond the cap,
  `contract review` reports bounded probes instead of the whole domain and
  the judges are inconclusive, as for any contract.
- An isolated constant-bound `count` retains C(N, K+1) clauses for `<= K`.
  Use the explicit Select sum for a linear constant-bound formula.
- Computed law positions require a declared range contained in the table.
  Range inference is conservative and does not use surrounding law guards.
- Policy table forms lower into the existing scalar rules and identity
  format. `ui.json` names entry IDs, and the interface shows each entry as a field.
- Linear formulas reduce expansion size, not the finite input domain.
  Checking large domains without enumeration remains separate work.

## Table forms in policy rules

Project-aware policy loading expands these forms in expression strings:

- `pre.100.110[command.101.121]` reads the entry named by the input index.
  A literal or bound index selects that field directly. A checked computed
  position, such as `pre.100.110[3 - command.101.121]`, uses the same eager
  Select expansion and declared-range check as a law.
- Named constant and completed derived reads use their project names:
  `fee_bps[command.101.120]` or `allowed[command.101.120]`. Boolean results
  can be rule conditions; numeric results can be scalar rule values.
- The post key `"110[command.101.121]": "member"` writes only the selected
  entry. Each expanded field is `choose(index == k, member, pre.100.FIELD)`.
  Overlapping table/scalar writes are refused. A literal-index write emits
  only that field. Writes name state tables; reads may also name command or
  context tables.
- `forall i in table T [where C] { F }` joins the substituted conditions with
  `&&`, using `C -> F` when filtered. `exists` uses `||` and `C && F`.
  `sum` joins values with `+`; its filter is eager `choose(C, V, 0)`.
  Variable names must be fresh, and nested forms preserve their bindings.

Read positions admit the checked integer expressions described above, using
bounded project projections rather than untyped policy aliases. Indexed post
keys retain literal, bound-variable or direct-input indices.
Expansion is preparation: it owns local scratch and returns owned scalar
expressions, with no new runtime state, indexing or decision authority. The
existing native evaluator supplies traps and measured work. Expansion has a
65,536 work cap per expression, including copied identifier bytes, and a
1 MiB text budget. Shared indexed-read parsing additionally charges the project
source and resulting scalar text against that work cap. This path cannot
bypass the policy parser's index depth, table-form or work checks. Existing scalar policy
loads retain their original path and limits.

The [state-limit library example](../examples/contract-tables/library-limits/README.md)
combines a state bound, a conditional sum and policy table forms. Its finite
domain is twice the fixed-limit example's; linear syntax does not shrink it.

## Background

Alfred V. Aho, Ravi Sethi and Jeffrey D. Ullman, *Compilers: Principles,
Techniques, and Tools*, Addison-Wesley, 1986, describes compiler translation
and intermediate representations. The table forms here lower to the existing
scalar and graph operations rather than introducing a runtime table evaluator.
Patrick Cousot and Radhia Cousot, *Abstract Interpretation: A Unified Lattice
Model for Static Analysis of Programs by Construction or Approximation of
Fixpoints*, Proceedings of the 4th ACM SIGACT-SIGPLAN Symposium on Principles
of Programming Languages, 1977, supplies the abstract-interpretation framework.
The index analysis here uses conservative integer intervals. Recursive-model
references are given above; these background citations are not a claim that
the sources establish this implementation's behavior or work accounting.

## What is new here

prior-art search pending. No priority claim is made for the combination of
bounded table syntax, eager scalar lowering and exact logical work accounting.
