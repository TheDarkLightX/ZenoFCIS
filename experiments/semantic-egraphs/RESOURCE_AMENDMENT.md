# Solver-budget amendment after the initial full run

The initial full run used 6 iterations, a 5,000-node threshold checked between
batches, and a 2-second CBC budget. It failed the recorded success criterion:
12 of 200 LP extractions reached the solver time limit, all in the general
balanced family. Two were rewrite-only and ten were semantic. Nine timed-out
results could not be decoded into selected integer DAGs; three produced terms
that passed behavior checks but were rejected for non-optimal solver status.
All six negative controls passed. The initial complete result and log are
retained in `results/main-rust.json` and `results/main-run.log`.

Before the second full run, the CBC budget is increased to 30 seconds for both
egg variants. The corpus, seed, case inclusion, ordinary rules, candidate
seeds, node/iteration thresholds, objective, lowering, verification gate,
negative controls, and node-count success criterion remain the same. Every
case is rerun, including those that failed initially. No timeout incumbent is
accepted. This is a transparent resource revision after observing the initial
run, not evidence that the initial two-second protocol succeeded.

The second run is saved separately as `results/main-30s-rust.json`. Any paper
must report the initial failures and this amendment alongside its final result.
If the second run fails, further changes require a separately retained record;
missing or failed variants cannot be excluded to obtain success.
