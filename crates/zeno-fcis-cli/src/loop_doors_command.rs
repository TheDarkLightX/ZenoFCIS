//! `zeno-fcis loop` doors: expressions, sketches, families and transfer.
//!
//! The doors let a proposer submit more than a whole canonical program. The
//! untrusted work (parsing, lowering, inverse semantics, hole solving,
//! synthesis, fitting) is the `zeno-fcis-loop-doors` crate's; this module
//! owns the files, the session, the contained stages and the verdicts. Every
//! acceptance comes from the existing checkers; regions, repair sites,
//! "unrealizable" and "not found" are the crate's untrusted findings:
//!
//! - `loop candidate --expressions` and `loop sketch` spend one session
//!   attempt each, exactly like `--candidate-json`, and the session's own
//!   check judges the lowered or completed program;
//! - every program this module reports as accepted, completed or "checked
//!   repair" also passed an in-process `transform check` whose receipt was
//!   replayed byte for byte (`transform replay`), in a contained check stage;
//!   for a session door that check runs before the attempt, and if it does
//!   not complete the attempt is not spent;
//! - `loop family` and `loop transfer` judge each instance the same way and
//!   keep no session.
//!
//! Results carry `authority: none`. Input files are bounded to 64 KiB, and
//! every command runs in the loop's contained process with its stage
//! deadlines; a stage that cannot start in time is reported as inconclusive.
//!
//! This module is LLM-generated, like the crate it drives.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::{Map, Value, json};
use zeno_fcis_loop_doors::expr::{
    Allow, Expr, Fill, Kind, Vocabulary, has_holes, params_of, parse, show,
};
use zeno_fcis_loop_doors::family::{self, Direct, Form, TransferStatus};
use zeno_fcis_loop_doors::inverse::Path as ExprPath;
use zeno_fcis_loop_doors::lower::lower_outputs;
use zeno_fcis_loop_doors::repair::{self, Status as RepairStatus};
use zeno_fcis_loop_doors::sketch::{self, SketchResult, Status as SketchStatus};
use zeno_fcis_loop_doors::subject::{DEFAULT_MAX_TUPLES, Subject, tuples};
use zeno_fcis_synthesis::finite::Program;
use zeno_fcis_synthesis::finite_runtime::import_program;

use super::{CHECK_DEADLINE, SEARCH_DEADLINE};
use crate::loop_proposers::{Proposed, Proposer};
use crate::neural_loop::acquisition::{Acquisition, Mode as AcquisitionMode};
use crate::neural_loop::program_json::{encode, program_to_json};
use crate::neural_loop::session::{Outcome, Proposal, ProposerFailure, Status};
use crate::transform::sha256_hex;

/// Schema of a submitted expressions or sketch document.
const EXPRESSIONS_SCHEMA: &str = "zeno-fcis/loop-expressions/1";
/// Schema of a family instance manifest.
const MANIFEST_SCHEMA: &str = "zeno-fcis/loop-family-manifest/1";
/// Profile of a meaning key.
const MEANING_PROFILE: &str = "zeno-fcis/loop-meaning/1";
/// Bytes of observations hashed per block of a meaning key.
const MEANING_BLOCK: usize = 1 << 16;
/// Schema of the stateless doors' results.
const DOORS_SCHEMA: &str = "zeno-fcis/loop-doors/2";
/// Largest expressions, sketch or manifest file.
const DOCUMENT_LIMIT: u64 = 65_536;
/// Largest program file named by a manifest.
const PROGRAM_LIMIT: u64 = 65_536;
/// Most expressions in one document (one per output).
const MAX_STATEMENTS: usize = 256;
/// Most instances one family or transfer run judges: each costs up to two
/// contained stages, within the loop's 64-frame control budget.
const MAX_INSTANCES: usize = 10;
/// Largest domain the doors enumerate and check.
const DOOR_MAX_TUPLES: u64 = DEFAULT_MAX_TUPLES;
/// The stateless doors' own invocation deadline, inside the contained
/// process's 25-second whole-run cap.
const STATELESS_DEADLINE: Duration = Duration::from_secs(22);
/// Control frames available to stages (the supervisor allows 64, one is the
/// exit report).
const FRAME_BUDGET: u32 = 62;
/// Least invocation time left after the confirming check for the attempt's
/// own check, so a door never runs the session into its deadline.
const ATTEMPT_RESERVE: Duration = Duration::from_millis(50);

// Fixed-vocabulary learn door, separate from sketch/version-space questions.
#[path = "loop_learn.rs"]
mod learn;

// BEGIN L1: proposal diagnostics, separate from checker authority.
#[path = "loop_search_limits.rs"]
mod search_limits;
// END L1.

#[derive(Subcommand)]
pub(crate) enum DoorCommand {
    // BEGIN black-box capture command (additive)
    #[command(
        about = "Capture references: owner whole/open/status, public packet, proposer candidate.",
        long_about = r#"Five operations share a bounded reference-capture protocol. The owner uses whole for one complete paged arithmetic comparison, or open to freeze an executable observation table in a private capture session. Packet exposes only the ABI, round and selected examples; the proposer submits candidate. Status returns owner-only evidence and rechecks stored success. These roles are a data-separation convention, not authenticated identities.

Inputs use finite-program ABI JSON or zeno-fcis/capture-expressions/1, not the ordinary loop-expressions document. Acceptance requires the existing transform checker and matching receipt replay over the complete observed finite domain. See each child for bounds and exits; results use zeno-fcis/black-box-capture/2. No runtime publication authority."#
    )]
    Capture {
        #[command(subcommand)]
        command: capture::Command,
    },
    // END black-box capture command
    // BEGIN composable program loop commands (no session attempts).
    #[command(
        about = "Complete, repair, and simplify a hash-bound proposal with check and receipt replay.",
        long_about = r#"Read --original (canonical bytes) and --proposal (zeno-fcis/loop-pipeline/1), each at most 65,536 bytes. Use --packet first to obtain original_sha256 and implementation_sha256; both must match the proposal. Supply either program JSON or an expressions document, plus optional stages (at most five; default complete,repair,canonical).

Stages run only when applicable: complete holes; repair refuted complete expressions; canonical or resubstitute a qualified program. The original must have at least one successful evaluation; all-original-observations-fail returns inconclusive (2). Qualification always needs transform check over the full declared input domain and matching receipt replay. Shared limits bound tuples, check pairs, stages, work and time. --out writes evidence, and candidate/original bytes only on overall qualification.

No session or attempts. JSON: zeno-fcis/loop-doors/2. Exit 0 packet or qualified; 1 refused/refuted; 2 inconclusive; 3 I/O error. Prior successful checks do not qualify a later incomplete result."#
    )]
    Pipeline {
        /// Canonical reference program (.zcve), at most 65,536 bytes.
        #[arg(long)]
        original: PathBuf,
        /// Pipeline JSON (zeno-fcis/loop-pipeline/1) bound to the reference and implementation hashes.
        #[arg(long, required_unless_present = "packet")]
        proposal: Option<PathBuf>,
        /// Print binding information without running a search.
        #[arg(long, conflicts_with = "out")]
        packet: bool,
        /// Write evidence to a new directory; candidate bytes require overall qualification.
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        limits: pipeline::SearchLimits,
    },
    #[command(
        about = "Select and evaluate stage sequences, run named strategies, or query usage knowledge.",
        long_about = r#"Choose one mode. --tasks reads zeno-fcis/loop-strategy/1 with exactly one development and one evaluation task, distinct original bytes and hash-bound pipeline proposals. At runtime --depth must be 1: compare five sequences, freeze the development winner, then evaluate without changing it.

Alternatively --kind and --problem read zeno-fcis/loop-strategy-problem/1. split-by-cases requires --original and qualifies through complete reference capture, transform check and replay. diagnose proposes weak-fault explanations/probes; ask-when-it-matters proposes answers/questions over the supplied finite readings. Neither latter mode qualifies a program.

--kb-op query/check/plan/advise/explain performs bounded structural usage inference; global --kb selects external layers. It supplies no program or execution authority. Shared search limits apply; files are at most 65,536 bytes. No session or attempts.

Task/named results use zeno-fcis/loop-doors/2; knowledge advice may use zeno-fcis/usage-knowledge/1. Exit 0 evaluated/qualified or completed structural inference; 1 refused/refuted; 2 inconclusive; 3 I/O error. Named question/diagnosis/answer proposals also exit 0; inconsistent knowledge exits 1 and no plan exits 2. An evaluated task report can contain unqualified programs; inspect each check."#
    )]
    Strategy {
        /// Task manifest (zeno-fcis/loop-strategy/1): one development task and one distinct evaluation task.
        #[arg(long, conflicts_with_all = ["problem", "kb_op"], required_unless_present_any = ["problem", "kb_op"])]
        tasks: Option<PathBuf>,
        /// Run a named strategy through the same typed plan/trace surface.
        #[arg(long, requires = "problem", value_parser = ["split-by-cases", "diagnose", "ask-when-it-matters"])]
        kind: Option<String>,
        /// Problem JSON (zeno-fcis/loop-strategy-problem/1) for the selected --kind.
        #[arg(long, requires = "kind")]
        problem: Option<PathBuf>,
        /// Fixed full-domain reference for split-by-cases.
        #[arg(long, requires = "problem")]
        original: Option<PathBuf>,
        // BEGIN structural knowledge strategy options.
        /// Query, check structural code consistency, or propose a type-directed plan.
        #[arg(long, value_parser = ["query", "check", "plan", "advise", "explain"], conflicts_with_all = ["tasks", "kind", "problem", "original", "out"])]
        kb_op: Option<String>,
        /// Query or explanation expression for --kb-op query or explain.
        #[arg(long, requires = "kb_op")]
        query: Option<String>,
        /// Comma-separated available type names for --kb-op plan.
        #[arg(long, requires = "kb_op")]
        have: Option<String>,
        /// Comma-separated required type names for --kb-op plan.
        #[arg(long, requires = "kb_op")]
        want: Option<String>,
        /// Maximum plan length, at most 16; only used with --kb-op.
        #[arg(long, default_value_t = 8)]
        kb_max_steps: usize,
        /// Maximum planning states, at most 4,096; only used with --kb-op.
        #[arg(long, default_value_t = 4096)]
        kb_max_states: usize,
        // END structural knowledge strategy options.
        /// Runtime depth is restricted to one by the invocation-wide frame budget.
        #[arg(long, default_value_t = 1)]
        depth: u8,
        /// New directory for the result and any qualified artifacts; existing paths are refused.
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        limits: pipeline::SearchLimits,
    },
    #[command(
        about = "Mine or load concrete rewrites, then recheck rules and the target program.",
        long_about = r#"Read --original canonical bytes and optionally --terms (zeno-fcis/loop-terms/1) or --bank (zeno-fcis/loop-rule-bank/1), each at most 65,536 bytes. Without either, mine terms from the original. Terms are single-output program JSON or kind/expression objects using the corpus vocabulary.

Check proposed cheaper concrete rules on this target input domain, then transform-check and replay any whole-program replacement against the fixed original. These are finite-domain rules, not universal identities. Shared search limits bound work, cells, terms, checks, tuples and time. --out exports result/bank and, only when qualified, original/candidate bytes. An unchanged original can qualify with improved:false.

No session or attempts. JSON: zeno-fcis/loop-doors/2. Exit 0 qualified; 1 refused/refuted; 2 inconclusive; 3 I/O error."#
    )]
    Rules {
        /// Canonical reference program (.zcve), at most 65,536 bytes.
        #[arg(long)]
        original: PathBuf,
        /// Optional term corpus (zeno-fcis/loop-terms/1); otherwise mine the original program.
        #[arg(long, conflicts_with = "bank")]
        terms: Option<PathBuf>,
        /// Existing rule bank (zeno-fcis/loop-rule-bank/1); recheck on this target domain.
        #[arg(long)]
        bank: Option<PathBuf>,
        /// New directory for the result and any qualified artifacts; existing paths are refused.
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        limits: pipeline::SearchLimits,
    },
    // END composable program loop commands.
    #[command(
        about = "Enumerate expressions fitting examples; optionally qualify against a complete reference.",
        long_about = r#"Read --spec (ASCII zeno-fcis/loop-learn/1, at most 65,536 bytes): named Bool/Int inputs, output domains, declared vocabulary, max_size and complete examples. Integer bounds/labels here are JSON integers, unlike program JSON decimal strings. Enumerate expressions only within the declared vocabulary/size; the library evaluator independently rechecks the lowered examples. Uniqueness is an enumeration proposal.

Default caps may only be reduced: 2,000,000 work, 4,096 tuples/meanings, 1,000,000 cells, 10,000 conflict subsets. With --original (canonical bytes, at most 65,536 bytes and the same ABI), every reference input must evaluate successfully within the 4,096-tuple cap. A fitting candidate must pass complete observed-table capture plus original-bound transform check and receipt replay; --out requires that reference and exports candidate bytes only when qualified. No session or attempts.

JSON: zeno-fcis/loop-doors/2. Without qualification: determined=0, question=3, outside-vocabulary/ambiguous/refused=1, inconclusive=2. With a reference: qualified=0, refuted/refused=1, inconclusive=2; I/O errors can be 3. Example fit alone issues no equivalence receipt."#
    )]
    Learn {
        /// Declaration and examples (zeno-fcis/loop-learn/1), at most 64 KiB.
        #[arg(long)]
        spec: PathBuf,
        // BEGIN L2: an optional complete native reference enables qualification.
        /// Automatically capture and check the learned program against this native reference.
        #[arg(long)]
        original: Option<PathBuf>,
        /// Export a qualified candidate and both bound reference programs to a new directory.
        #[arg(long, requires = "original")]
        out: Option<PathBuf>,
        // END L2.
        /// Enumeration work units, at most 2,000,000.
        #[arg(long, default_value_t = 2_000_000)]
        max_work: u64,
        /// Enumerated input tuples, at most 4,096.
        #[arg(long, default_value_t = 4096)]
        max_tuples: u64,
        /// Raw meanings per kind, at most 4,096.
        #[arg(long, default_value_t = 4096)]
        max_meanings: usize,
        /// Cumulative enumeration table cells, at most 1,000,000.
        #[arg(long, default_value_t = 1_000_000)]
        max_cells: u64,
        /// Example subsets examined for a conflict, at most 10,000.
        #[arg(long, default_value_t = 10_000)]
        max_conflict_subsets: u64,
    },
    #[command(
        about = "Solve finite expression holes, then check one completion in an existing session.",
        long_about = r#"Read --sketch (zeno-fcis/loop-expressions/1, at most 65,536 bytes), including named inputs and finite constant/choice holes. Load --session, replay its retained evidence, and search for a completion within the door budgets (at most 131,072 input tuples, five-second search stages). A proposed completion needs transform check and receipt replay before the session checker independently judges it on the full original domain.

