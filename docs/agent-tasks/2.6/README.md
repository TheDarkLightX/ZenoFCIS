# ZenoFCIS 2.6 task packets

These are self-contained task specifications for coding agents working on ZenoFCIS 2.6. Each packet states:
- the user story;
- the verified facts about the current code, with file and line references;
- what to build;
- the planted-defect controls;
- the definition of done;
- what is out of scope.

Each packet also includes the shared repository rules. Give an agent one packet per task.

All packets start from `main` at `c951cd957a831326373c710efe6165ec0f6bb8a3` (release 2.4.0). They add new files
where they can, so that the later 2.5 release line can be merged underneath them.

| Packet | What it delivers | Tools needed | Evidence level |
|---|---|---|---|
| [P1: shell simulation](P1_SHELL_SIMULATION.md) | Seeded fault simulation for the SQLite store, an independent protocol model, real kill-and-reopen tests and planted-defect controls | Rust 1.97.1 | Tested |
| [P2: Lean induction](P2_LEAN_INDUCTION.md) | A Lean exporter for inductive claims in a linear integer fragment; two template claims reach kernel checked | Rust 1.97.1; Lean 4.30.0 (pinned hash) | Kernel checked, for the fragment |
| [P3: composed-domain proof](P3_COMPOSED_DOMAIN_PROOF.md) | A shared outcome resolver, its Verus proof, and the composed-domain induction theorem | Rust 1.97.1 and 1.98.1; pinned Verus | Proved, with named bridges |
| [P4: structure-search gate](P4_STRUCTURE_SEARCH_GATE.md) | A restricted Rust profile checker, a cost meter, gates G0-G3, two queue designs and their controls, and a Verus proof of the simple design | Rust 1.97.1 and 1.98.1; pinned Verus | Tested, finite checked and proved, per part |

The environment needs network access during setup, to install the Rust toolchains and download the pinned Lean and
Verus archives. Every download is checked against the repository's recorded hashes. Lean and Verus are pinned for
Linux x86-64.

Results come back as draft pull requests and are reviewed before any merge. These packets are task instructions,
not release documentation; they describe planned work, not shipped capability.

## Revision 2

Revision 1 (commit `a38a8ee`) had acceptance requirements that were impossible or tested the wrong condition. Revision
2 corrects them. An agent that started from revision 1 must apply these changes:

- **P1, shell simulation.**
  - The crash points return an error and roll back, so a child process cannot die inside the transaction. P1 now
    permits a test-only abort mode for the crash points, and an interleaving hook between `synchronize()` and
    `BEGIN`, both behind the `crash-injection` feature.
  - Freshness and acknowledgement checks are layered. Each mutation now needs a behavioural witness: mutate every
    layer together, or build a schedule that reaches only the mutated layer.
  - The stale-writer control now removes the whole compare-and-swap predicate, and uses the interleaving window.
  - Two vague mutations are replaced.
- **P2, Lean induction.**
  - The project definition has no genesis value, so `history_holds` now takes the base case as a hypothesis. A
    concrete genesis theorem is optional, and only with a bound genesis value.
  - The controls are now a table, each with a normal result and the result under its mutation. The
    domain-dependent fixture must be **checked** normally, and must fail after the domains are dropped.
  - The definedness control now uses a claim whose own arithmetic can overflow.
- **P3, composed-domain proof.**
  - The obligations now match the checker (`obligations.rs:1-42`): Reject, Refused and Skip are legitimate, and the
    delivered read-only state must satisfy its invariant. The theorem now includes that invariant.
  - State application is a second shared proved function, `apply`. The "commits only its own successor" control is
    a mutation of `apply`, not of the resolver.
- **P4, structure-search gate.**
  - `m.cmp` and `m.eq` accept only fixed-size scalar types.
  - Derived `Clone` and equality are refused, because they bypass the meter.
  - Exhaustion now has exact refinement semantics: `Exhausted` if and only if the spec cost exceeds the limit,
    with no new version.
  - Every gate now runs standalone on every plant, with an overlap matrix, so each gate's control is shown
    independently of the pipeline order.
