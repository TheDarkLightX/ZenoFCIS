# Independent native-benchmark review

Date: 2026-10-04. Reviewer: GPT-6 Astra, Max reasoning, in an independent
read-only agent. Root implemented the experiment. Review was limited to the
offline benchmark, runtime/admission semantics, descriptor and recorded evidence;
the reviewer did not run builds, edit source or review the broader V2 release.

Result: **no decisive blockers** in the stated offline scalar result.

The reviewer confirmed the prior-operand and external-user guards, ordered-root
remapping, typed Boolean OR replacement, charge-before-instruction semantics,
position-preserving padding and explicit resource-refinement nonclaim. It
independently calculated the 13-domain product as 1,296,000 and inspected inverse
ordinals, unique visitation, final coverage and direct-oracle comparisons.

The first source audit checked 168 manifest entries, all artifacts and successful
command logs. A focused follow-up checked the final dedicated lockfile, offline
locked builds, all 180 entries in run 02, both profiles' artifact hashes and
matching results. The only packaging catch was the globally ignored Cargo.lock;
the benchmark-local ignore exception now retains it. All nine registry pins
match the original workspace lockfile.

Final reviewed SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `src/main.rs` | `2bf04938510a5928b63c744145d16fac47a4f426427b95c200914ae3c697d4bc` |
| `run.py` | `a8606abf346c0b53ee024bc38b28826b7d7410d9b8e261424d7df7a24f4a4c79` |
| `Cargo.lock` | `3a7a22aa7fb03d534bd122968bd8ca23beb4ce87f236b98c1cd4aff308b8b135` |
| `README.md` | `de01297211852cf7438bcf87bf8415ad8c0e7cb3c907f96972e73392435cd14f` |
| [Native evidence](../../evidence/v2_1_benchmarks/withdrawal-native-20261004.json) | `34f74d157e4ad7350865723e65b1937325a4ec873d97f0304a531a7018e0967e` |

The native evidence's four recorded `cargo` paths were redacted on 2026-10-04 to
`~/.cargo/bin/cargo` for the release privacy check; the hash above is of the
redacted file, and no other byte changed.

This is trusted local execution, not hermetic attestation: the runner inherits its
environment and does not bind compiler/binary executable hashes in the receipt.
The current graph's release run does not enumerate every budget on every input.
Its complete scalar product and tested budget scopes are stated explicitly.
Serialized IR size and logical Steps establish neither physical speed nor
whole-application equivalence. No formal proof was replayed. Review acceptance
does not make the dirty V2 integration ready to publish.