One submitted completion or decided empty/malformed proposal consumes an attempt; an unfinished search or confirming check can return before submission, retaining charged preparation/replay work. JSON: zeno-fcis/transform-loop/1 with detail.door diagnostics. Exit 0 equivalent (improved or not) or duplicate with independent check/replay confirmation; 1 different/refused/unconfirmed duplicate or failed proposal; 2 incomplete, failed check or closed session; 3 I/O error. Creates no session; a selected replacement retains its receipt. Unrealizable is a search finding, not a checker proof."#
    )]
    Sketch {
        /// Existing session directory created by loop open.
        #[arg(long)]
        session: PathBuf,
        /// Sketch document (zeno-fcis/loop-expressions/1) with holes.
        #[arg(long)]
        sketch: PathBuf,
        /// Optional JSON recorded as the proposer's untrusted provenance.
        #[arg(long)]
        provenance: Option<PathBuf>,
    },
    #[command(
        about = "Check a parameterized program or sketch on each listed finite family instance.",
        long_about = r#"Read --instances (zeno-fcis/loop-family-manifest/1) and --program (zeno-fcis/loop-expressions/1 with $PARAMETERS). Parameter values belong in the manifest; the family program must not supply its own. Files and named canonical programs are at most 65,536 bytes; the manifest lists 1 to 10 instances with relative paths and signed-64-bit JSON integer parameter values. Each instance is limited to 131,072 input tuples.

Lower or solve the program per instance, then require transform check plus matching receipt replay. --fit proposes affine hole values a*$P+b from instances marked for fitting and checks the lifted program on all listed instances. This establishes no unlisted-instance claim. No session or attempts; per-instance reports include receipt digests, not exported receipt files.

JSON: zeno-fcis/loop-doors/2. Exit 0 accepted summarizes the initial per-instance checks; the fitted template has a separate detail.lifted_check verdict, which does not change that exit. Exit 1 refuted or invalid input; 2 unrealizable/partial or containment inconclusive; 3 I/O error."#
    )]
    Family {
        /// Instance manifest (zeno-fcis/loop-family-manifest/1).
        #[arg(long)]
        instances: PathBuf,
        /// Family program (zeno-fcis/loop-expressions/1) over $PARAMETERS.
        #[arg(long)]
        program: PathBuf,
        /// Fit each hole's values to a * $P + b and check the lifted program.
        #[arg(long)]
        fit: bool,
    },
    #[command(
        about = "Try a source program directly, as a sketch, then by repair on listed instances.",
        long_about = r#"Read --from (zeno-fcis/loop-expressions/1 with source parameters) and --to-instances (zeno-fcis/loop-family-manifest/1). Source parameter values are signed-64-bit JSON integers. Source expressions must be plain and use literal constants, not $PARAMETERS. Use the supplied source parameter values to locate constants near changed target parameters; try direct use, open those constants as choice holes, then try repair. Every successful target program needs transform check and matching receipt replay on that target reference domain.

At most 10 listed instances, 131,072 tuples per instance, and 65,536 bytes per document/program; bounded search/check stages share the invocation deadline. No session or attempts; report receipt digests without exporting receipt files. Coverage is only the listed instances.

JSON: zeno-fcis/loop-doors/2. Exit 0 transferred to all; 1 failed (none) or refused; 2 partial or containment inconclusive. Per-target budget stops can aggregate to failed (1) or partial (2); inspect rows; 3 I/O error."#
    )]
    Transfer {
        /// Source program (zeno-fcis/loop-expressions/1) with its instance's parameters.
        #[arg(long)]
        from: PathBuf,
        /// Target instance manifest (zeno-fcis/loop-family-manifest/1).
        #[arg(long = "to-instances")]
        to_instances: PathBuf,
    },
    #[command(
        about = "Hash complete finite observations and compare them with the reference program.",
        long_about = r#"Read --original and either canonical --candidate or --expressions (zeno-fcis/loop-expressions/1); each file is at most 65,536 bytes. Require identical input/output domains and at most 131,072 input tuples. The library evaluator computes every output or exact evaluation failure in enumeration order; hash these observations and domains under zeno-fcis/loop-meaning/1.

Returns both digests and same_as_original. Digest equality is a meaning key, subject to hash collisions; it is not an independent transform verdict or receipt. No session or attempts. JSON: zeno-fcis/loop-doors/2. Exit 0 keyed even when the keys differ; 1 refused; 2 inconclusive; 3 I/O error."#
    )]
    Meaning {
        /// Canonical original program (program.zcve encoding).
        #[arg(long)]
        original: PathBuf,
        /// Program as one expression per output (zeno-fcis/loop-expressions/1).
        #[arg(
            long,
            conflicts_with = "candidate",
            required_unless_present = "candidate"
        )]
        expressions: Option<PathBuf>,
        /// Canonical candidate program.
        #[arg(long)]
        candidate: Option<PathBuf>,
    },
    #[command(
        about = "Evaluate one expression at one supplied input; produce values without an equivalence verdict.",
        long_about = r#"Read --original for input domains and --expressions as zeno-fcis/loop-expressions/1 with vocabulary and exactly one plain output expression, no parameters or holes. Pass --input as comma-separated signed-i128 values in the original input order (use --input=-1,0 for negatives). The original domain must have at most 131,072 tuples even though this operation evaluates only one point. Files are at most 65,536 bytes.

Lower the expression and use the library evaluator with a full signed-i128 or Boolean output domain. JSON: zeno-fcis/transform-loop/1, evaluated values are decimal strings. Exit 0 evaluated; 1 refused (including invalid input or arithmetic failure); 2 containment inconclusive; 3 I/O error. No session, attempts, equivalence verdict or receipt."#
    )]
    Evaluate {
        /// Canonical program supplying the declared input domains.
        #[arg(long)]
        original: PathBuf,
        /// One plain output expression and its input vocabulary.
        #[arg(long)]
        expressions: PathBuf,
        /// Input values in the original's ABI order.
        #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
        input: Vec<i128>,
    },
    #[command(
        about = "Find open contract decisions, or ask which finite sketch completion is intended.",
        long_about = r#"Choose --contract (project.zeno and v2/policy.json) with optional zeno-fcis/loop-answers/1 --answers, or --specification (zeno-fcis/loop-version-space/1). Contract mode compares laws and answers with the existing core on the declared finite domain and asks about the largest ambiguous region; answers are not adopted. At most 131,072 tuples and 4,000,000 admissibility checks.

Requirements-only mode enumerates finite sketch completions under laws/examples and proposes a distinguishing input or conflict. A singleton is checked using the library law evaluator, transform check and receipt replay. Default ceilings: 2,000,000 work, 4,096 tuples/meanings, 1,000,000 cells, 10,000 subsets.

No session or attempts. Contract JSON: zeno-fcis/loop-doors/2; exit 0 completed question report, 1 conflict/inconsistent/refused, 2 incomplete/inconclusive/out-of-scope, 3 I/O error. Requirements JSON: zeno-fcis/loop-version-space/2 (early input errors can use transform-loop/1); question/no-completion/determined=0, refused=1, inconclusive=2. A question or count is not a program acceptance."#
    )]
    Question {
        /// Existing application contract (original question mode).
        #[arg(
            long,
            conflicts_with = "specification",
            required_unless_present = "specification"
        )]
        contract: Option<PathBuf>,
        /// Requirements-only finite sketch, laws and examples; no existing core.
        #[arg(long, conflicts_with_all = ["contract", "answers"])]
        specification: Option<PathBuf>,
        /// The owner's answers so far (contract question mode only).
        #[arg(long, requires = "contract")]
        answers: Option<PathBuf>,
        #[command(flatten)]
        version_limits: version_space::Arguments,
    },
    #[command(
        about = "Compare restated rules with the core on all declared inputs and reachable states.",
        long_about = r#"Read --contract (project.zeno and v2/policy.json), --rule (zeno-fcis/loop-rules/1), and optional --answers (zeno-fcis/loop-answers/1). Documents are at most 65,536 bytes; the supported finite core is limited to 131,072 tuples. The door evaluator classifies rules as exact, holds, wrong or vacuous, gives regions/witnesses, and computes required/open/forbidden where its admissibility budget permits.

JSON: zeno-fcis/loop-doors/2. Exit 0 judged even if a rule is wrong; 1 refused; 2 inconclusive/out-of-scope; 3 I/O error. No session, attempts or equivalence receipt, and no rule or owner answer is adopted."#
    )]
    TeachBack {
        /// Application directory holding project.zeno and v2/policy.json.
        #[arg(long)]
        contract: PathBuf,
        /// The rules (zeno-fcis/loop-rules/1).
        #[arg(long = "rule")]
        rules: PathBuf,
        /// The owner's answers so far, for whether they already require each rule (zeno-fcis/loop-answers/1).
        #[arg(long)]
        answers: Option<PathBuf>,
    },
    #[command(
        about = "Evaluate component interfaces and case tables; optionally check their full composition.",
        long_about = r#"Read --contract and --architecture (zeno-fcis/loop-architecture/1, at most 65,536 bytes). Steps 1 to 3 use door simulation for required behavior, realizability and component case tables over the supported finite core (at most 131,072 tuples). Simulation findings are not composed-program acceptance.

--compose compares the composition checker joint step with the core on every declared input and checks obligations; accepted requires both plus unblamed step-3 tables. --export requires --compose and a new directory; it writes generated contracts and the composition manifest even for a negative verdict.

JSON: zeno-fcis/loop-doors/2. Exit 0 realizable/completed/meets-requirements/accepted; 1 unrealizable/refuted/refused; 2 inconclusive/out-of-scope; 3 I/O error. No loop session, attempts or transform receipt; only --compose invokes the independent composition checker."#
    )]
    Decompose {
        /// Application directory holding project.zeno and v2/policy.json.
        #[arg(long)]
        contract: PathBuf,
        /// The components in merge order (zeno-fcis/loop-architecture/1).
        #[arg(long)]
        architecture: PathBuf,
        /// Step 4, the checked verdict: compose the components with the composition checker's joint step, compare with the core on every input, and run its obligation check.
        #[arg(long)]
        compose: bool,
        /// With --compose: write the composition manifest and the generated component contracts to this new directory (refused if it exists; written whatever the verdict).
        #[arg(long, requires = "compose")]
        export: Option<PathBuf>,
    },
}

// Requirements-only question mode.
#[path = "loop_version_space.rs"]
mod version_space;

#[path = "loop_judges.rs"]
pub(crate) mod judges;

#[path = "loop_decompose.rs"]
mod decompose;

#[path = "loop_compose.rs"]
mod compose;

// BEGIN black-box capture module (additive)
#[path = "loop_capture.rs"]
pub(crate) mod capture;
// END black-box capture module

// BEGIN composable program loop modules (separate from architecture composition).
#[path = "loop_pipeline.rs"]
pub(crate) mod pipeline;
#[path = "loop_rules.rs"]
mod rules;
#[path = "loop_strategy.rs"]
mod strategy;
#[path = "loop_strategy_profiles.rs"]
mod strategy_profiles;
// BEGIN structural knowledge strategy module.
#[path = "loop_strategy_knowledge.rs"]
mod strategy_knowledge;
// END structural knowledge strategy module.
// END composable program loop modules.

