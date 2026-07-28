# Execution and sandbox boundary

ZenoFCIS separates semantic confinement from process containment. These are different assurance claims.

| Surface | Current guarantee | Explicit nonclaim |
|---|---|---|
| Pure semantic crates | No ambient I/O, clocks, randomness, threads, executable effects, or unsafe Rust in the reviewed source boundary | This does not isolate the library process, compiler, dependencies, memory, CPU, or microarchitecture |
| Generated pure cores and hooks | Type systems and source policies restrict what protocol-authoritative project logic may express | They do not sandbox the agent, compiler, build script, procedural macro, or analyzer |
| Retained-fixture workflow | Dedicated `/tmp` paths prevent source/output confusion and preserve a clean checkout | `/tmp`, an ephemeral runner, and read-only repository permissions are not hostile-process containment |
| Mounted runtimes and backend engines | Strict adapters validate bounded canonical outputs and complete decision artifacts | They do not restrict the external process's filesystem, network, environment, child processes, CPU, or memory |
| Solidity | The retained fixture compiles with exact `solc` and OpenZeppelin inputs; deployed execution is contained by the EVM | Host compilation does not execute contract behavior, bind bytecode to a deployment, or authorize production use |
| Solana | The retained Anchor workspace passes host `cargo check` with its retained lockfile; deployed execution is contained by the Solana VM | Host checking does not invoke Anchor CLI or `build-sbf`, produce SBF/ELF, execute instructions, verify a build, or bind a deployed program |
| Shells and effect interpreters | They may execute only validated closed plans | They hold real external authority and still require deployment-specific isolation, credentials, resource limits, and operational review |

## When process isolation is required

The pure core does not require a separate OS sandbox merely to preserve its functional semantics. It still requires bounded inputs and deterministic resource budgets.

Use an OS, container, VM, or equivalent sandbox when executing potentially untrusted:

- generated or agent-authored source;
- Cargo build scripts or procedural macros;
- compilers and analyzers;
- mounted runtimes;
- private synthesis, solver, prover, compiler, or LLM backends;
- migration tools;
- effect interpreters.

Apply least privilege, an isolated filesystem, no production credentials, network denial unless explicitly required, process limits, CPU and memory limits, and retained execution evidence.

## Assurance ladder

These gates establish different claims and must not be collapsed:

1. pure-core and source-policy admission;
2. deterministic regeneration;
3. host compilation;
4. chain-target compilation and reproducible or verified build;
5. behavioral, instruction, invariant, and adversarial testing;
6. deployed bytecode or program-data identity binding;
7. independent review and explicit production authorization.

Passing one stage does not imply any later stage.

## Chain virtual machines

EVM and Solana VM execution supply platform-specific containment and transaction semantics after deployment. That containment does not establish source correctness, economic correctness, exact token behavior, upgrade safety, oracle safety, MEV resistance, side-channel resistance, reproducible builds, or production authorization.

## Explicit nonclaims

ZenoFCIS does not currently provide a general hostile-code sandbox. Semantic purity, source scanning, strict output admission, dedicated build directories, ephemeral CI runners, and chain VM execution are separate controls with separate evidence.
