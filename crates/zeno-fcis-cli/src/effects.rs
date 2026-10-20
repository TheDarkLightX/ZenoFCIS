//! Effect declarations are checked exhaustively against each clap enum.
use crate::{Value, checker_process, json};

#[derive(Clone, Copy)]
enum Effect {
    Group,
    Declared(
        &'static [&'static str],
        &'static [&'static str],
        bool,
        Option<&'static str>,
    ),
}
fn declared(
    (reads, writes, tools, flag): (
        &'static [&'static str],
        &'static [&'static str],
        bool,
        Option<&'static str>,
    ),
) -> Effect {
    Effect::Declared(reads, writes, tools, flag)
}
trait Declarations: Sized {
    // This typed match checks every enum variant; the same entries form lookup.
    const CASES: fn(&Self) -> Effect;
    fn lookup(path: &[&str]) -> Option<Effect>;
}
macro_rules! effects {
    ($ty:ty {
        leaves { $($name:literal => $pattern:pat => $effect:expr;)* }
        groups { $($group:literal => $group_pattern:pat => $group_field:ident : $group_type:ty;)* }
        flat { $($flat_pattern:pat => $flat_field:ident : $flat_type:ty;)* }
        optional { $($optional:literal => $optional_pattern:pat => $optional_field:ident : $optional_type:ty => $fallback:expr;)* }
    }) => {
        impl Declarations for $ty {
            const CASES: fn(&Self) -> Effect = |value| match value {
                $($pattern => $effect,)*
                $($group_pattern => (<$group_type as Declarations>::CASES)($group_field),)*
                $($flat_pattern => (<$flat_type as Declarations>::CASES)($flat_field),)*
                $($optional_pattern => match $optional_field {
                    Some(value) => (<$optional_type as Declarations>::CASES)(value),
                    None => $fallback,
                },)*
            };
            fn lookup(path: &[&str]) -> Option<Effect> {
                let _ = Self::CASES;
                if path.is_empty() { return Some(Effect::Group); }
                if let [name] = path {
                    let direct = match *name {
                        $($name => Some($effect),)*
                        $($optional => Some($fallback),)*
                        _ => None,
                    };
                    if direct.is_some() { return direct; }
                }
                $(if path.first() == Some(&$group) {
                    return <$group_type as Declarations>::lookup(&path[1..]);
                })*
                $(if path.first() == Some(&$optional) && path.len() > 1 {
                    return <$optional_type as Declarations>::lookup(&path[1..]);
                })*
                let found = None;
                $(let found = found.or_else(|| <$flat_type as Declarations>::lookup(path));)*
                found
            }
        }
    };
}

effects! { crate::Command {
    leaves {
        "describe" => Self::Describe { .. } => declared((&[], &[], false, None));
        "optimize" => Self::Optimize { .. } => declared((
                &[
                    "original-program",
                    "optional-strategy",
                    "optional-candidate-programs",
                ],
                &["optional-candidate-program", "optional-equivalence-receipt"],
                false,
                None,
            ));
        "new" => Self::New { .. } => declared((&["target-directory", "application-contract", "source-tree", "source-lock", "optional-cargo-cache"], &["project-files", "temporary-lock-workspace"], true, None));
        "check" => Self::Check { .. } => declared((&["project"], &[], false, None));
        "graph" => Self::Graph { .. } => declared((&["project"], &[], false, None));
        "explain" => Self::Explain { .. } => declared((&["project"], &[], false, None));
        "prove" => Self::Prove { .. } => declared((
                &["project", "tools-manifest", "toolchain-files"],
                &["temporary-files", "retained-evidence"],
                true,
                None,
            ));
        "counterexample" => Self::Counterexample { .. } => declared((
                &["project", "tools-manifest", "toolchain-files"],
                &["temporary-files", "retained-evidence"],
                true,
                None,
            ));
        "doctor" => Self::Doctor { .. } => declared((
                &["tools-manifest", "toolchain-files"],
                &["temporary-files"],
                true,
                None,
            ));
        "purity" => Self::Purity { .. } => declared((&["rust-source"], &[], false, None));
    }
    groups {
        "synth" => Self::Synth { command } => command: crate::synth::Command;
        "transform" => Self::Transform { command } => command: crate::transform_command::Command;
        "loop" => Self::Loop { command } => command: crate::loop_command::Command;
        "contract" => Self::Contract { command } => command: crate::ContractCommand;
        "backend" => Self::Backend { command } => command: crate::BackendCommand;
        "kb" => Self::Kb { operation } => operation: crate::knowledge_command::Operation;
    }
    flat {
    }
    optional {
        "generate" => Self::Generate { target, .. } => target: crate::GenerateTarget => declared((
                &["project", "generated-artifacts"],
                &["generated-artifacts"],
                false,
                Some("--check"),
            ));
    }
}}

