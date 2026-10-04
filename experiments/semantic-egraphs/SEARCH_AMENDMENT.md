# Smaller-search amendment after the 30-second run

The 30-second run retained the original six-iteration, 5,000-node-threshold
search. It still failed the tool gate: semantic extraction of
`balanced_general-16` reached the solver time limit. Its decoded 22-node
incumbent passed behavior checks against the 30-node local baseline but remains
a failed result. The full run and independent failure audit are retained.

Before the third full run, both egg variants are limited to three iterations
and a 1,000-node threshold checked between batches. The CBC limit stays at 30
seconds. This tests whether smaller search snapshots remain useful while making
integer extraction reliable. No source, rule, seed, corpus, case, objective,
lowering, verification requirement, negative control, or node-count success
threshold changes. Every one of the 100 cases is rerun.

The third result is saved separately as `results/bounded-rust.json`. The first
two configurations failed; a successful third result cannot retroactively
make either earlier protocol successful. The study is an exploratory case
study with retained resource revisions, not a confirmatory benchmark under a
single untouched resource configuration.
