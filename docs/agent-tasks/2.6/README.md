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