effects! { crate::BackendCommand {
    leaves {
        "list" => Self::List => declared((&[], &[], false, None));
        "inspect" => Self::Inspect { .. } => declared((&["tools-manifest"], &[], false, None));
        "inventory-lean" => Self::InventoryLean { .. } => declared((&["toolchain-files"], &[], false, None));
        "verify" => Self::Verify { .. } => declared((
                &["tools-manifest", "toolchain-files"],
                &["temporary-files"],
                true,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::GenerateTarget {
    leaves {
        "contract" => Self::Contract { .. } => declared((
                &["application-contract", "generated-artifacts"],
                &["generated-artifacts"],
                false,
                Some("--check"),
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::synth::Command {
    leaves {
        "discover" => Self::Discover => declared((&[], &[], false, None));
        "run" => Self::Run { .. } => declared((
                &["synthesis-problem", "synthesis-artifacts"],
                &["synthesis-artifacts"],
                false,
                Some("--check"),
            ));
        "verify" => Self::Verify { .. } => declared((
                &[
                    "synthesis-problem",
                    "synthesis-artifacts",
                    "compiler-or-interpreter",
                ],
                &["temporary-files", "optional-conformance-receipt"],
                true,
                None,
            ));
    }
    groups {
        "completion" => Self::Completion { command } => command: crate::synth::completion::Command;
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::synth::completion::Command {
    leaves {
        "discover" => Self::Discover => declared((&[], &[], false, None));
        "find" => Self::Find { .. } => declared((&["completion-problem"], &["completion-case"], false, None));
        "verify" => Self::Verify { .. } => declared((
                &["completion-problem", "completion-evidence"],
                &[],
                false,
                None,
            ));
        "replay" => Self::Replay { .. } => declared((
                &["completion-problem", "completion-evidence"],
                &[],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::transform_command::Command {
    leaves {
        "check" => Self::Check { .. } => declared((
                &[
                    "original-program",
                    "candidate-program",
                    "optional-tools-manifest",
                    "optional-toolchain-files",
                    "optional-caller-cpu-limits",
                ],
                &[
                    "optional-equivalence-receipt",
                    "optional-symbolic-receipt",
                    "optional-query-directory",
                    "temporary-files",
                    "optional-transient-user-unit",
                ],
                true,
                None,
            ));
        "replay" => Self::Replay { .. } => declared((
                &[
                    "equivalence-receipt",
                    "original-program",
                    "candidate-program",
                ],
                &[],
                false,
                None,
            ));
        "check-certificates" => Self::CheckCertificates { .. } => declared((
                &[
                    "original-program",
                    "candidate-program",
                    "tools-manifest",
                    "toolchain-files",
                    "certificate-checker-manifest",
                    "certificate-checker-executable",
                    "certificate-directory",
                    "cli-executable",
                ],
                &[
                    "optional-certificate-receipt",
                    "optional-query-directory",
                    "temporary-files",
                ],
                true,
                None,
            ));
        "replay-certificates" => Self::ReplayCertificates { .. } => declared((
                &[
                    "certificate-receipt",
                    "original-program",
                    "candidate-program",
                    "tools-manifest",
                    "toolchain-files",
                    "certificate-checker-manifest",
                    "certificate-checker-executable",
                    "certificate-directory",
                    "cli-executable",
                ],
                &["temporary-files"],
                true,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::loop_command::Command {
    leaves {
        "open" => Self::Open { .. } => declared((
                &["original-program", "caller-cpu-limits"],
                &["session-directory", "transient-user-unit"],
                checker_process::loop_profile().is_some(),
                None,
            ));
        "candidate" => Self::Candidate { .. } => declared((
                &[
                    "session-directory",
                    "candidate-program",
                    "optional-expressions-document",
                    "caller-cpu-limits",
                ],
                &["session-directory", "transient-user-unit"],
                checker_process::loop_profile().is_some(),
                None,
            ));
        "run" => Self::Run { .. } => declared((
                &[
                    "session-directory",
                    "proposer-configuration",
                    "optional-seed-programs",
                    "caller-cpu-limits",
                ],
                &["session-directory", "transient-user-unit"],
                checker_process::loop_profile().is_some(),
                None,
            ));
        "resume" => Self::Resume { .. } => declared((
                &["session-directory", "caller-cpu-limits"],
                &["session-directory", "transient-user-unit"],
                checker_process::loop_profile().is_some(),
                None,
            ));
        "encode" => Self::Encode { .. } => declared((
                &["program-json", "caller-cpu-limits"],
                &["canonical-program", "transient-user-unit"],
                checker_process::loop_profile().is_some(),
                None,
            ));
    }
    groups {
    }
    flat {
        Self::Doors(command) => command: crate::loop_command::doors::DoorCommand;
    }
    optional {
    }
}}

effects! { crate::ContractCommand {
    leaves {
        "export-program" => Self::ExportProgram { .. } => declared((
                &["application-contract", "adopted-artifacts"],
                &["decision-program"],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
        Self::Review(command) => command: crate::review_command::Command;
        Self::Judge(command) => command: crate::judge_command::Command;
        Self::Composition(command) => command: crate::composition_command::Command;
        Self::Draft(command) => command: crate::draft_command::Command;
        Self::Adopt(command) => command: crate::contract_adopt::Command;
        Self::Diff(command) => command: crate::contract_diff::Command;
        Self::Evolve(command) => command: crate::contract_evolve::Command;
        Self::Symbolic(command) => command: crate::symbolic_command::Command;
        Self::Tau(command) => command: crate::tau_command::Command;
    }
    optional {
    }
}}

effects! { crate::review_command::Command {
    leaves {
        "review" => Self::Review { .. } => declared((
                &["application-contract", "decision-examples", "optional-proposed-laws"],
                &["optional-review-packet", "optional-seed-programs"],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::composition_command::Command {
    leaves {
        "check-composition" => Self::CheckComposition { .. } => declared((
                &[
                    "composition-manifest",
                    "application-contract",
                    "optional-aggregate-contract",
                    "optional-composition-receipt",
                ],
                &["optional-aggregate-contract", "optional-composition-receipt"],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::draft_command::Command {
    leaves {
    }
    groups {
        "draft" => Self::Draft { command } => command: crate::draft_command::Operation;
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::draft_command::Operation {
    leaves {
        "start" => Self::Start { .. } => declared((
                &["draft-session", "supplied-intent-rules-labels"],
                &["draft-session"],
                false,
                None,
            ));
        "propose" => Self::Propose { .. } => declared((
                &["draft-session", "supplied-intent-rules-labels"],
                &["draft-session"],
                false,
                None,
            ));
        "questions" => Self::Questions { .. } => declared((&["draft-session"], &["temporary-draft-lock"], false, None));
        "label" => Self::Label { .. } => declared((
                &["draft-session", "supplied-intent-rules-labels"],
                &["draft-session"],
                false,
                None,
            ));
        "check" => Self::Check { .. } => declared((&["draft-session"], &["temporary-draft-lock"], false, None));
        "finalize" => Self::Finalize { .. } => declared((
                &["draft-session"],
                &["new-draft-contract", "temporary-draft-lock"],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::contract_adopt::Command {
    leaves {
        "adopt" => Self::Adopt { .. } => declared((
                &[
                    "application-contract",
                    "candidate-program",
                    "equivalence-receipt",
                ],
                &[
                    "application-contract",
                    "adopted-artifacts",
                    "generated-artifacts",
                ],
                false,
                None,
            ));
        "refresh-receipts" => Self::RefreshReceipts { .. } => declared((
                &["application-contract", "adopted-artifacts"],
                &[
                    "application-contract",
                    "adopted-artifacts",
                    "generated-artifacts",
                ],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::contract_diff::Command {
    leaves {
        "diff" => Self::Diff { .. } => declared((
                &["application-contract", "adopted-artifacts"],
                &[],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::contract_evolve::Command {
    leaves {
        "evolve" => Self::Evolve { .. } => declared((
                &[
                    "application-contract",
                    "adopted-artifacts",
                    "evolved-artifacts",
                    "new-contract",
                ],
                &[
                    "application-contract",
                    "adopted-artifacts",
                    "evolved-artifacts",
                    "generated-artifacts",
                ],
                false,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::symbolic_command::Command {
    leaves {
        "check-symbolic" => Self::CheckSymbolic { .. } => declared((
                &[
                    "application-contract",
                    "optional-strengthening",
                    "optional-tools-manifest",
                    "optional-toolchain-files",
                ],
                &[
                    "optional-symbolic-report",
                    "optional-query-directory",
                    "temporary-files",
                ],
                true,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

effects! { crate::tau_command::Command {
    leaves {
        "export" => Self::Export { .. } => declared((
                &["application-contract", "optional-tau-import-metadata"],
                &["new-tau-query-directory"],
                true,
                None,
            ));
        "import" => Self::Import { .. } => declared((
                &["pointwise-tau-source", "explicit-width-scaling", "input-domain-limits"],
                &["new-contract-directory"],
                true,
                None,
            ));
    }
    groups {
    }
    flat {
    }
    optional {
    }
}}

pub(super) fn describe(path: &[String]) -> Value {
    let path: Vec<_> = path.iter().map(String::as_str).collect();
    match <crate::Command as Declarations>::lookup(&path)
        .unwrap_or_else(|| panic!("validated command path has no effect declaration: {path:?}"))
    {
        Effect::Group => json!({"classification": "command-group"}),
        Effect::Declared(reads, writes, executes_tools, read_only_flag) => {
            let mut result = json!({
                "classification": "declared", "reads": reads, "writes": writes,
                "executes_tools": executes_tools, "read_only_flag": read_only_flag,
            });
            if path.first() == Some(&"loop") && path.get(1) != Some(&"encode") {
                if let Some(reads) = result["reads"].as_array_mut() {
                    reads.push(json!("optional-knowledge-layers-and-session-binding"));
                }
                if let Some(writes) = result["writes"].as_array_mut() {
                    writes.extend([
                        json!("optional-private-knowledge-inbox"),
                        json!("optional-session-knowledge-binding"),
                    ]);
                }
                result["automatic_knowledge_disabled_by"] = json!("--no-kb");
            }
            result
        }
    }
}

effects! { crate::loop_command::doors::DoorCommand {
    leaves {
        "fuzz" => Self::Fuzz { .. } => declared((
                &["original-program", "candidate-program", "fuzz-request", "caller-cpu-limits"],
                &["transient-user-unit"],
                true,
                None,
            ));
        "pipeline" => Self::Pipeline { .. } => declared((
                &[
                    "original-program",
                    "optional-pipeline-proposal",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit", "optional-pipeline-export"],
                true,
                None,
            ));
        "strategy" => Self::Strategy { .. } => declared((
                &[
                    "strategy-task-manifest",
                    "task-original-programs",
                    // BEGIN typed strategy mode input effects.
                    "optional-strategy-problem",
                    "optional-case-reference-program",
                    // END typed strategy mode input effects.
                    // BEGIN structural knowledge strategy input effects.
                    "compiled-public-knowledge-example",
                    "optional-external-knowledge-file",
                    // END structural knowledge strategy input effects.
                    "caller-cpu-limits",
                ],
                &["transient-user-unit", "optional-strategy-export"],
                true,
                None,
            ));
        "rules" => Self::Rules { .. } => declared((
                &[
                    "original-program",
                    "optional-term-corpus",
                    "optional-rule-bank",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit", "optional-rule-export"],
                true,
                None,
            ));
        "learn" => Self::Learn { .. } => declared((
                &["learn-declaration-and-examples", "optional-native-reference", "caller-cpu-limits"],
                &["transient-user-unit", "optional-learn-export"],
                true,
                None,
            ));
        "sketch" => Self::Sketch { .. } => declared((
                &["session-directory", "sketch-document", "caller-cpu-limits"],
                &["session-directory", "transient-user-unit"],
                true,
                None,
            ));
        "family" => Self::Family { .. } => declared((
                &[
                    "family-manifest",
                    "family-program",
                    "instance-programs",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit"],
                true,
                None,
            ));
        "transfer" => Self::Transfer { .. } => declared((
                &[
                    "source-expressions",
                    "family-manifest",
                    "instance-programs",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit"],
                true,
                None,
            ));
        "meaning" => Self::Meaning { .. } => declared((
                &[
                    "original-program",
                    "optional-expressions-document",
                    "optional-candidate-program",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit"],
                true,
                None,
            ));
        "question" => Self::Question { .. } => declared((
                &[
                    "optional-application-contract",
                    "optional-version-space-specification",
                    "optional-owner-answers",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit"],
                true,
                None,
            ));
        "teach-back" => Self::TeachBack { .. } => declared((
                &[
                    "application-contract",
                    "loop-door-document",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit"],
                true,
                None,
            ));
        "decompose" => Self::Decompose { .. } => declared((
                &[
                    "application-contract",
                    "loop-door-document",
                    "caller-cpu-limits",
                ],
                &["transient-user-unit", "optional-composition-export"],
                true,
                None,
            ));
    }
    groups {
        "capture" => Self::Capture { command } => command: crate::loop_command::doors::capture::Command;
    }
    flat {

    }
    optional {}
}}

effects! { crate::loop_command::doors::capture::Command {
    leaves {
        "open" => Self::Open { .. } => declared((
                &[
                    "capture-spec",
                    "owner-reference-source",
                    "caller-cpu-limits",
                ],
                &[
                    "new-private-capture-session",
                    "transient-user-unit",
                    "bounded-scratch-file",
                ],
                true,
                None,
            ));
        "packet" => Self::Packet { .. } => declared((
                &["private-capture-session", "caller-cpu-limits"],
                &["transient-user-unit"],
                true,
                None,
            ));
        "candidate" => Self::Candidate { .. } => declared((
                &[
                    "private-capture-session",
                    "candidate-json-or-expressions",
                    "caller-cpu-limits",
                ],
                &[
                    "private-capture-session",
                    "exclusive-session-lock",
                    "transient-user-unit",
                ],
                true,
                None,
            ));
        "status" => Self::Status { .. } => declared((
                &["private-capture-session", "caller-cpu-limits"],
                &["transient-user-unit"],
                true,
                None,
            ));
    }
    groups {

    }
    flat {

    }
    optional {}
}}

effects! { crate::judge_command::Command {
    leaves {
        "profile" => Self::Profile { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "lemma" => Self::Lemma { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "scenario" => Self::Scenario { .. } => declared((
                &["application-contract", "decision-scenarios"],
                &["optional-judge-packet"],
                false,
                None,
            ));
    }
    groups {
        "probe" => Self::Probe { question } => question: crate::judge_command::ProbeCommand;
    }
    flat {

    }
    optional {}
}}

effects! { crate::judge_command::ProbeCommand {
    leaves {
        "explain" => Self::Explain { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "reachable" => Self::Reachable { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "count" => Self::Count { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "implies" => Self::Implies { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "decisions" => Self::Decisions { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
        "depends" => Self::Depends { .. } => declared((
                &["application-contract"],
                &["optional-judge-packet"],
                false,
                None,
            ));
    }
    groups {

    }
    flat {

    }
    optional {}
}}

effects! { crate::knowledge_command::Operation {
    leaves {
        "advise" => Self::Advise { .. } => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "plan" => Self::Plan { .. } => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "replan" => Self::Replan { .. } => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "explain" => Self::Explain { .. } => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "check" => Self::Check => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "query" => Self::Query { .. } => declared((
                &["explicit-knowledge-layers", "explicit-validation-records"],
                &[],
                false,
                None,
            ));
        "replay" => Self::Replay { .. } => declared((
                &[
                    "explicit-versioned-recipe",
                    "bound-inputs",
                    "pinned-checker",
                ],
                &["new-private-validation-file", "temporary-checker-files"],
                true,
                None,
            ));
    }
    groups {

    }
    flat {

    }
    optional {}
}}