/// Runs a stateless door or a sketch, and returns the exit code and report.
pub(crate) fn run(command: DoorCommand) -> (u8, Value) {
    run_with_policy(command, None)
}
pub(crate) fn run_with_policy(
    command: DoorCommand,
    knowledge: Option<&crate::loop_knowledge::LoopPolicy>,
) -> (u8, Value) {
    match command {
        // BEGIN black-box capture dispatch (additive)
        DoorCommand::Capture { command } => capture::run(command),
        DoorCommand::Evaluate {
            original,
            expressions,
            input,
        } => evaluate_command(&original, &expressions, &input),
        // END black-box capture dispatch
        // BEGIN composable program loop dispatch (qualification uses check + replay).
        DoorCommand::Pipeline {
            original,
            proposal,
            packet,
            out,
            limits,
        } => as_door(
            "pipeline",
            pipeline::command(
                &original,
                proposal.as_deref(),
                packet,
                out.as_deref(),
                limits,
                knowledge,
            ),
        ),
        DoorCommand::Strategy {
            tasks,
            kind,
            problem,
            original,
            // BEGIN structural knowledge strategy dispatch fields.
            kb_op,
            query,
            have,
            want,
            kb_max_steps,
            kb_max_states,
            // END structural knowledge strategy dispatch fields.
            depth,
            out,
            limits,
        } => as_door(
            "strategy",
            strategy::command(
                tasks.as_deref(),
                kind.as_deref(),
                problem.as_deref(),
                original.as_deref(),
                // BEGIN structural knowledge strategy dispatch arguments.
                None,
                kb_op.as_deref(),
                query.as_deref(),
                have.as_deref(),
                want.as_deref(),
                kb_max_steps,
                kb_max_states,
                // END structural knowledge strategy dispatch arguments.
                depth,
                out.as_deref(),
                limits,
                knowledge,
            ),
        ),
        DoorCommand::Rules {
            original,
            terms,
            bank,
            out,
            limits,
        } => as_door(
            "rules",
            rules::command(
                &original,
                terms.as_deref(),
                bank.as_deref(),
                out.as_deref(),
                limits,
            ),
        ),
        // END composable program loop dispatch.
        DoorCommand::Learn {
            spec,
            original,
            out,
            max_work,
            max_tuples,
            max_meanings,
            max_cells,
            max_conflict_subsets,
        } => learn::run(
            &spec,
            original.as_deref(),
            out.as_deref(),
            zeno_fcis_loop_doors::learn::Limits {
                work: max_work,
                tuples: max_tuples,
                meanings: max_meanings,
                cells: max_cells,
                conflict_subsets: max_conflict_subsets,
            },
        ),
        DoorCommand::Sketch {
            session,
            sketch,
            provenance,
        } => {
            let mut acquisition = Acquisition::reserve(AcquisitionMode::Resume);
            let outcome = session_door(
                &session,
                &sketch,
                provenance.as_deref(),
                Door::Sketch,
                &mut acquisition,
            );
            super::acquisition_outcome(outcome, &acquisition)
        }
        DoorCommand::Family {
            instances,
            program,
            fit,
        } => as_door("family", family_command(&instances, &program, fit)),
        DoorCommand::Transfer { from, to_instances } => {
            as_door("transfer", transfer_command(&from, &to_instances))
        }
        DoorCommand::Meaning {
            original,
            expressions,
            candidate,
        } => as_door(
            "meaning",
            meaning_command(&original, expressions.as_deref(), candidate.as_deref()),
        ),
        DoorCommand::Question {
            contract,
            specification,
            answers,
            version_limits,
        } => {
            if let Some(path) = specification {
                version_space::run(&path, version_limits)
            } else if let Some(dir) = contract {
                as_door("question", question_command(&dir, answers.as_deref()))
            } else {
                refused("question-needs-contract-or-specification")
            }
        }
        DoorCommand::TeachBack {
            contract,
            rules,
            answers,
        } => as_door(
            "teach-back",
            teach_back_command(&contract, &rules, answers.as_deref()),
        ),
        DoorCommand::Decompose {
            contract,
            architecture,
            compose,
            export,
        } => as_door(
            "decompose",
            decompose_command(&contract, &architecture, compose, export.as_deref()),
        ),
    }
}

/// `loop candidate --expressions FILE`.
pub(crate) fn expressions(dir: &Path, path: &Path, provenance: Option<&Path>) -> (u8, Value) {
    let mut acquisition = Acquisition::reserve(AcquisitionMode::Resume);
    let outcome = session_door(dir, path, provenance, Door::Expressions, &mut acquisition);
    super::acquisition_outcome(outcome, &acquisition)
}

// --- documents ---------------------------------------------------------------

fn refused(reason: &str) -> (u8, Value) {
    super::result(crate::INVALID, "refused", json!({"reason": reason}))
}

/// The stateless doors report under their own schema, refusals included.
fn as_door(door: &str, (exit, mut report): (u8, Value)) -> (u8, Value) {
    report["schema"] = json!(DOORS_SCHEMA);
    report["integer_encoding"] = json!(crate::report_numbers::ENCODING);
    report["door"] = json!(door);
    (exit, report)
}

fn doors_result(exit: u8, door: &str, status: &str, mut detail: Value) -> (u8, Value) {
    search_limits::annotate(door, status, &mut detail);
    (
        exit,
        json!({"schema": DOORS_SCHEMA, "integer_encoding": crate::report_numbers::ENCODING, "door": door, "status": status, "authority": "none", "detail": detail}),
    )
}

/// What a document declares about names.
struct VocabularySpec {
    inputs: Vec<String>,
    variants: BTreeMap<String, BTreeMap<String, i128>>,
    cases: Option<i128>,
}

/// A parsed expressions document (texts not yet parsed as expressions).
struct Document {
    vocabulary: Option<VocabularySpec>,
    outputs: Vec<String>,
    parameters: BTreeMap<String, i128>,
}

fn identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(format!("{what}-unknown-field:{key}")),
        None => Ok(()),
    }
}

fn integer_map(value: &Value, what: &str) -> Result<BTreeMap<String, i128>, String> {
    let object = value.as_object().ok_or_else(|| format!("{what}-shape"))?;
    let mut out = BTreeMap::new();
    for (name, value) in object {
        if !identifier(name) {
            return Err(format!("{what}-name:{name}"));
        }
        out.insert(
            name.clone(),
            value
                .as_i64()
                .map(i128::from)
                .ok_or_else(|| format!("{what}-value:{name}"))?,
        );
    }
    Ok(out)
}

