# Rounded fee contract

This development example computes a generic fee from authored rules. The fee
is `ceil(amount * rate / 10)` on amounts 0..=20 and rates 0..=3; the state
contains the resulting fee and net amount. The small denominator makes rounding
visible in a reduced domain. A basis-point policy uses the same fused operation
with denominator 10000 and its own declared ranges and qualification.

The rule computes the fee in the library evaluator. Independent declared laws
check rounding, conservation and an exact product bound. Generation, Authority
execution, full reduced-domain checking, app replay and emitter conformance
remain pending qualification on this development source. No external value
movement, deployment approval or wider-domain financial theorem is supplied.

```sh
zeno-fcis generate contract examples/arithmetic-fee
zeno-fcis contract export-program examples/arithmetic-fee --out fee.zcve
zeno-fcis transform check --original fee.zcve --candidate fee.zcve --receipt fee-receipt.json
zeno-fcis transform replay --original fee.zcve --candidate fee.zcve --receipt fee-receipt.json
zeno-fcis new fee-app --contract examples/arithmetic-fee --source .
```

The generated application's Cargo tests and documented `init`, `decide`,
`submit` and `history` commands exercise its bound Authority and SQLite
publication route. Use the generated README for their arguments.
The application build and those commands are additional checks; program
self-equivalence alone does not establish that the fee law is correct.
