# Numeric-domain checking survey

- Existing symbolic equivalence encodes admitted compiled programs, not source
  expressions. Reuse its SMT integer/Boolean terms, checked printer and queries.
- Scalar execution is eager: unused nodes and unselected arms can fail.
  Arithmetic and output-domain failures are distinct and must remain so.
- Existing contract symbolic checking binds the Authority descriptor, resolves
  branch successors and replays models natively. It covers state preservation,
  not all command/context/decision laws, and currently refuses unsigned subjects.
- Its verdict policy allows cvc5 unsat without Z3 corroboration and replayed sat
  despite disagreement. Keep that established API separate; new law checking
  requires matching conclusive answers from both solvers.
- SMT Int can express exact wide products and fused division. Checked scalar
  multiplication still requires a width guard; products_ge has no truncation.
- Numeric compiled graphs and typed law programs already distinguish signed,
  unsigned and Boolean values. Reuse those data and native evaluator semantics.
- Full unsigned inputs need lossless model transport: two bounded 64-bit limbs
  avoid narrowing the solver runner's signed model-value representation.
- Tau export has a Z3 comparison half over compiled graphs and independent rule
  expressions, but restricts arithmetic ranges and unsigned subjects. Reuse its
  semantic lessons, not its restricted export admission as a general law checker.
- Transform symbolic checks partition equivalence obligations and replay models;
  their receipts do not replace exhaustive adoption receipts. Representation
  refinement currently enumerates concrete/abstract pairs and remains separate.
- Solvers::open admits manifest-pinned binaries; solve runs both with the
  formal-tools timeout/output limits and can retain queries and outputs.
- Qualification tests read ZENO_FCIS_Z3 and ZENO_FCIS_CVC5, check binary identities
  and generate manifests. Both pinned executables are runnable for this work:
  Z3 4.16.0 and cvc5 1.3.3.
- Review currently chooses a full domain or advisory boundary probes; the lemma
  judge needs enumeration. Preserve the exhaustive route below its existing cap.
- The app factory invokes the UI oracle before scaffold emission. Numeric native
  applications already have a separate scaffold, but the durable/UI bridge needs
  coordination. UI presentation checking is a distinct bounded claim.
- The supplied pool uses exact fused division, fee 997/1000 and minimum output;
  its full eager graph can refuse independently of its selected decision case.
- New symbolic evidence is solver-checked (z3 and cvc5 agree), never exhaustive
  or kernel-checked. Unsupported operations, disagreement and missing answers
  must leave UNKNOWN. Native Authority remains the only runtime decision path.