fn vocabulary_spec(value: &Value) -> Result<VocabularySpec, String> {
    let object = value.as_object().ok_or("vocabulary-shape")?;
    only_keys(object, &["inputs", "variants", "cases"], "vocabulary")?;
    let inputs = object
        .get("inputs")
        .and_then(Value::as_array)
        .ok_or("vocabulary-inputs")?
        .iter()
        .map(|name| name.as_str().map(str::to_owned).ok_or("vocabulary-inputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut variants = BTreeMap::new();
    if let Some(table) = object.get("variants") {
        let table = table.as_object().ok_or("vocabulary-variants")?;
        for (field, names) in table {
            variants.insert(field.clone(), integer_map(names, "variant")?);
        }
    }
    let cases = match object.get("cases") {
        None => None,
        Some(value) => Some(
            value
                .as_i64()
                .map(i128::from)
                .filter(|n| (1..=1 << 16).contains(n))
                .ok_or("vocabulary-cases")?,
        ),
    };
    Ok(VocabularySpec {
        inputs,
        variants,
        cases,
    })
}

fn document(bytes: &[u8]) -> Result<Document, String> {
    if !bytes.is_ascii() {
        return Err("document-not-ascii".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "document-json")?;
    let object = value.as_object().ok_or("document-shape")?;
    only_keys(
        object,
        &["schema", "vocabulary", "outputs", "parameters"],
        "document",
    )?;
    if object.get("schema").and_then(Value::as_str) != Some(EXPRESSIONS_SCHEMA) {
        return Err("document-schema".into());
    }
    let outputs = object
        .get("outputs")
        .and_then(Value::as_array)
        .ok_or("document-outputs")?;
    if outputs.is_empty() || outputs.len() > MAX_STATEMENTS {
        return Err("document-statement-count".into());
    }
    let outputs = outputs
        .iter()
        .map(|text| text.as_str().map(str::to_owned).ok_or("document-outputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let vocabulary = object.get("vocabulary").map(vocabulary_spec).transpose()?;
    let parameters = match object.get("parameters") {
        None => BTreeMap::new(),
        Some(value) => integer_map(value, "parameter")?,
    };
    Ok(Document {
        vocabulary,
        outputs,
        parameters,
    })
}

fn vocabulary(spec: &VocabularySpec, program: &Program) -> Result<Vocabulary, String> {
    Vocabulary::new(
        spec.inputs.clone(),
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        &spec.variants,
    )
    .map_err(|refusal| refusal.0)
}

/// Parses the outputs with consecutive hole numbers across outputs.
fn expressions_of(texts: &[String], allow: Allow) -> Result<Vec<Expr>, String> {
    let mut next = 0;
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        let node = parse(text, next, allow).map_err(|refusal| refusal.0)?;
        next += zeno_fcis_loop_doors::expr::holes_of(&node).len();
        out.push(node);
    }
    Ok(out)
}

/// A document's bytes, refusing an unreadable or over-large file.
fn read_bytes(path: &Path) -> Result<Vec<u8>, (u8, Value)> {
    let bytes = crate::transform_command::read_bounded(path, DOCUMENT_LIMIT)
        .map_err(|error| super::io_failure(&error))?;
    if crate::transform_command::over(&bytes, DOCUMENT_LIMIT) {
        return Err(refused("document-too-large"));
    }
    Ok(bytes)
}

fn read_document(path: &Path) -> Result<Document, (u8, Value)> {
    document(&read_bytes(path)?).map_err(|reason| refused(&reason))
}

// --- rendering ---------------------------------------------------------------

fn point_json(subject: &Subject, ordinal: usize) -> Value {
    let mut out = Map::new();
    for (k, (name, value)) in subject
        .vocab
        .names()
        .iter()
        .zip(subject.point(ordinal))
        .enumerate()
    {
        let shown = if subject.vocab.input_kinds()[k] == Kind::Bool {
            json!(value == 1)
        } else {
            match subject.vocab.variant_name(name, value) {
                Some(variant) => json!(variant),
                None => crate::report_numbers::integer(value),
            }
        };
        out.insert(name.clone(), shown);
    }
    Value::Object(out)
}

fn texts(subject: &Subject, nodes: &[Expr]) -> Value {
    Value::from(
        nodes
            .iter()
            .enumerate()
            .map(|(j, node)| show(node, &subject.vocab, Some(subject.output_kind(j))))
            .collect::<Vec<_>>(),
    )
}

fn fill_json(subject: &Subject, sketch: &[Expr], k: usize, fill: &Fill) -> Value {
    let kind = sketch
        .iter()
        .find_map(|node| hole_kind(node, k, subject))
        .unwrap_or(Kind::Int);
    match fill {
        Fill::Const(value) => {
            let text = show(&Expr::Int(*value), &subject.vocab, Some(kind));
            json!({"fill": text, "value": crate::report_numbers::integer(*value)})
        }
        Fill::Choice(index) => {
            let alternative = sketch
                .iter()
                .find_map(|node| choice_alternative(node, k, *index));
            json!({"fill": alternative.map(|a| show(&a, &subject.vocab, Some(kind))), "choice": index})
        }
        Fill::Ast(expr) => json!({"fill": show(expr, &subject.vocab, Some(kind))}),
    }
}

fn hole_kind(node: &Expr, k: usize, subject: &Subject) -> Option<Kind> {
    let mut kinds = BTreeMap::new();
    for j in 0..subject.vocab.outputs().len() {
        let _ = zeno_fcis_loop_doors::expr::hole_kinds(
            node,
            subject.output_kind(j),
            &subject.vocab,
            &mut kinds,
        );
        if kinds.contains_key(&k) {
            break;
        }
    }
    kinds.get(&k).copied()
}

fn choice_alternative(node: &Expr, k: usize, index: usize) -> Option<Expr> {
    zeno_fcis_loop_doors::expr::holes_of(node)
        .into_iter()
        .find_map(|hole| match hole {
            Expr::Hole(id, zeno_fcis_loop_doors::expr::Hole::Choice(alternatives)) if *id == k => {
                alternatives.get(index).cloned()
            }
            _ => None,
        })
}

fn path_json(path: &ExprPath) -> Value {
    Value::from(path.iter().map(|i| u64::from(*i)).collect::<Vec<_>>())
}

fn program_json(program: &Program) -> Value {
    match encode(program) {
        Ok(bytes) => json!({
            "nodes": program.nodes().len(),
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "program": program_to_json(program),
        }),
        Err(_) => json!({"nodes": program.nodes().len(), "bytes": null}),
    }
}

// --- contained stages and the in-process check --------------------------------

/// Stage bookkeeping for one invocation: its deadline and control frames.
struct Stages {
    started: Instant,
    deadline: Duration,
    frames: u32,
}

impl Stages {
    fn new(started: Instant, deadline: Duration, frames_used: u32) -> Self {
        Self {
            started,
            deadline,
            frames: frames_used,
        }
    }

    /// The contained supervisor charges search/check CPU (5s/2s), including
    /// its children on systemd. The invocation still has its original wall
    /// deadline; deterministic tuple/node/work reservations are unchanged.
    fn run<T>(&mut self, search: bool, work: impl FnOnce() -> T) -> Result<T, String> {
        if self.frames + 2 > FRAME_BUDGET {
            return Err("stage-budget".into());
        }
        let cap = if search {
            SEARCH_DEADLINE
        } else {
            CHECK_DEADLINE
        };
        let duration = cap.min(self.deadline.saturating_sub(self.started.elapsed()));
        if duration < Duration::from_millis(50) {
            return Err("invocation-deadline".into());
        }
        crate::checker_process::start(duration, search)?;
        self.frames += 2;
        let begun = Instant::now();
        let value = work();
        crate::checker_process::finish()?;
        // FINISH independently checks the supervisor's kernel CPU counter
        // before permitting a result. Local unit execution has no supervisor,
        // so retain its cooperative wall fallback rather than inventing usage.
        if self.started.elapsed() >= self.deadline {
            return Err("invocation-deadline".into());
        }
        if !crate::checker_process::contained() && begun.elapsed() >= duration {
            return Err("stage-deadline".into());
        }
        Ok(value)
    }
}

/// `transform check` of `candidate` against `original` with its receipt,
/// then `transform replay` of that receipt, in one contained check stage.
fn checked(
    stages: &mut Stages,
    original: &[u8],
    candidate: &[u8],
    subject: Option<&Subject>,
) -> Value {
    // The default limits of `transform check`, so the receipt is the one a
    // separate `transform check --receipt` writes for the same pair; the
    // door only starts checks on domains within its own tuple budget.
    let limits = crate::transform::Limits {
        steps: crate::transform::DEFAULT_STEP_LIMIT,
        input_tuples: crate::transform::DEFAULT_MAX_INPUT_TUPLES,
    };
    let size = import_program(original).map(|program| tuples(program.inputs()));
    if !matches!(size, Ok(size) if size <= u128::from(DOOR_MAX_TUPLES)) {
        return json!({"verdict": "inconclusive", "reason": "domain over the door's check budget"});
    }
    let outcome = stages.run(false, || {
        crate::transform::check(original, candidate, limits).map(|equivalence| {
            let receipt = equivalence.receipt();
            let replay =
                crate::transform::replay(&receipt, original, candidate, limits.input_tuples);
            (receipt, replay)
        })
    });
    match outcome {
        Err(reason) => json!({"verdict": "inconclusive", "reason": reason}),
        Ok(Ok((receipt, replay))) => json!({
            "verdict": "equivalent",
            "checker": crate::transform::CHECKER,
            "receipt_sha256": sha256_hex(&receipt),
            "replay": if replay == crate::transform::Replay::Matched { "matched" } else { "differs" },
            "candidate_sha256": sha256_hex(candidate),
        }),
        Ok(Err(crate::transform::Rejection::Counterexample(counterexample))) => {
            let ordinal = usize::try_from(counterexample.ordinal).ok();
            json!({
                "verdict": "counterexample",
                "ordinal": counterexample.ordinal,
                "input": match (subject, ordinal) {
                    (Some(subject), Some(ordinal)) if ordinal < subject.size => point_json(subject, ordinal),
                    _ => crate::transform::values_json(&counterexample.input),
                },
                "original": observation(&counterexample.original),
                "candidate": observation(&counterexample.candidate),
            })
        }
        Ok(Err(crate::transform::Rejection::Inconclusive(stop))) => {
            json!({"verdict": "inconclusive", "reason": format!("{stop:?}")})
        }
        Ok(Err(crate::transform::Rejection::Refused(refusal))) => {
            json!({"verdict": "refused", "reason": format!("{refusal:?}")})
        }
    }
}

fn observation(observation: &crate::transform::Observation) -> Value {
    match &observation.result {
        Ok(outputs) => json!({"ok": crate::transform::values_json(outputs)}),
        Err(failure) => json!({"error": crate::transform::failure_tag(*failure)}),
    }
}

/// Whether a check accepted: equivalent and its receipt replayed.
fn accepted(check: &Value) -> bool {
    check["verdict"] == "equivalent" && check["replay"] == "matched"
}

fn bytes_of(program: &Program) -> Option<Vec<u8>> {
    encode(program).ok()
}

// --- session doors -------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Door {
    Expressions,
    Sketch,
}

/// The door's single proposal for the session's attempt.
struct DoorProposer {
    identity: &'static str,
    proposal: Option<Proposal>,
    asserted: Value,
}

impl Proposer for DoorProposer {
    fn identity(&self) -> &str {
        self.identity
    }

    fn uses_model_calls(&self) -> bool {
        false
    }

    fn is_hosted(&self) -> bool {
        false
    }

    fn exhausted(&self) -> bool {
        self.proposal.is_none()
    }

    fn propose(&mut self, _view: &Value, _feedback: &[Value]) -> Proposed {
        Proposed {
            proposal: self
                .proposal
                .take()
                .unwrap_or(Proposal::Failed(ProposerFailure::Exhausted)),
            provenance: json!({"provider": self.identity, "provider_asserted": self.asserted}),
        }
    }
}

/// What the door prepared before the attempt.
struct Prepared {
    subject: Option<Subject>,
    nodes: Vec<Expr>,
    program: Option<Program>,
    detail: Value,
}

/// An undecodable document: a malformed proposal, like undecodable JSON.
fn malformed(door: Door, reason: &str) -> Prepared {
    Prepared {
        subject: None,
        nodes: Vec::new(),
        program: None,
        detail: match door {
            Door::Expressions => {
                json!({"door": "expressions", "lowering": "refused", "reason": reason})
            }
            Door::Sketch => json!({"door": "sketch", "status": "refused", "reason": reason}),
        },
    }
}

fn prepare_expressions(document: &Document, original: &Program) -> Prepared {
    let failed = |reason: String| Prepared {
        subject: None,
        nodes: Vec::new(),
        program: None,
        detail: json!({"door": "expressions", "lowering": "refused", "reason": reason}),
    };
    let Some(spec) = &document.vocabulary else {
        return failed("document-vocabulary-missing".into());
    };
    let vocab = match vocabulary(spec, original) {
        Ok(vocab) => vocab,
        Err(reason) => return failed(reason),
    };
    let nodes = match expressions_of(
        &document.outputs,
        Allow {
            holes: false,
            params: false,
            keep: true,
        },
    ) {
        Ok(nodes) => nodes,
        Err(reason) => return failed(reason),
    };
    let mut resolved = Vec::with_capacity(nodes.len());
    for node in &nodes {
        if let Err(refusal) = vocab.check_names(node) {
            return failed(refusal.0);
        }
        match vocab.resolve_variants(node) {
            Ok(node) => resolved.push(node),
            Err(refusal) => return failed(refusal.0),
        }
    }
    let program = match lower_outputs(&resolved, &vocab, original) {
        Ok(program) => program,
        Err(refusal) => return failed(refusal.0),
    };
    let subject = Subject::new(
        original.clone(),
        vocab,
        document.parameters.clone(),
        spec.cases,
        DOOR_MAX_TUPLES,
    )
    .ok();
    let detail =
        json!({"door": "expressions", "lowering": "lowered", "lowered": program_json(&program)});
    Prepared {
        subject,
        nodes: resolved,
        program: Some(program),
        detail,
    }
}

fn prepare_sketch(
    document: &Document,
    original: &Program,
    stages: &mut Stages,
) -> (Prepared, Option<SketchResult>) {
    let failed = |reason: String| {
        (
            Prepared {
                subject: None,
                nodes: Vec::new(),
                program: None,
                detail: json!({"door": "sketch", "status": "refused", "reason": reason}),
            },
            None,
        )
    };
    let Some(spec) = &document.vocabulary else {
        return failed("document-vocabulary-missing".into());
    };
    let vocab = match vocabulary(spec, original) {
        Ok(vocab) => vocab,
        Err(reason) => return failed(reason),
    };
    let nodes = match expressions_of(
        &document.outputs,
        Allow {
            holes: true,
            params: false,
            keep: true,
        },
    ) {
        Ok(nodes) => nodes,
        Err(reason) => return failed(reason),
    };
    let solved = stages.run(true, || {
        let subject = Subject::new(
            original.clone(),
            vocab,
            document.parameters.clone(),
            spec.cases,
            DOOR_MAX_TUPLES,
        )?;
        let result = sketch::solve(&subject, &nodes, &sketch::Settings::default());
        Ok::<_, zeno_fcis_loop_doors::Refusal>((subject, result))
    });
    let (subject, result) = match solved {
        Err(reason) => {
            return (
                Prepared {
                    subject: None,
                    nodes: Vec::new(),
                    program: None,
                    detail: json!({"door": "sketch", "status": "inconclusive", "reason": reason}),
                },
                None,
            );
        }
        Ok(Err(refusal)) => return failed(refusal.0),
        Ok(Ok(solved)) => solved,
    };
    let detail = sketch_json(&subject, &result);
    (
        Prepared {
            nodes: result.expressions.clone(),
            program: result.program.clone(),
            subject: Some(subject),
            detail,
        },
        Some(result),
    )
}

fn searched_json(subject: &Subject, result: &SketchResult) -> Value {
    Value::from(
        result
            .outputs
            .iter()
            .map(|info| {
                let pivot = info.pivot.map(|k| {
                    let kind = result.sketch.get(info.output).and_then(|node| {
                        zeno_fcis_loop_doors::expr::holes_of(node)
                            .into_iter()
                            .find(|h| matches!(h, Expr::Hole(id, _) if *id == k))
                            .cloned()
                    });
                    let space = match kind {
                        Some(Expr::Hole(_, zeno_fcis_loop_doors::expr::Hole::Choice(a))) => {
                            json!({"kind": "choice", "alternatives": a.len()})
                        }
                        Some(Expr::Hole(_, zeno_fcis_loop_doors::expr::Hole::Expr)) => {
                            json!({"kind": "expression", "complete": false})
                        }
                        Some(Expr::Hole(_, zeno_fcis_loop_doors::expr::Hole::Const(range))) => {
                            let bound = result
                                .sketch
                                .get(info.output)
                                .filter(|node| {
                                    !zeno_fcis_loop_doors::inverse::positions(node).iter().any(
                                        |(_, sub)| {
                                            matches!(
                                                sub,
                                                Expr::Call(..)
                                                    | Expr::Bin(
                                                        zeno_fcis_loop_doors::expr::Op::Mul,
                                                        ..
                                                    )
                                            )
                                        },
                                    )
                                })
                                .map(|node| sketch::constant_bound(subject, node));
                            json!({"kind": "constant", "derived": "exact over all integers",
                                   "range": range.map(|(lo, hi)| crate::report_numbers::integers(&[lo, hi])),
                                   "enumeration_bound": bound.map(crate::report_numbers::integer)})
                        }
                        _ => Value::Null,
                    };
                    json!({"hole": k, "space": space})
                });
                json!({
                    "output": format!("out{}", info.output),
                    "holes": info.holes,
                    "search": info.search,
                    "solved_alone": info.solved_alone,
                    "pivot": pivot,
                    "searched": info.searched.iter().map(|s| json!({
                        "hole": s.hole,
                        "kind": if s.choice { "choice" } else { "constant" },
                        "values": s.values.len(),
                        "min": s.values.iter().min().copied().map(crate::report_numbers::integer),
                        "max": s.values.iter().max().copied().map(crate::report_numbers::integer),
                        "listed": if s.values.len() <= 64 { crate::report_numbers::integers(&s.values) } else { Value::Null },
                        "complete": s.complete,
                    })).collect::<Vec<_>>(),
                    "assignments": info.assignments.to_string(),
                    "tried": info.tried,
                    "visits": info.visits,
                    "class_values": info.class_values,
                    "budget_unit": "terminal assignment or disjoint pruned subtree",
                    "verdict": info.verdict.as_ref().map(SketchStatus::name),
                    "witness": info.witness.map(|w| json!({
                        "output": format!("out{}", w.output),
                        "ordinal": w.ordinal,
                        "input": point_json(subject, w.ordinal),
                    })),
                    "conflicting_inputs": info.conflicting_inputs.iter().map(|ordinal| json!({
                        "ordinal": ordinal, "input": point_json(subject, *ordinal),
                        "required_output": crate::report_numbers::integer(subject.targets[info.output][*ordinal]),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn sketch_json(subject: &Subject, result: &SketchResult) -> Value {
    let holes: Map<String, Value> = result
        .holes
        .iter()
        .map(|(k, fill)| (k.to_string(), fill_json(subject, &result.sketch, *k, fill)))
        .collect();
    let mut detail = json!({
        "door": "sketch",
        "status": result.status.name(),
        "sketch": texts(subject, &result.sketch),
        "holes": holes,
        "searched": searched_json(subject, result),
        "evaluated": result.evaluated,
        "reason": result.reason,
    });
    if let Some(program) = &result.program {
        detail["completion"] = program_json(program);
        detail["expressions"] = texts(subject, &result.expressions);
    }
    if result.status == SketchStatus::Unrealizable {
        let witness = result.outputs.iter().find_map(|info| info.witness);
        detail["evidence"] = match witness {
            Some(w) => {
                json!({"kind": "witness", "output": format!("out{}", w.output), "ordinal": w.ordinal,
                              "input": point_json(subject, w.ordinal)})
            }
            None => json!({"kind": "complete-space-exhausted",
                "scope": "one-shared-assignment-over-the-declared-domain",
                "explanation": "No shared assignment works on every input; this need not have a single-input witness",
                "output": result.outputs.last().map(|info| format!("out{}", info.output)),
                "conflicting_inputs": result.outputs.iter().flat_map(|info| info.conflicting_inputs.iter().map(move |ordinal| json!({
                    "output": format!("out{}", info.output), "ordinal": ordinal, "input": point_json(subject, *ordinal),
                    "required_output": crate::report_numbers::integer(subject.targets[info.output][*ordinal]),
                }))).collect::<Vec<_>>()}),
        };
    }
    let settings = sketch::Settings::default();
    detail["search_bounds"] = json!({"synth_depth":settings.synth_depth,
        "max_assignments":settings.max_assignments,"max_work":settings.max_work,
        "candidate_values":sketch::MAX_CANDIDATES,"completion_combinations":sketch::MAX_COMBINATIONS});
    search_limits::annotate("sketch", result.status.name(), &mut detail);
    detail
}

fn begin(
    dir: &Path,
    started: Instant,
    acquisition: &mut Acquisition,
    provenance: bool,
) -> Result<(super::Store, crate::neural_loop::session::Session), (u8, Value)> {
    let (mut store, mut session) = super::load_session(dir, started, acquisition)?;
    if let Status::Closed(reason) = session.status() {
        return Err(super::result(
            crate::BLOCKED,
            "session-closed",
            json!({"stop_reason": reason.name(), "report": session.report()}),
        ));
    }
    // Prepaid intake before any file is read, as for a JSON candidate: the
    // door document's own bound, twice for decoding, the provenance's when
    // given, and fixed overhead.
    let intake = 2 * DOCUMENT_LIMIT
        + if provenance {
            2 * crate::loop_proposers::CONFIG_LIMIT
        } else {
            0
        }
        + 4096;
    if let Err(reason) = session.reserve_preparation(intake) {
        if let Err(error) = store.persist(session.ledger()) {
            return Err(super::io_failure(&error));
        }
        return Err(super::result(
            crate::BLOCKED,
            "inconclusive",
            json!({"reason": reason.name(), "report": session.report()}),
        ));
    }
    store
        .persist(session.ledger())
        .map_err(|error| super::io_failure(&error))?;
    Ok((store, session))
}

fn session_door(
    dir: &Path,
    path: &Path,
    provenance: Option<&Path>,
    door: Door,
    acquisition: &mut Acquisition,
) -> (u8, Value) {
    let started = Instant::now();
    let (mut store, mut session) = match begin(dir, started, acquisition, provenance.is_some()) {
        Ok(loaded) => loaded,
        Err(failure) => return failure,
    };
    // As for `--candidate-json`: an unreadable or over-large file is refused,
    // and an undecodable document is a malformed proposal that still
    // consumes the attempt.
    let document = match read_bytes(path) {
        Ok(bytes) => document(&bytes),
        Err(failure) => return failure,
    };
    let asserted = match provenance {
        Some(path) => {
            match crate::transform_command::read_bounded(path, crate::loop_proposers::CONFIG_LIMIT)
            {
                Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
                Err(error) => return super::io_failure(&error),
            }
        }
        None => Value::Null,
    };
    let deadline = Duration::from_millis(session.request().limits().deadline_ms);
    // load_session's replay stage has already used two control frames.
    let mut stages = Stages::new(started, deadline, 2);
    let original = session.request().program().clone();
    let (prepared, solved) = match (&document, door) {
        (Err(reason), _) => (malformed(door, reason), None),
        (Ok(document), Door::Expressions) => (prepare_expressions(document, &original), None),
        (Ok(document), Door::Sketch) => prepare_sketch(document, &original, &mut stages),
    };
    let original_bytes = session.request().original().to_vec();
    // The door's confirming check runs before the attempt, which may replace
    // the incumbent: an acceptance is reported only when this check accepted
    // and its receipt replayed. A check that did not complete, or too little
    // time left for the attempt's own check, ends the invocation as
    // inconclusive without spending the attempt.
    let confirmation = prepared.program.as_ref().and_then(bytes_of).map(|bytes| {
        checked(
            &mut stages,
            &original_bytes,
            &bytes,
            prepared.subject.as_ref(),
        )
    });
    // A sketch whose solving stage could not run, or whose search stopped on
    // its budget (the solver's own `Inconclusive`), proposes nothing yet: no
    // attempt is spent.
    let unsolved = door == Door::Sketch
        && document.is_ok()
        && match &solved {
            None => prepared.detail["status"] == "inconclusive",
            Some(result) => result.status == SketchStatus::Inconclusive,
        };
    let decided = !unsolved
        && confirmation.as_ref().is_none_or(|check| {
            accepted(check) || check["verdict"] == "counterexample" || check["verdict"] == "refused"
        });
    if !decided || deadline.saturating_sub(started.elapsed()) < ATTEMPT_RESERVE {
        let reason = if decided {
            "invocation-deadline"
        } else if unsolved {
            "door-search-not-completed"
        } else {
            "door-check-not-completed"
        };
        let mut detail = prepared.detail;
        if let Some(check) = confirmation {
            if door == Door::Sketch {
                detail["status"] = json!("completion-not-confirmed");
            }
            detail["check"] = check;
        }
        return super::result(
            crate::BLOCKED,
            "inconclusive",
            json!({"reason": reason, "report": session.report(), "door": detail}),
        );
    }
    let proposal = match (&prepared.program, door) {
        (Some(program), _) => match bytes_of(program) {
            Some(bytes) => Proposal::Candidate(bytes),
            None => Proposal::Failed(ProposerFailure::Malformed("door-encoding".into())),
        },
        (None, Door::Sketch) if solved.is_some() => Proposal::Failed(ProposerFailure::Empty),
        (None, _) => Proposal::Failed(ProposerFailure::Malformed(format!(
            "{}:{}",
            if door == Door::Sketch {
                "sketch"
            } else {
                "expressions"
            },
            prepared.detail["reason"].as_str().unwrap_or("refused")
        ))),
    };
    let mut proposer = DoorProposer {
        identity: if door == Door::Sketch {
            "zeno-fcis/loop-door-sketch/1"
        } else {
            "zeno-fcis/loop-door-expressions/1"
        },
        proposal: Some(proposal),
        asserted,
    };
    let engine = super::strategy_engine();
    let outcomes = match super::drive(
        &mut session,
        &mut store,
        &mut proposer,
        &engine,
        started,
        Some(1),
    ) {
        Ok(outcomes) => outcomes,
        Err(error) => return super::io_failure(&error),
    };
    // The attempt's check stage used two more frames.
    stages.frames += 2;
    let request = session.request();
    let inputs = request.program().inputs();
    let outputs = request.program().outputs();
    let (Some(record), Some(outcome)) = (
        session
            .attempts()
            .last()
            .filter(|_| proposer.proposal.is_none()),
        outcomes.last(),
    ) else {
        let stop = match session.status() {
            Status::Closed(reason) => reason.name(),
            Status::Open => "open".to_owned(),
        };
        return super::result(
            crate::BLOCKED,
            "session-closed",
            json!({"stop_reason": stop, "report": session.report(), "door": prepared.detail}),
        );
    };
    let checked_duplicate =
        matches!(outcome, Outcome::Duplicate(_)) && confirmation.as_ref().is_some_and(accepted);
    let exit = match outcome {
        Outcome::Duplicate(_) if checked_duplicate => crate::OK,
        Outcome::Equivalent { .. } if confirmation.as_ref().is_some_and(accepted) => crate::OK,
        Outcome::Equivalent { .. } => crate::BLOCKED,
        Outcome::Different(_)
        | Outcome::Refused(_)
        | Outcome::Duplicate(_)
        | Outcome::Failed(_) => crate::INVALID,
        _ => crate::BLOCKED,
    };
    let mut detail = prepared.detail;
    match (outcome, &prepared.program, &prepared.subject) {
        (Outcome::Equivalent { .. } | Outcome::Duplicate(_), Some(_), _)
            if !matches!(outcome, Outcome::Duplicate(_)) || checked_duplicate =>
        {
            if let Some(check) = confirmation {
                if door == Door::Sketch || checked_duplicate {
                    detail["status"] = json!(if accepted(&check) {
                        "completed"
                    } else {
                        "completion-not-confirmed"
                    });
                }
                if checked_duplicate {
                    detail["selection"] = json!("duplicate");
                }
                detail["check"] = check;
            }
        }
        (Outcome::Different(_) | Outcome::Duplicate(_), Some(program), Some(subject))
            if door == Door::Expressions =>
        {
            detail["explanation"] = explanation(
                &mut stages,
                subject,
                &prepared.nodes,
                program,
                &original_bytes,
            );
        }
        (Outcome::Different(_), Some(_), _) if door == Door::Sketch => {
            // The solver's own comparison agreed but the session's check did
            // not: a proposer defect, reported and never adopted.
            detail["status"] = json!("completion-refused-by-the-checker");
        }
        _ => {}
    }
    let incumbent_program = match session.incumbent().replacement() {
        Some(replacement) => import_program(replacement.bytes())
            .map(|program| program_to_json(&program))
            .unwrap_or(Value::Null),
        None => program_to_json(request.program()),
    };
    super::result(
        exit,
        &record.outcome,
        json!({
            "attempt": record.attempt,
            "outcome": outcome.json(request),
            "feedback": record.feedback.json(inputs, outputs),
            "all_feedback": session.feedback().iter().map(|feedback| feedback.json(inputs, outputs)).collect::<Vec<_>>(),
            "incumbent": session.incumbent().json(request.original_sha256(), request.cost()),
            "incumbent_program": incumbent_program,
            "status": session.incumbent().status(),
            "accounting": session.ledger().accounting().json(),
            "session": match session.status() {
                Status::Open => json!({"state": "open"}),
                Status::Closed(reason) => json!({"state": "closed", "stop_reason": reason.name()}),
            },
            "door": detail,
        }),
    )
}

fn failure_region_json(subject: &Subject, region: &repair::Region) -> Value {
    let mut region_json = json!({
        "tuples": region.tuples,
        "of": region.of,
        "per_output": region.per_output.iter().map(|(j, n)| (format!("out{j}"), json!(n))).collect::<Map<_, _>>(),
    });
    match &region.formula {
        Some(formula) => {
            region_json["where"] = json!(show(formula, &subject.vocab, Some(Kind::Bool)));
            region_json["formula"] = json!("exact on every tuple of the domain");
        }
        None => {
            region_json["shared_fields"] = region
                .shared
                .iter()
                .map(|(k, v)| {
                    let name = &subject.vocab.names()[*k];
                    let shown = if subject.vocab.input_kinds()[*k] == Kind::Bool {
                        json!(*v == 1)
                    } else {
                        subject.vocab.variant_name(name, *v).map_or_else(
                            || crate::report_numbers::integer(*v),
                            |variant| json!(variant),
                        )
                    };
                    (name.clone(), shown)
                })
                .collect::<Map<_, _>>()
                .into();
        }
    }
    region_json
}

/// The region, ranked sites and a repair for a refuted proposal; a repair is
/// "checked repair" only after `transform check` and its replay accept it.
fn explanation(
    stages: &mut Stages,
    subject: &Subject,
    nodes: &[Expr],
    program: &Program,
    original: &[u8],
) -> Value {
    let explained = stages.run(true, || {
        repair::explain(subject, nodes, program, true, &repair::Settings::default())
    });
    let explained = match explained {
        Err(reason) => return json!({"status": "inconclusive", "reason": reason}),
        Ok(Err(refusal)) => return json!({"status": "refused", "reason": refusal.0}),
        Ok(Ok(explained)) => explained,
    };
    let mut region_json = failure_region_json(subject, &explained.region);
    region_json["output_regions"] = Value::Object(
        explained
            .output_regions
            .iter()
            .map(|(j, region)| (format!("out{j}"), failure_region_json(subject, region)))
            .collect(),
    );
    let mut sites = Vec::new();
    let mut unable = Map::new();
    for (j, localised) in &explained.localised {
        unable.insert(
            format!("out{j}"),
            json!({"positions_proved_unable": localised.proved_unable, "viable_positions": localised.viable}),
        );
        for site in &localised.sites {
            sites.push(json!({
                "output": format!("out{}", site.output),
                "path": path_json(&site.path),
                "subexpression": show(&site.subexpression, &subject.vocab, None),
                "size": site.size,
                "replacement": site.replacement.as_ref().map(|r| show(r, &subject.vocab, Some(site.kind_at))),
                "kind": site.found_by.map(repair::SiteKind::name),
                "label": site.found_by.map(|kind| if kind == repair::SiteKind::Synthesised { "rewrite" } else { kind.name() }),
                "repaired_nodes": site.repaired_nodes,
            }));
        }
    }
    let repair_json = match &explained.repair {
        None => Value::Null,
        Some(proposed) => {
            let mut out = json!({
                "proposed": proposed.status.name(),
                "expressions": texts(subject, &proposed.expressions),
                "steps": proposed.steps.iter().map(|step| json!({
                    "output": format!("out{}", step.output),
                    "path": path_json(&step.path),
                    "subexpression": show(&step.subexpression, &subject.vocab, None),
                    "replacement": show(&step.replacement, &subject.vocab, None),
                })).collect::<Vec<_>>(),
            });
            if let RepairStatus::Refused(refusal) = &proposed.status {
                out["reason"] = json!(refusal.0);
            }
            if let (RepairStatus::Repaired, Some(program)) = (&proposed.status, &proposed.program) {
                out["program"] = program_json(program);
                if let Some(bytes) = bytes_of(program) {
                    let check = checked(stages, original, &bytes, Some(subject));
                    out["label"] = json!(if accepted(&check) {
                        "checked repair"
                    } else {
                        "repair not confirmed by the checker"
                    });
                    out["check"] = check;
                }
            }
            let settings = repair::Settings::default();
            out["search_bounds"] = json!({"attempted_sites":repair::MAX_ATTEMPTS,
                "reported_sites":repair::MAX_SITES,"synthesis_work":settings.synthesis.total_work,
                "synth_depth":settings.synth_depth,"max_steps":settings.max_steps,
                "max_positions":settings.max_positions});
            search_limits::annotate("repair", proposed.status.name(), &mut out);
            out
        }
    };
    json!({
        "region": region_json,
        "sites": sites,
        "localisation": unable,
        "repair": repair_json,
    })
}

// --- manifests -------------------------------------------------------------------

struct Instance {
    name: String,
    parameters: BTreeMap<String, i128>,
    program: Vec<u8>,
    optimize_nodes: Option<usize>,
    fit: bool,
}

/// One listed instance before its files are read: name, parameters, program
/// path, optional optimize-result path, and whether it is fitted on.
type Listed = (
    String,
    BTreeMap<String, i128>,
    PathBuf,
    Option<PathBuf>,
    bool,
);

struct Manifest {
    family: String,
    vocabulary: VocabularySpec,
    instances: Vec<Instance>,
}

fn relative(base: &Path, text: &str) -> Result<PathBuf, String> {
    let path = Path::new(text);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("manifest-path:{text}"));
    }
    Ok(base.join(path))
}

fn read_manifest(path: &Path) -> Result<Manifest, (u8, Value)> {
    let bytes = crate::transform_command::read_bounded(path, DOCUMENT_LIMIT)
        .map_err(|error| super::io_failure(&error))?;
    if crate::transform_command::over(&bytes, DOCUMENT_LIMIT) {
        return Err(refused("manifest-too-large"));
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let parsed = (|| -> Result<(String, VocabularySpec, Vec<Listed>), String> {
        if !bytes.is_ascii() {
            return Err("manifest-not-ascii".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "manifest-json")?;
        let object = value.as_object().ok_or("manifest-shape")?;
        only_keys(
            object,
            &["schema", "family", "vocabulary", "instances"],
            "manifest",
        )?;
        if object.get("schema").and_then(Value::as_str) != Some(MANIFEST_SCHEMA) {
            return Err("manifest-schema".into());
        }
        let family = object
            .get("family")
            .and_then(Value::as_str)
            .filter(|f| {
                !f.is_empty()
                    && f.len() <= 64
                    && f.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
            .ok_or("manifest-family")?
            .to_owned();
        let vocabulary = vocabulary_spec(object.get("vocabulary").ok_or("manifest-vocabulary")?)?;
        let instances = object
            .get("instances")
            .and_then(Value::as_array)
            .ok_or("manifest-instances")?;
        if instances.is_empty() || instances.len() > MAX_INSTANCES {
            return Err("manifest-instance-count".into());
        }
        let mut out = Vec::new();
        for instance in instances {
            let entry = instance.as_object().ok_or("manifest-instance-shape")?;
            only_keys(
                entry,
                &["name", "parameters", "program", "optimize", "fit"],
                "instance",
            )?;
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty() && n.len() <= 128 && n.is_ascii())
                .ok_or("instance-name")?
                .to_owned();
            let parameters = integer_map(
                entry.get("parameters").ok_or("instance-parameters")?,
                "parameter",
            )?;
            let program = relative(
                base,
                entry
                    .get("program")
                    .and_then(Value::as_str)
                    .ok_or("instance-program")?,
            )?;
            let optimize = entry
                .get("optimize")
                .map(|v| {
                    v.as_str()
                        .ok_or("instance-optimize")
                        .map_err(String::from)
                        .and_then(|t| relative(base, t))
                })
                .transpose()?;
            let fit = match entry.get("fit") {
                None => true,
                Some(value) => value.as_bool().ok_or("instance-fit")?,
            };
            out.push((name, parameters, program, optimize, fit));
        }
        Ok((family, vocabulary, out))
    })();
    let (family, vocabulary, listed) = parsed.map_err(|reason| refused(&reason))?;
    let mut instances = Vec::new();
    for (name, parameters, program, optimize, fit) in listed {
        let program = crate::transform_command::read_bounded(&program, PROGRAM_LIMIT)
            .map_err(|error| super::io_failure(&error))?;
        if crate::transform_command::over(&program, PROGRAM_LIMIT) {
            return Err(refused("program-too-large"));
        }
        let optimize_nodes = match optimize {
            None => None,
            Some(path) => {
                let bytes = crate::transform_command::read_bounded(&path, PROGRAM_LIMIT)
                    .map_err(|error| super::io_failure(&error))?;
                import_program(&bytes).ok().map(|p| p.nodes().len())
            }
        };
        instances.push(Instance {
            name,
            parameters,
            program,
            optimize_nodes,
            fit,
        });
    }
    Ok(Manifest {
        family,
        vocabulary,
        instances,
    })
}

/// An instance's subject, or why there is none.
fn instance_subject(manifest: &Manifest, instance: &Instance) -> Result<Subject, String> {
    let program = import_program(&instance.program)
        .map_err(|error| format!("instance-not-admitted:{error:?}"))?;
    let vocab = vocabulary(&manifest.vocabulary, &program)?;
    Subject::new(
        program,
        vocab,
        instance.parameters.clone(),
        manifest.vocabulary.cases,
        DOOR_MAX_TUPLES,
    )
    .map_err(|refusal| refusal.0)
}

// --- the family door -------------------------------------------------------------

struct Row {
    json: Value,
    verdict: String,
    holes: Option<BTreeMap<usize, Fill>>,
    parameters: BTreeMap<String, i128>,
    nodes: Option<usize>,
    optimize_nodes: Option<usize>,
    fit: bool,
}

fn judge_instances(
    manifest: &Manifest,
    outputs: &[Expr],
    stages: &mut Stages,
    fitted: Option<&BTreeMap<String, bool>>,
) -> Vec<Row> {
    let mut order: Vec<&Instance> = manifest.instances.iter().collect();
    // Smallest domain first, then by name.
    order.sort_by_key(|instance| {
        let size = import_program(&instance.program)
            .map(|p| tuples(p.inputs()))
            .unwrap_or(u128::MAX);
        (size, instance.name.clone())
    });
    let holes = outputs.iter().any(has_holes);
    let mut rows = Vec::new();
    for instance in order {
        let mut row = json!({"instance": instance.name, "parameters": instance.parameters.iter().map(|(name, value)| (name.clone(), crate::report_numbers::integer(*value))).collect::<Map<_, _>>()});
        if let Some(fitted) = fitted {
            row["fitted"] = json!(fitted.get(&instance.name).copied().unwrap_or(false));
        }
        let mut out = Row {
            json: Value::Null,
            verdict: String::from("refused"),
            holes: None,
            parameters: instance.parameters.clone(),
            nodes: None,
            optimize_nodes: instance.optimize_nodes,
            fit: instance.fit,
        };
        let prepared = if holes {
            stages.run(true, || {
                instance_subject(manifest, instance).map(|subject| {
                    let prepared = family::prepare(outputs, &subject, &sketch::Settings::default());
                    (subject, prepared)
                })
            })
        } else {
            Ok(instance_subject(manifest, instance).map(|subject| {
                let prepared = family::prepare(outputs, &subject, &sketch::Settings::default());
                (subject, prepared)
            }))
        };
        let (subject, prepared) = match prepared {
            Err(reason) => {
                row["verdict"] = json!("inconclusive");
                row["reason"] = json!(reason);
                out.verdict = String::from("inconclusive");
                out.json = row;
                rows.push(out);
                continue;
            }
            Ok(Err(reason)) => {
                row["verdict"] = json!("refused");
                row["reason"] = json!(reason);
                out.json = row;
                rows.push(out);
                continue;
            }
            Ok(Ok(pair)) => pair,
        };
        row["tuples"] = json!(subject.size);
        row["optimize_nodes"] = json!(instance.optimize_nodes);
        if let Some(result) = &prepared.sketch {
            row["sketch"] = sketch_json(&subject, result);
            out.holes = Some(result.holes.clone());
            if result.status != SketchStatus::Completed {
                // An unrealizable sketch is the doors' claim, not a checker
                // counterexample: it is labelled as such and never "refuted".
                let verdict = result.status.name();
                row["verdict"] = json!(verdict);
                row["reason"] = json!(result.reason);
                out.verdict = verdict.to_owned();
                out.json = row;
                rows.push(out);
                continue;
            }
        }
        let Some(program) = &prepared.program else {
            row["verdict"] = json!("refused");
            row["reason"] = json!(prepared.refusal.map(|r| r.0));
            out.json = row;
            rows.push(out);
            continue;
        };
        let shown = match &prepared.sketch {
            Some(result) => &result.expressions,
            None => &prepared.bound,
        };
        row["expressions"] = texts(&subject, shown);
        row["program"] = program_json(program);
        out.nodes = Some(program.nodes().len());
        let check = match bytes_of(program) {
            Some(bytes) => checked(stages, &instance.program, &bytes, Some(&subject)),
            None => json!({"verdict": "refused", "reason": "encoding"}),
        };
        let verdict = if accepted(&check) {
            "accepted"
        } else if check["verdict"] == "counterexample" {
            "refuted"
        } else {
            "inconclusive"
        };
        row["verdict"] = json!(verdict);
        row["check"] = check;
        out.verdict = verdict.to_owned();
        out.json = row;
        rows.push(out);
    }
    rows
}

fn summary(rows: &[Row]) -> Value {
    let accepted: Vec<&Row> = rows.iter().filter(|r| r.verdict == "accepted").collect();
    // Only a checker counterexample refutes; an unrealizable sketch on some
    // instance is reported as such.
    let failing = rows.iter().find(|r| r.verdict == "refuted");
    let unrealizable = rows.iter().filter(|r| r.verdict == "unrealizable").count();
    let verdict = if accepted.len() == rows.len() {
        "accepted"
    } else if failing.is_some() {
        "refuted"
    } else if unrealizable > 0 {
        "unrealizable"
    } else {
        "partial"
    };
    json!({
        "verdict": verdict,
        "instances": rows.len(),
        "accepted": accepted.len(),
        "unrealizable": unrealizable,
        "coverage": format!("checked on these {} instances", rows.len()),
        "nodes": accepted.iter().filter_map(|r| r.nodes).sum::<usize>(),
        "optimize_nodes": accepted.iter().filter_map(|r| r.optimize_nodes).sum::<usize>(),
        "smaller_than_optimize": accepted.iter().filter(|r| matches!((r.nodes, r.optimize_nodes), (Some(n), Some(o)) if n < o)).count(),
        "rows": rows.iter().map(|r| r.json.clone()).collect::<Vec<_>>(),
        "smallest_failing": failing.map(|r| r.json.clone()),
    })
}

fn family_command(instances: &Path, program: &Path, fit: bool) -> (u8, Value) {
    let started = Instant::now();
    let manifest = match read_manifest(instances) {
        Ok(manifest) => manifest,
        Err(failure) => return failure,
    };
    let document = match read_document(program) {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    if !document.parameters.is_empty() {
        return refused("family-program-takes-no-parameter-values");
    }
    if let Some(spec) = &document.vocabulary
        && (spec.inputs != manifest.vocabulary.inputs
            || spec.variants != manifest.vocabulary.variants)
    {
        return refused("family-program-vocabulary-differs-from-the-manifest");
    }
    let outputs = match expressions_of(&document.outputs, Allow::ALL) {
        Ok(outputs) => outputs,
        Err(reason) => return refused(&reason),
    };
    let declared: std::collections::BTreeSet<&String> = manifest
        .instances
        .iter()
        .flat_map(|i| i.parameters.keys())
        .collect();
    for node in &outputs {
        if let Some(unknown) = params_of(node).iter().find(|p| !declared.contains(p)) {
            return refused(&format!("unknown-parameter:{unknown}"));
        }
    }
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    let rows = judge_instances(&manifest, &outputs, &mut stages, None);
    let mut detail = summary(&rows);
    detail["family"] = json!(manifest.family);
    let hole_count: usize = outputs
        .iter()
        .map(|n| zeno_fcis_loop_doors::expr::holes_of(n).len())
        .sum();
    if fit && hole_count > 0 {
        let samples: Vec<(BTreeMap<String, i128>, BTreeMap<usize, Fill>)> = rows
            .iter()
            .filter(|r| r.fit && r.json["sketch"]["status"] == "completed")
            .filter_map(|r| r.holes.clone().map(|h| (r.parameters.clone(), h)))
            .collect();
        let forms = family::fit(&samples);
        detail["fitted_on"] = json!(
            rows.iter()
                .filter(|r| r.fit && r.json["sketch"]["status"] == "completed")
                .map(|r| r.json["instance"].clone())
                .collect::<Vec<_>>()
        );
        detail["fits"] = forms
            .iter()
            .map(|(k, form)| (k.to_string(), form_json(form.as_ref())))
            .collect::<Map<_, _>>()
            .into();
        if !forms.is_empty() && forms.len() == hole_count && forms.values().all(Option::is_some) {
            let lifted: Result<Vec<Expr>, _> = outputs
                .iter()
                .map(|n| family::apply_forms(n, &forms))
                .collect();
            match lifted {
                Ok(lifted) => {
                    detail["lifted"] = Value::from(
                        lifted
                            .iter()
                            .map(zeno_fcis_loop_doors::expr::print_symbolic)
                            .collect::<Vec<_>>(),
                    );
                    let fitted: BTreeMap<String, bool> = rows
                        .iter()
                        .map(|r| {
                            (
                                r.json["instance"].as_str().unwrap_or_default().to_owned(),
                                r.fit && r.json["sketch"]["status"] == "completed",
                            )
                        })
                        .collect();
                    if lifted.iter().any(has_holes) {
                        detail["lifted_check"] = json!({"verdict": "not-checked", "reason": "the lifted program keeps choice holes"});
                    } else {
                        let again = judge_instances(&manifest, &lifted, &mut stages, Some(&fitted));
                        detail["lifted_check"] = summary(&again);
                        detail["lifted_check"]["unfitted_instances_checked"] =
                            json!(again.iter().filter(|r| r.json["fitted"] == false).count());
                        detail["lifted_check"]["unfitted_instances_accepted"] = json!(
                            again
                                .iter()
                                .filter(|r| r.json["fitted"] == false && r.verdict == "accepted")
                                .count()
                        );
                    }
                }
                Err(refusal) => {
                    detail["lifted_check"] = json!({"verdict": "refused", "reason": refusal.0})
                }
            }
        }
    }
    let status = detail["verdict"].as_str().unwrap_or("partial").to_owned();
    let exit = match status.as_str() {
        "accepted" => crate::OK,
        "refuted" => crate::INVALID,
        _ => crate::BLOCKED,
    };
    doors_result(exit, "family", &status, detail)
}

fn form_json(form: Option<&Form>) -> Value {
    match form {
        None => Value::Null,
        Some(Form::Constant(value)) => {
            json!({"constant": crate::report_numbers::integer(*value), "text": value.to_string()})
        }
        Some(form @ Form::Linear { parameter, a, b }) => {
            json!({"parameter": parameter, "a": crate::report_numbers::integer(*a), "b": crate::report_numbers::integer(*b), "text": form.text()})
        }
    }
}

// --- the meaning key ---------------------------------------------------------------

fn evaluate_command(original: &Path, expressions: &Path, input: &[i128]) -> (u8, Value) {
    use zeno_fcis_loop_doors::expr::kind_of;
    use zeno_fcis_synthesis::finite::{Domain, Op};
    let bytes = match crate::transform_command::read_bounded(original, PROGRAM_LIMIT) {
        Ok(bytes) if !crate::transform_command::over(&bytes, PROGRAM_LIMIT) => bytes,
        Ok(_) => return refused("program-too-large"),
        Err(error) => return super::io_failure(&error),
    };
    let original = match import_program(&bytes) {
        Ok(program) => program,
        Err(error) => return refused(&format!("original-not-admitted:{error:?}")),
    };
    if tuples(original.inputs()) > u128::from(DOOR_MAX_TUPLES) {
        return refused("domain-over-budget");
    }
    let document = match read_document(expressions) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if document.outputs.len() != 1 || !document.parameters.is_empty() {
        return refused("evaluate-needs-one-plain-expression");
    }
    let Some(spec) = document.vocabulary else {
        return refused("document-vocabulary-missing");
    };
    let initial_vocab = match vocabulary(&spec, &original) {
        Ok(v) => v,
        Err(e) => return refused(&e),
    };
    let node = match parse(&document.outputs[0], 0, Allow::PLAIN)
        .and_then(|n| initial_vocab.resolve_variants(&n))
    {
        Ok(node) => node,
        Err(e) => return refused(&e.0),
    };
    let kind = match kind_of(&node, &initial_vocab) {
        Ok(k) => k.unwrap_or(Kind::Int),
        Err(e) => return refused(&e.0),
    };
    let (output, zero) = if kind == Kind::Bool {
        (Domain::Bool, Op::Bool(false))
    } else {
        (
            Domain::Int {
                min: i128::MIN,
                max: i128::MAX,
            },
            Op::Int(0),
        )
    };
    let seed = match Program::try_new(
        original.inputs().to_vec(),
        vec![output],
        vec![zero],
        vec![0],
    ) {
        Ok(p) => p,
        Err(e) => return refused(&format!("evaluate-not-admitted:{e:?}")),
    };
    let vocab = match vocabulary(&spec, &seed) {
        Ok(v) => v,
        Err(e) => return refused(&e),
    };
    let program = match lower_outputs(&[node], &vocab, &seed) {
        Ok(p) => p,
        Err(e) => return refused(&e.0),
    };
    match program.evaluate(input) {
        Ok(values) => super::result(
            crate::OK,
            "evaluated",
            json!({"values": values.iter().map(ToString::to_string).collect::<Vec<_>>()}),
        ),
        Err(error) => refused(&format!("evaluation-refused:{error:?}")),
    }
}

/// A program's meaning key: a digest of its input and output domains and of
/// its observation on every declared input, in the checker's order: every
/// output, or the evaluator's exact failure. Observations come from the
/// library's verified V2 evaluator with the checker's full step budget, as
/// `transform check` observes them, so two programs have equal keys exactly
/// when `transform check` finds them equivalent (within its limits).
fn meaning_key(program: &Program) -> String {
    let limits = zeno_fcis_synthesis::finite::v2_zero_limits().with_limit(
        zeno_fcis_synthesis::finite::V2Resource::Step,
        crate::finite_checker::FULL_BUDGET,
    );
    let shown = program_to_json(program);
    let header = json!({
        "profile": MEANING_PROFILE,
        "inputs": shown["inputs"],
        "outputs": shown["outputs"],
    });
    let mut digests = header.to_string().into_bytes();
    let mut block: Vec<u8> = Vec::with_capacity(MEANING_BLOCK + 256);
    let mut tuple = crate::finite_checker::first_tuple(program.inputs());
    let mut count: u64 = 0;
    loop {
        match program.execute_v2(&tuple, limits).into_parts().0 {
            Ok(values) => {
                block.push(0);
                for value in values {
                    block.extend_from_slice(&value.to_be_bytes());
                }
            }
            Err(failure) => {
                let text = format!("{failure:?}");
                block.push(1);
                block.extend_from_slice(
                    &u64::try_from(text.len()).unwrap_or(u64::MAX).to_be_bytes(),
                );
                block.extend_from_slice(text.as_bytes());
            }
        }
        count += 1;
        if block.len() >= MEANING_BLOCK {
            digests.extend_from_slice(sha256_hex(&block).as_bytes());
            block.clear();
        }
        if !crate::finite_checker::advance(program.inputs(), &mut tuple) {
            break;
        }
    }
    digests.extend_from_slice(sha256_hex(&block).as_bytes());
    digests.extend_from_slice(&count.to_be_bytes());
    sha256_hex(&digests)
}

/// `loop meaning`: the meaning keys of a program and of its original.
fn meaning_command(
    original: &Path,
    expressions: Option<&Path>,
    candidate: Option<&Path>,
) -> (u8, Value) {
    let started = Instant::now();
    let read = |path: &Path| match crate::transform_command::read_bounded(path, PROGRAM_LIMIT) {
        Ok(bytes) if crate::transform_command::over(&bytes, PROGRAM_LIMIT) => {
            Err(refused("program-too-large"))
        }
        Ok(bytes) => Ok(bytes),
        Err(error) => Err(super::io_failure(&error)),
    };
    let original_program = match read(original).map(|bytes| import_program(&bytes)) {
        Ok(Ok(program)) => program,
        Ok(Err(error)) => return refused(&format!("original-not-admitted:{error:?}")),
        Err(failure) => return failure,
    };
    let size = tuples(original_program.inputs());
    if size > u128::from(DOOR_MAX_TUPLES) {
        return refused("domain over the door's budget");
    }
    let program = match (expressions, candidate) {
        (Some(path), _) => {
            let document = match read_document(path) {
                Ok(document) => document,
                Err(failure) => return failure,
            };
            let prepared = prepare_expressions(&document, &original_program);
            match prepared.program {
                Some(program) => program,
                None => {
                    return refused(prepared.detail["reason"].as_str().unwrap_or("refused"));
                }
            }
        }
        (None, Some(path)) => match read(path).map(|bytes| import_program(&bytes)) {
            Ok(Ok(program)) => program,
            Ok(Err(error)) => return refused(&format!("candidate-not-admitted:{error:?}")),
            Err(failure) => return failure,
        },
        (None, None) => return refused("no program"),
    };
    // `transform check` refuses a pair whose domains differ; so does the key,
    // before enumerating anything.
    if program.inputs() != original_program.inputs()
        || program.outputs() != original_program.outputs()
    {
        return refused("candidate-domains-differ-from-the-original");
    }
    let shown = program_json(&program);
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    match stages.run(true, || {
        (meaning_key(&original_program), meaning_key(&program))
    }) {
        Err(reason) => doors_result(
            crate::BLOCKED,
            "meaning",
            "inconclusive",
            json!({"reason": reason}),
        ),
        Ok((original_key, key)) => doors_result(
            crate::OK,
            "meaning",
            "keyed",
            json!({
                "profile": MEANING_PROFILE,
                "key": key,
                "original_key": original_key,
                "same_as_original": key == original_key,
                "tuples": size.to_string(),
                "program": shown,
                "meaning": "equal keys exactly when transform check finds the programs equivalent, within its limits",
            }),
        ),
    }
}

// --- the contract doors: question, teach-back, decompose -----------------------------

/// A bounded JSON document.
fn json_document(path: &Path) -> Result<Value, (u8, Value)> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|_| refused("document-not-json"))
}

/// The core table of `dir`, in a contained search stage.
fn contract_core(
    dir: &Path,
    stages: &mut Stages,
    question_profile: bool,
) -> Result<judges::Core, (u8, Value)> {
    match stages.run(true, || judges::Core::load(dir, question_profile)) {
        Err(reason) => Err(doors_result(
            crate::BLOCKED,
            "contract",
            "inconclusive",
            json!({"reason": reason}),
        )),
        Ok(Err(stop)) => Err(stop_result(stop)),
        Ok(Ok(core)) => Ok(core),
    }
}

fn stop_result(stop: judges::Stop) -> (u8, Value) {
    match stop {
        judges::Stop::Invalid(reason) => refused(&reason),
        judges::Stop::OutOfScope(reason) => super::result(
            crate::BLOCKED,
            "out-of-scope",
            json!({"reason": reason,
                "determination": crate::contract::review::decision_space::determination_scope(true, "declared-laws-and-recorded-answers")}),
        ),
        judges::Stop::Inconclusive(reason) => {
            super::result(crate::BLOCKED, "inconclusive", json!({"reason": reason}))
        }
    }
}

/// `loop question`.
fn question_command(dir: &Path, answers: Option<&Path>) -> (u8, Value) {
    let started = Instant::now();
    let document = match answers.map(json_document).transpose() {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    let core = match contract_core(dir, &mut stages, true) {
        Ok(core) => core,
        Err(failure) => return failure,
    };
    let answers = match document.map(|d| core.answers(&d)).transpose() {
        Ok(answers) => answers.unwrap_or_default(),
        Err(stop) => return stop_result(stop),
    };
    let mut detail = match stages.run(true, || judges::question(&core, &answers)) {
        Err(reason) => {
            return super::result(crate::BLOCKED, "inconclusive", json!({"reason": reason}));
        }
        Ok(Err(stop)) => return stop_result(stop),
        Ok(Ok(detail)) => detail,
    };
    if detail["status"] == "conflict" {
        return (
            crate::INVALID,
            json!({"status": "conflict", "authority": "none", "detail": detail}),
        );
    }
    // The answers' laws, checked by the gaps judge on the core's decisions.
    if !answers.is_empty() {
        let (lines, owners) = match core.pinned_lines(&answers) {
            Ok(pinned) => pinned,
            Err(stop) => return stop_result(stop),
        };
        let gaps = stages.run(true, || {
            crate::contract::review::gaps::judge(
                core.project(),
                core.rules_text(),
                crate::contract::review::gaps::Options {
                    max_tuples: judges::MAX_TUPLES,
                    max_work: crate::contract::review::gaps::DEFAULT_MAX_WORK,
                    proposals: Some(&lines),
                    // The question reports only determination and the
                    // proposals' check, so the mutants are not bound.
                    mutants: false,
                },
            )
        });
        use crate::contract::review::gaps::Gaps;
        detail["laws_checked_by"] = json!("contract review --gaps (proposed laws)");
        match gaps {
            Err(reason) => {
                return super::result(
                    crate::BLOCKED,
                    "inconclusive",
                    json!({"reason": reason, "detail": detail}),
                );
            }
            Ok(Err(error)) => return refused(&format!("{}: {}", error.place(), error.reason())),
            Ok(Ok(Gaps::Measured(measured))) => {
                detail["gaps"] = json!({
                    "status": "measured",
                    "determined": measured["proposals"]["determined"],
                    "inactive": measured["proposals"]["inactive"],
                    "determination": measured["proposals"]["after"]["determination"],
                });
            }
            Ok(Ok(Gaps::FalseAlarm(alarm))) => {
                let law = alarm["false_alarm"]["law"]
                    .as_u64()
                    .and_then(|id| u32::try_from(id).ok());
                let owner = law.and_then(|id| owners.get(&id).copied());
                let agrees = owner.is_some_and(|k| detail["answers"][k]["status"] == "recorded");
                if owner.is_none() || agrees {
                    // A law pinned from an answer that agrees with the core
                    // cannot fail on the core: a defect, never a verdict.
                    return refused("pinned-law-false-alarm-on-an-agreeing-answer");
                }
                if let Some(k) = owner {
                    detail["answers"][k]["false_alarm"] = alarm["false_alarm"].clone();
                    let position = owners
                        .iter()
                        .filter(|(_, answer)| **answer == k)
                        .position(|(id, _)| Some(*id) == law);
                    if let Some((scope, formula)) = position
                        .and_then(|position| core.pin(&answers[k]).ok()?.into_iter().nth(position))
                    {
                        let origin = json!({"answer": k, "scope": scope, "formula": formula, "synthetic_id": law});
                        detail["answers"][k]["false_alarm"]["law_origin"] = origin.clone();
                        detail["gaps"]["law_origin"] = origin;
                    }
                }
                detail["gaps"]["status"] = json!("proposal-false-alarm");
                detail["gaps"]["law"] = json!(law);
            }
            Ok(Ok(other)) => {
                return super::result(
                    crate::BLOCKED,
                    "inconclusive",
                    json!({"reason": other.status(), "gaps": other.json(), "detail": detail}),
                );
            }
        }
    }
    let status = detail["status"].as_str().unwrap_or("open").to_owned();
    (
        if status == "inconsistent" {
            crate::INVALID
        } else if status == "incomplete" {
            crate::BLOCKED
        } else {
            crate::OK
        },
        json!({"status": status, "authority": "none", "detail": detail}),
    )
}

/// `loop teach-back`.
fn teach_back_command(dir: &Path, rules: &Path, answers: Option<&Path>) -> (u8, Value) {
    let started = Instant::now();
    let rules_document = match json_document(rules) {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    let answers_document = match answers.map(json_document).transpose() {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    let core = match contract_core(dir, &mut stages, false) {
        Ok(core) => core,
        Err(failure) => return failure,
    };
    let rules = match core.rules(&rules_document) {
        Ok(rules) => rules,
        Err(stop) => return stop_result(stop),
    };
    let answers = match answers_document.map(|d| core.answers(&d)).transpose() {
        Ok(answers) => answers.unwrap_or_default(),
        Err(stop) => return stop_result(stop),
    };
    // The verdicts need no admissibility table: when computing `required`
    // does not finish in its stage, the rules are judged again without it.
    let judged = match stages.run(true, || judges::teach_back(&core, &rules, &answers, true)) {
        Err(_) => stages.run(true, || judges::teach_back(&core, &rules, &answers, false)),
        done => done,
    };
    match judged {
        Err(reason) => super::result(crate::BLOCKED, "inconclusive", json!({"reason": reason})),
        Ok(Err(stop)) => stop_result(stop),
        Ok(Ok(detail)) => (
            crate::OK,
            json!({"status": "judged", "authority": "none", "detail": detail}),
        ),
    }
}

/// `loop decompose`: steps 1 to 3 (the door's own evaluation) and, with
/// `--compose`, step 4 (the composition checker's verdict, combined with
/// steps 1 to 3 as the weaker of the two).
fn decompose_command(
    dir: &Path,
    architecture: &Path,
    compose: bool,
    export: Option<&Path>,
) -> (u8, Value) {
    let started = Instant::now();
    let document = match json_document(architecture) {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    let core = match contract_core(dir, &mut stages, false) {
        Ok(core) => core,
        Err(failure) => return failure,
    };
    let parts = match decompose::parts(&core, &document) {
        Ok(parts) => parts,
        Err(stop) => return stop_result(stop),
    };
    let (mut detail, tables) =
        match stages.run(true, || decompose::decompose_with_tables(&core, parts)) {
            Err(reason) => {
                return super::result(crate::BLOCKED, "inconclusive", json!({"reason": reason}));
            }
            Ok(Err(stop)) => return stop_result(stop),
            Ok(Ok(found)) => found,
        };
    if compose {
        match tables {
            None => {
                detail["compose"] = json!({"status": "not-run",
                    "reason": "every component needs a case table: steps 1 to 3 gave none"});
            }
            Some(tables) => match stages.run(true, || compose::compose(&core, &tables, export)) {
                Err(reason) => {
                    return super::result(
                        crate::BLOCKED,
                        "inconclusive",
                        json!({"reason": reason, "detail": detail}),
                    );
                }
                Ok(Err(stop)) => return stop_result(stop),
                Ok(Ok(composed)) => {
                    let equal = composed["equal_to_core"] == true;
                    let proved = composed["obligations"]["verdict"] == "proved";
                    let checked = if equal && proved {
                        "accepted"
                    } else if !equal {
                        "refuted"
                    } else {
                        "obligations-not-proved"
                    };
                    // The weaker of the two: a step-3 blame is never
                    // overridden, and `accepted` needs both.
                    let steps = detail["verdict"].as_str().unwrap_or("refused").to_owned();
                    detail["steps_verdict"] = json!(steps);
                    detail["compose_verdict"] = json!(checked);
                    detail["verdict_from"] = json!(
                        "the weaker of steps 1 to 3 (door-simulation) and the composed check (composition-checker)"
                    );
                    detail["verdict"] = json!(if steps == "refuted" {
                        "refuted"
                    } else {
                        checked
                    });
                    detail["compose"] = composed;
                }
            },
        }
    }
    let verdict = detail["verdict"].as_str().unwrap_or("refused").to_owned();
    let exit = match verdict.as_str() {
        "accepted" | "completed" | "meets-requirements" | "realizable" => crate::OK,
        _ => crate::INVALID,
    };
    (
        exit,
        json!({"status": verdict, "authority": "none", "detail": detail}),
    )
}

// --- the transfer door -------------------------------------------------------------

fn transfer_command(from: &Path, to_instances: &Path) -> (u8, Value) {
    let started = Instant::now();
    let manifest = match read_manifest(to_instances) {
        Ok(manifest) => manifest,
        Err(failure) => return failure,
    };
    let document = match read_document(from) {
        Ok(document) => document,
        Err(failure) => return failure,
    };
    if document.parameters.is_empty() {
        return refused("transfer-source-needs-its-parameter-values");
    }
    let outputs = match expressions_of(
        &document.outputs,
        Allow {
            holes: false,
            params: false,
            keep: false,
        },
    ) {
        Ok(outputs) => outputs,
        Err(reason) => return refused(&reason),
    };
    let mut stages = Stages::new(started, STATELESS_DEADLINE, 0);
    let mut rows = Vec::new();
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    let mut transferred = 0;
    for instance in &manifest.instances {
        let mut row = json!({"instance": instance.name, "parameters": instance.parameters.iter().map(|(name, value)| (name.clone(), crate::report_numbers::integer(*value))).collect::<Map<_, _>>()});
        let moved = stages.run(true, || {
            instance_subject(&manifest, instance).map(|subject| {
                let result = family::transfer(
                    &outputs,
                    &document.parameters,
                    &subject,
                    &sketch::Settings::default(),
                    &repair::Settings::default(),
                );
                (subject, result)
            })
        });
        let (subject, result) = match moved {
            Err(reason) => {
                row["status"] = json!("inconclusive");
                row["reason"] = json!(reason);
                rows.push(row);
                continue;
            }
            Ok(Err(reason)) => {
                row["status"] = json!("refused");
                row["reason"] = json!(reason);
                rows.push(row);
                continue;
            }
            Ok(Ok(pair)) => pair,
        };
        row["tuples"] = json!(subject.size);
        row["direct"] = match &result.direct {
            Direct::Agrees => json!({"verdict": "agrees"}),
            Direct::Differs(ordinal) => {
                json!({"verdict": "differs", "ordinal": ordinal, "input": point_json(&subject, *ordinal)})
            }
            Direct::Refused(refusal) => json!({"verdict": "refused", "reason": refusal.0}),
        };
        row["opened"] = json!(result.opened.len());
        if let Some(sketched) = &result.sketch {
            row["sketch"] = json!(sketched.status.name());
        }
        if !result.choices.is_empty() {
            row["choices"] = Value::from(
                result
                    .choices
                    .iter()
                    .map(|(k, from, to)| json!({"hole": k, "from": from, "to": to}))
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(repaired) = &result.repair {
            row["repair"] = json!({
                "proposed": repaired.status.name(),
                "steps": repaired.steps.len(),
            });
        }
        row["proposed"] = json!(result.status.name());
        match (&result.program, result.status) {
            (Some(program), status) if status != TransferStatus::Failed => {
                row["expressions"] = texts(&subject, &result.expressions);
                row["program"] = program_json(program);
                let check = match bytes_of(program) {
                    Some(bytes) => checked(&mut stages, &instance.program, &bytes, Some(&subject)),
                    None => json!({"verdict": "refused", "reason": "encoding"}),
                };
                if accepted(&check) {
                    transferred += 1;
                    *by.entry(status.name()).or_default() += 1;
                    row["status"] = json!(status.name());
                } else {
                    row["status"] = json!("not-confirmed-by-the-checker");
                }
                row["check"] = check;
            }
            _ => row["status"] = json!("failed"),
        }
        rows.push(row);
    }
    let status = if transferred == rows.len() {
        "transferred"
    } else if transferred == 0 {
        "failed"
    } else {
        "partial"
    };
    let exit = match status {
        "transferred" => crate::OK,
        "failed" => crate::INVALID,
        _ => crate::BLOCKED,
    };
    doors_result(
        exit,
        "transfer",
        status,
        json!({
            "family": manifest.family,
            "from": {"parameters": document.parameters},
            "transferred": transferred,
            "of": rows.len(),
            "by": by,
            "coverage": format!("checked on these {} instances", rows.len()),
            "rows": rows,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn knowledge_operation(
    paths: &[PathBuf],
    operation: &str,
    query: Option<&str>,
    have: Option<&str>,
    want: Option<&str>,
    task: &str,
    max_steps: usize,
    max_states: usize,
    limits: pipeline::SearchLimits,
) -> (u8, Value) {
    strategy_knowledge::command_layered(
        paths, operation, query, have, want, task, max_steps, max_states, limits,
    )
}

pub(super) fn knowledge_session(command: &DoorCommand) -> Option<(&Path, bool)> {
    match command {
        DoorCommand::Sketch { session, .. } => Some((session, false)),
        DoorCommand::Capture { command } => match command {
            capture::Command::Whole { .. } => None,
            capture::Command::Open { session, .. } => Some((session, true)),
            capture::Command::Packet { session }
            | capture::Command::Candidate { session, .. }
            | capture::Command::Status { session, .. } => Some((session, false)),
        },
        _ => None,
    }
}

/// Assessment-only routing: the original exact checker and receipt replay stay in the pipeline.
pub(crate) fn assessment_run(
    original: &[u8],
    specification: &Value,
    actions: &[crate::knowledge_assessment::Action],
    budget: &crate::knowledge_replay::TaskBudget,
) -> Result<Value, &'static str> {
    let mut proposal = specification.clone();
    proposal["schema"] = json!(pipeline::PIPELINE_SCHEMA);
    proposal["original_sha256"] = json!(sha256_hex(original));
    proposal["implementation_sha256"] = json!(pipeline::implementation());
    if proposal["program"] == "identity" {
        proposal["program"] =
            program_to_json(&import_program(original).map_err(|_| "kb-task-program")?);
    }
    let stages = pipeline::stages(
        proposal
            .get("stages")
            .unwrap_or(&json!(["canonical", "resubstitute"])),
    )
    .map_err(|_| "kb-task-stages")?;
    let task = pipeline::task(original.to_vec(), proposal).map_err(|_| "kb-task-proposal")?;
    let limits = pipeline::SearchLimits {
        max_tuples: budget.tuples,
        max_gate_pairs: budget.gate_pairs,
        max_search_work: budget.search_work,
        max_cells: 1_048_576,
        max_assignments: 128,
        max_terms: 32,
        max_checks: budget.checks,
        deadline_ms: budget.timeout_ms,
    };
    if !limits.valid() {
        return Err("kb-task-limits");
    }
    let mut counter = pipeline::Budget::new(limits);
    if !actions.is_empty() {
        counter.knowledge = Some(crate::loop_knowledge::LoopPolicy::from_actions(actions));
    }
    let mut run = pipeline::run(&task, &stages, &mut counter);
    search_limits::annotate("pipeline", run.status, &mut run.report);
    run.report["checker_limits"] = json!({"steps":crate::transform::DEFAULT_STEP_LIMIT,"input_tuples":crate::transform::DEFAULT_MAX_INPUT_TUPLES});
    Ok(json!({"schema":"zeno-fcis/kb-task-output/1","authority":"none","detail":run.report}))
}

/// Exact finite artifact replay for tier-one facts; no strategy or publication authority.
pub(crate) fn assessment_certificate(
    original: &[u8],
    candidate: &[u8],
) -> Result<Value, &'static str> {
    let op = import_program(original).map_err(|_| "kb-proof-original")?;
    let cp = import_program(candidate).map_err(|_| "kb-proof-candidate")?;
    let count = tuples(op.inputs());
    if count > 4096 {
        return Err("kb-proof-domain-budget");
    }
    let limits = crate::transform::Limits {
        steps: crate::transform::DEFAULT_STEP_LIMIT,
        input_tuples: 4096,
    };
    let check = match crate::transform::check(original, candidate, limits) {
        Ok(eq) => {
            let receipt = eq.receipt();
            let replay = crate::transform::replay(&receipt, original, candidate, 4096);
            json!({"verdict":"equivalent","replay":if replay==crate::transform::Replay::Matched{"matched"}else{"differs"},"receipt_sha256":sha256_hex(&receipt)})
        }
        Err(crate::transform::Rejection::Counterexample(c)) => {
            json!({"verdict":"counterexample","ordinal":c.ordinal,"input":crate::transform::values_json(&c.input),"original":observation(&c.original),"candidate":observation(&c.candidate)})
        }
        Err(_) => return Err("kb-artifact-inconclusive"),
    };
    let accepted = check["verdict"] == "equivalent" && check["replay"] == "matched";
    Ok(
        json!({"schema":"zeno-fcis/kb-task-output/1","authority":"none","detail":{"status":if accepted{"qualified"}else{"refuted"},"original":program_json(&op),"best":if accepted{program_json(&cp)}else{Value::Null},"best_check":if accepted{check.clone()}else{Value::Null},"candidates":[{"candidate":program_json(&cp),"check":check}],"execution_trace":["check-program","replay-receipt"],"checker_limits":{"steps":crate::transform::DEFAULT_STEP_LIMIT,"input_tuples":4096},"trace":[],"reason":Value::Null,"work":{"check_requests":1,"gate_pairs_reserved":u64::try_from(count*2).map_err(|_|"kb-work-overflow")?,"search_stage_requests":0,"control_frames":0,"capture_work_reserved":0,"capture_cells_reserved":0}}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_family_reports_keep_full_width_offsets_and_constants() {
        for value in [
            i128::MIN,
            i128::MAX,
            -(1i128 << 100),
            1i128 << 100,
            1i128 << 64,
            5,
        ] {
            let constant = form_json(Some(&Form::Constant(value)));
            assert_eq!(
                crate::report_numbers::read(&constant["constant"]),
                Some(value)
            );
            let linear = form_json(Some(&Form::Linear {
                parameter: "C".into(),
                a: 1,
                b: value,
            }));
            assert_eq!(crate::report_numbers::read(&linear["a"]), Some(1));
            assert_eq!(crate::report_numbers::read(&linear["b"]), Some(value));
            assert!(
                Form::Linear {
                    parameter: "C".into(),
                    a: 1,
                    b: value
                }
                .expr()
                .is_ok()
            );
            assert_eq!(
                linear["text"],
                Form::Linear {
                    parameter: "C".into(),
                    a: 1,
                    b: value
                }
                .text()
            );
        }
    }

    #[test]
    fn evaluate_uses_i128_inputs_and_full_width_arithmetic_outputs() {
        use zeno_fcis_synthesis::finite::{Domain, Op};
        let wide = 1i128 << 100;
        let program = Program::try_new(
            vec![Domain::Int {
                min: wide,
                max: wide,
            }],
            vec![Domain::Int {
                min: 0,
                max: i128::MAX,
            }],
            vec![Op::Input(0)],
            vec![0],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let scratch = std::env::temp_dir().join(format!(
            "loop-evaluate-wide-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&scratch).unwrap_or_else(|e| panic!("{e:?}"));
        let original = scratch.join("original.zcve");
        let expressions = scratch.join("expressions.json");
        std::fs::write(
            &original,
            encode(&program).unwrap_or_else(|e| panic!("{e:?}")),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        for (expression, expected) in [
            ("mul_div_floor(x, 3, 2)", (wide * 3 / 2).to_string()),
            ("products_ge(x, 3, x, 2)", "1".to_owned()),
        ] {
            std::fs::write(
                &expressions,
                json!({
                    "schema":"zeno-fcis/loop-expressions/1", "vocabulary":{"inputs":["x"]},
                    "outputs":[expression],
                })
                .to_string(),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            let (status, value) = evaluate_command(&original, &expressions, &[wide]);
            assert_eq!(status, crate::OK, "{value}");
            assert_eq!(value["detail"]["values"], json!([expected]));
        }
        std::fs::remove_dir_all(scratch).unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn documents_are_bounded_and_fail_closed() {
        let good = br#"{"schema":"zeno-fcis/loop-expressions/1","vocabulary":{"inputs":["x"]},"outputs":["x"]}"#;
        assert!(document(good).is_ok());
        for bad in [
            &br#"{"schema":"zeno-fcis/loop-expressions/2","outputs":["x"]}"#[..],
            br#"{"schema":"zeno-fcis/loop-expressions/1","outputs":[]}"#,
            br#"{"schema":"zeno-fcis/loop-expressions/1","outputs":["x"],"extra":1}"#,
            br#"{"schema":"zeno-fcis/loop-expressions/1","outputs":[1]}"#,
            br#"{"schema":"zeno-fcis/loop-expressions/1","outputs":["x"],"vocabulary":{"inputs":["x"],"more":1}}"#,
            br#"{"schema":"zeno-fcis/loop-expressions/1","outputs":["x"],"parameters":{"C":"one"}}"#,
            "{\"schema\":\"zeno-fcis/loop-expressions/1\",\"outputs\":[\"x\u{e9}\"]}".as_bytes(),
        ] {
            assert!(document(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
        let many = format!(
            r#"{{"schema":"zeno-fcis/loop-expressions/1","outputs":[{}]}}"#,
            vec!["\"x\""; MAX_STATEMENTS + 1].join(",")
        );
        assert!(document(many.as_bytes()).is_err());
        assert!(relative(Path::new("/m"), "../x").is_err());
        assert!(relative(Path::new("/m"), "/etc/x").is_err());
        assert!(relative(Path::new("/m"), "a/b.zcve").is_ok());
    }
}
