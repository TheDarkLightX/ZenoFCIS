//! Contract change classification: every planted change of the fixture
//! table gets its kind and names exactly its changed items; the document is
//! deterministic and the kind symmetric; an adoption is a program successor
//! and the adoption gate refuses every other kind; and against the SQLite
//! shell's own upgrade module, compiled from its source, the shell's policy
//! comparison accepts exactly the pairs the classifier calls identical or
//! program successors, and every pair its F6.1 Tier A admission admits is a program
//! successor.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;
use zeno_fcis_codec::{Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{Op, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

use super::*;
use crate::contract::{AdoptionSources, CheckedCandidate, Usage, program_bytes};
use crate::transform::{self, DEFAULT_MAX_INPUT_TUPLES, DEFAULT_STEP_LIMIT, Limits};

/// The SQLite shell's pure upgrade decision, compiled from the shell's own
/// source. It imports `Error` and `hash` from its parent, this module, which
/// defines both as the shell does, and its sibling `equivalence`.
#[allow(dead_code, unreachable_pub)]
#[path = "../../../../zeno-fcis-shell-sqlite/src/v2/upgrade.rs"]
mod upgrade;

/// The shell's behaviour-change admission and data migration, which
/// `upgrade` imports from its parent through the actual shell library.
use crate::shell_v2::{behaviour, migration};

/// The shell's exhaustive program comparison, premise 3 of a succession,
/// which `upgrade` imports from its parent through the actual shell library.
use crate::transform::tests::shell_equivalence as equivalence;

/// The shell's error, with the variants its upgrade module uses.
#[derive(Debug)]
enum Error {
    Encoding(EncodeError),
    Identity,
    Lineage,
    Range,
    Upgrade(upgrade::Refusal),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encoding(error) => write!(formatter, "encoding: {error:?}"),
            other => write!(formatter, "{other:?}"),
        }
    }
}

impl From<EncodeError> for Error {
    fn from(error: EncodeError) -> Self {
        Self::Encoding(error)
    }
}

/// The shell's domain-separated hash.
fn hash(domain: Domain<'static>, bytes: &[u8]) -> Result<Hash32, Error> {
    Ok(commitment::<RustCryptoSha256>(domain, bytes)?)
}

/// A contract's files: what `Inputs::read` reads from a directory.
#[derive(Clone, Debug)]
struct Files {
    project: String,
    rules: String,
    adoptions: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Files {
    /// The files of `base`, relative to the CLI crate, with `edits` applied;
    /// each edit must match exactly its `count` times.
    fn read(base: &str, edits: &[Value]) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(base);
        let read = |name: &str| {
            fs::read(dir.join(name)).unwrap_or_else(|error| panic!("read {base}/{name}: {error}"))
        };
        let text = |name: &str| {
            String::from_utf8(read(name)).unwrap_or_else(|error| panic!("{base}/{name}: {error}"))
        };
        let mut files = BTreeMap::from([
            ("project.zeno".to_owned(), text("project.zeno")),
            ("v2/policy.json".to_owned(), text("v2/policy.json")),
        ]);
        for edit in edits {
            let field = |key: &str| {
                edit[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("{edit}: {key}"))
            };
            let file = files
                .get_mut(field("file"))
                .unwrap_or_else(|| panic!("{edit}: unknown file"));
            let count = usize::try_from(edit["count"].as_u64().unwrap_or(0)).unwrap_or(0);
            assert_eq!(file.matches(field("from")).count(), count, "{base}: {edit}");
            *file = file.replace(field("from"), field("to"));
        }
        let rules = files.remove("v2/policy.json").unwrap_or_default();
        let count = serde_json::from_str::<Value>(&rules)
            .ok()
            .and_then(|rules| Some(rules.get("adoptions")?.as_array()?.len()))
            .unwrap_or(0);
        let adoptions = (1..=count)
            .map(|ordinal| {
                let directory = format!("v2/adoptions/{ordinal}");
                (
                    read(&format!("{directory}/program.zcve")),
                    read(&format!("{directory}/receipt.json")),
                )
            })
            .collect();
        Self {
            project: files.remove("project.zeno").unwrap_or_default(),
            rules,
            adoptions,
        }
    }

    fn adoption_sources(&self) -> Vec<AdoptionSources<'_>> {
        self.adoptions
            .iter()
            .map(|(candidate, receipt)| AdoptionSources { candidate, receipt })
            .collect()
    }

    fn sources<'a>(&'a self, adoptions: &'a [AdoptionSources<'a>]) -> ContractSources<'a> {
        ContractSources {
            project: &self.project,
            rules: &self.rules,
            schema_origin: None,
            adoptions,
            replayed: &[],
            evolutions: &[],
        }
    }

    fn generated(self) -> Generated {
        let generated = {
            let adoptions = self.adoption_sources();
            generate_contract(self.sources(&adoptions))
                .unwrap_or_else(|error| panic!("the contract generates: {error}"))
        };
        Generated {
            files: self,
            generated,
        }
    }
}

/// A contract's files and the contract generated from them.
struct Generated {
    files: Files,
    generated: GeneratedContract,
}

/// One planted pair of the fixture table, both sides generated.
struct Pair {
    name: String,
    kind: String,
    changes: Option<Vec<String>>,
    notes: Option<Vec<String>>,
    old: Generated,
    new: Generated,
}

fn keys(value: &Value) -> Option<Vec<String>> {
    value.as_array().map(|items| {
        items
            .iter()
            .map(|item| item.as_str().unwrap_or_default().to_owned())
            .collect()
    })
}

/// Every pair of `tests/fixtures/contract-diff/pairs.json`, then an
/// adoption of the spend-approval fixture, generated once for all tests.
fn pairs() -> &'static [Pair] {
    static PAIRS: OnceLock<Vec<Pair>> = OnceLock::new();
    PAIRS.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/contract-diff/pairs.json");
        let table: Value = serde_json::from_slice(
            &fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let side = |side: &Value| {
            let edits = side["edits"].as_array().cloned().unwrap_or_default();
            Files::read(side["base"].as_str().unwrap_or_default(), &edits).generated()
        };
        let mut pairs: Vec<Pair> = table["pairs"]
            .as_array()
            .unwrap_or_else(|| panic!("pairs"))
            .iter()
            .map(|pair| Pair {
                name: pair["name"].as_str().unwrap_or_default().to_owned(),
                kind: pair["kind"].as_str().unwrap_or_default().to_owned(),
                changes: keys(&pair["changes"]),
                notes: keys(&pair["notes"]),
                old: side(&pair["old"]),
                new: side(&pair["new"]),
            })
            .collect();
        let spend = Files::read("tests/fixtures/spend-approval", &[]);
        pairs.push(Pair {
            name: "spend-approval-adoption".to_owned(),
            kind: "program-successor".to_owned(),
            changes: Some(vec![
                "program::changed".to_owned(),
                "limit:step:changed".to_owned(),
            ]),
            notes: Some(Vec::new()),
            old: spend.clone().generated(),
            new: adopted(&spend).generated(),
        });
        pairs
    })
}

fn pair(name: &str) -> &'static Pair {
    pairs()
        .iter()
        .find(|pair| pair.name == name)
        .unwrap_or_else(|| panic!("no pair {name}"))
}

/// `files` with one more adoption, made through `contract adopt`'s pure
/// transitions, the classifier's gate included: a candidate with two unused
/// constants, whose receipt the transform checker writes.
fn adopted(files: &Files) -> Files {
    let adoptions = files.adoption_sources();
    let sources = files.sources(&adoptions);
    let current = generate_contract(sources).unwrap_or_else(|error| panic!("{error}"));
    let program = import_program(current.program()).unwrap_or_else(|error| panic!("{error}"));
    let candidate = padded(&program);
    let limits = Limits {
        steps: DEFAULT_STEP_LIMIT,
        input_tuples: DEFAULT_MAX_INPUT_TUPLES,
    };
    let receipt = transform::check(current.program(), &candidate, limits)
        .unwrap_or_else(|rejection| panic!("equivalent: {rejection:?}"))
        .receipt();
    let plan = CheckedCandidate::check(sources, candidate.clone(), receipt.clone())
        .and_then(|checked| checked.plan(sources, Usage::NewVersion))
        .unwrap_or_else(|error| panic!("the adoption is planned: {error}"));
    let mut adopted = files.clone();
    adopted.rules = plan.rules().to_owned();
    adopted.adoptions.push((candidate, receipt));
    adopted
}

/// `program` with two unused constants: equal results, two more Steps.
fn padded(program: &Program) -> Vec<u8> {
    let mut nodes = program.nodes().to_vec();
    nodes.extend([Op::Int(0), Op::Int(0)]);
    let padded = Program::try_new(
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        nodes,
        program.roots().to_vec(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    program_bytes(&padded).unwrap_or_else(|error| panic!("{error}"))
}

fn classify(old: &Generated, new: &Generated) -> Diff {
    let old_adoptions = old.files.adoption_sources();
    let new_adoptions = new.files.adoption_sources();
    between(
        old.files.sources(&old_adoptions),
        &old.generated,
        new.files.sources(&new_adoptions),
        &new.generated,
    )
    .unwrap_or_else(|refused| panic!("{}: {}", refused.side.name(), refused.error))
}

fn entry_keys(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| format!("{}:{}:{}", entry.item, entry.id, entry.change))
        .collect()
}

/// Whether the changed parts are the ones `kind` allows.
fn parts_fit(kind: Kind, parts: &[Part]) -> bool {
    let only = |allowed: &[Part]| parts.iter().all(|part| allowed.contains(part));
    match kind {
        Kind::Identical => parts.is_empty(),
        Kind::ProgramSuccessor => {
            parts.contains(&Part::Program) && only(&[Part::Program, Part::StepLimit])
        }
        Kind::Rename => parts == [Part::Names],
        Kind::LayoutChange => {
            parts.contains(&Part::State)
                && !parts.contains(&Part::Interface)
                && !parts.contains(&Part::Channels)
        }
        Kind::RuleChange => {
            only(&[
                Part::Reasons,
                Part::Genesis,
                Part::Laws,
                Part::Cases,
                Part::Program,
                Part::StepLimit,
                Part::Limits,
            ]) && parts
                .iter()
                .any(|part| *part != Part::Program && *part != Part::StepLimit)
        }
        Kind::Unrelated => !parts.is_empty(),
    }
}

#[test]
fn every_planted_pair_gets_its_kind_and_names_exactly_its_changes() {
    let kinds: Vec<&str> = pairs().iter().map(|pair| pair.kind.as_str()).collect();
    for kind in Kind::PRECEDENCE {
        assert!(kinds.contains(&kind.name()), "no planted {}", kind.name());
    }
    for pair in pairs() {
        let diff = classify(&pair.old, &pair.new);
        let name = &pair.name;
        assert_eq!(diff.kind().name(), pair.kind, "{name}");
        if let Some(expected) = &pair.changes {
            assert_eq!(&entry_keys(&diff.changes), expected, "{name}: changes");
        }
        if let Some(expected) = &pair.notes {
            assert_eq!(&entry_keys(&diff.notes), expected, "{name}: notes");
        }
        assert!(
            parts_fit(diff.kind(), &diff.parts),
            "{name}: {:?}",
            diff.parts
        );
        assert_eq!(
            diff.kind() == Kind::Identical,
            diff.changes.is_empty(),
            "{name}"
        );
        // The kind does not depend on the direction.
        assert_eq!(
            classify(&pair.new, &pair.old).kind(),
            diff.kind(),
            "{name} reversed"
        );
    }
}

#[test]
fn the_document_is_deterministic_and_states_each_admission_path() {
    for pair in pairs() {
        let first = classify(&pair.old, &pair.new);
        let document = first.json();
        assert_eq!(
            document,
            classify(&pair.old, &pair.new).json(),
            "{}",
            pair.name
        );
        assert_eq!(document["schema"], DIFF_SCHEMA);
        assert_eq!(document["status"], "classified");
        assert_eq!(document["authority"], "none");
        assert_eq!(document["kind"], pair.kind);
        assert_eq!(
            document["precedence"],
            serde_json::json!([
                "identical",
                "program-successor",
                "rename",
                "layout-change",
                "rule-change",
                "unrelated"
            ])
        );
        let lines = first.lines();
        assert_eq!(document["summary"], serde_json::json!(lines));
        assert!(
            lines[0].starts_with(&format!("{}: ", pair.kind)),
            "{lines:?}"
        );
        assert!(lines[2].starts_with("admission: "), "{lines:?}");
        for entry in first.changes.iter().chain(&first.notes) {
            assert!(!entry.text.is_empty() && !entry.text.contains('\n'));
            assert!(
                lines.contains(&format!("- {}", entry.text)),
                "{}: {}",
                pair.name,
                entry.text
            );
        }
        // Every admission path the classifier names exists today.
        let exists: Vec<bool> = document["admission"]["paths"]
            .as_array()
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| path["exists_today"].as_bool().unwrap_or(true))
                    .collect()
            })
            .unwrap_or_default();
        let expected = match first.kind() {
            Kind::Identical | Kind::Unrelated => vec![],
            Kind::ProgramSuccessor => vec![true],
            Kind::Rename | Kind::LayoutChange => vec![true],
            Kind::RuleChange => vec![true, true],
        };
        assert_eq!(exists, expected, "{}", pair.name);
        assert_eq!(
            document["admission"]["refused"],
            first.kind() == Kind::Unrelated
        );
        assert_eq!(
            document["admission"]["needed"],
            first.kind() != Kind::Identical
        );
    }
}

#[test]
fn a_rule_change_names_both_admission_paths_and_the_changed_case() {
    let pair = pair("case-guard-changed");
    let diff = classify(&pair.old, &pair.new);
    let document = diff.json();
    let ids: Vec<&str> = document["admission"]["paths"]
        .as_array()
        .map(|paths| {
            paths
                .iter()
                .filter_map(|path| path["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(ids, ["g2-forward-simulation", "g14.1-behaviour-change"]);
    let text = diff.admission_text();
    assert!(text.contains("G14.1's behaviour-change upgrade, which exists today"));
    assert!(text.contains("`contract evolve` records this account"));
    assert!(text.contains("Genesis exactness, law 990, applies only to new stores"));
    assert!(text.contains("G2's forward simulation also exists today, for a rule change"));
    assert!(text.contains("`contract evolve --migration` admits it only when simulation"));
    assert_eq!(
        diff.changes[0].text,
        "case 12 \"execute: tier 1 and above need the CFO\" changed: its condition \
         `action == 172 && tier >= 1 && !cfo_ok` became `action == 172 && tier >= 2 && !cfo_ok`"
    );
    assert_eq!(diff.changes[0].aspects, ["when"]);
}

#[test]
fn every_adoption_is_a_program_successor_held_by_the_new_lineage() {
    for name in ["withdrawal-queue-adoption", "spend-approval-adoption"] {
        let pair = pair(name);
        let diff = classify(&pair.old, &pair.new);
        assert_eq!(diff.kind(), Kind::ProgramSuccessor, "{name}");
        let receipts: Vec<String> = pair
            .new
            .generated
            .summary()
            .adoptions
            .iter()
            .map(|adoption| adoption.receipt_sha256.clone())
            .collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(diff.lineage, Some((1, receipts)), "{name}");
        assert_eq!(diff.rollback, None);
        assert_eq!(diff.json()["admission"]["ready"], true);
        assert!(diff.admission_text().contains(
            "holds the old contract as version 1, with the receipt of the adoption after it"
        ));
        // Back from the adopted version: the same kind, but a store never
        // returns to an earlier version.
        let back = classify(&pair.new, &pair.old);
        assert_eq!(back.kind(), Kind::ProgramSuccessor);
        assert_eq!((back.lineage.clone(), back.rollback), (None, Some(1)));
        assert_eq!(back.json()["admission"]["ready"], false);
        assert!(
            back.admission_text()
                .contains("never returns to an earlier version")
        );
    }
}

#[test]
fn the_adoption_gate_refuses_every_kind_but_a_program_successor_and_names_it() {
    for pair in pairs() {
        let diff = classify(&pair.old, &pair.new);
        let gate = require_successor(&diff, "v2/policy.json adoptions[0]", 1);
        match diff.kind() {
            Kind::ProgramSuccessor => assert!(gate.is_ok(), "{}", pair.name),
            kind => {
                let error = gate
                    .err()
                    .unwrap_or_else(|| panic!("{}: a {} is refused", pair.name, kind.name()));
                assert_eq!(error.place(), "v2/policy.json adoptions[0]");
                assert!(
                    error.reason().starts_with(&format!(
                        "the adoption would make a {} of version 1, not a program successor",
                        kind.name()
                    )),
                    "{}",
                    error.reason()
                );
            }
        }
    }
}

/// The shell's premise-1 policy comparison, and its F6.1 Tier A admission of
/// an upgrade from `old` to `new` as `Superseded::upgrade` decides it:
/// between equal identities nothing is established; otherwise the shell
/// establishes all five premises from the two catalogs (the policy
/// comparison, law 991 in both, both Step limits, no Step observation, and
/// the two decision programs equal on every input tuple under the default
/// cap), with one receipt between them, and its pure decision must admit a
/// program successor.
fn shell(old: &Generated, new: &Generated) -> (bool, bool) {
    let shell_error = |error: Error| ContractError::new("shell", error.to_string());
    let old_adoptions = old.files.adoption_sources();
    let new_adoptions = new.files.adoption_sources();
    let old_sources = old.files.sources(&old_adoptions);
    let new_sources = new.files.sources(&new_adoptions);
    with_current(old_sources, &old.generated, |was| {
        with_current(new_sources, &new.generated, |is| {
            policy::with_catalog(was.contract, was.schema, |_, from| {
                policy::with_catalog(is.contract, is.schema, |_, to| {
                    let premise = upgrade::program_successor(from, to).map_err(shell_error)?;
                    let bind = |catalog| {
                        authority::bind(catalog)
                            .map_err(|refusal| ContractError::new("bind", format!("{refusal:?}")))
                    };
                    let (from_authority, to_authority) = (bind(from)?, bind(to)?);
                    let receipts = [Hash32::new([7; 32])];
                    let successor = if from_authority.identity() == to_authority.identity() {
                        Err(upgrade::Premise::Policy)
                    } else {
                        upgrade::Successor::establish(
                            from,
                            to,
                            &receipts,
                            equivalence::DEFAULT_MAX_INPUT_TUPLES,
                        )
                        .map_err(shell_error)?
                    };
                    let decided = upgrade::decide(&upgrade::Facts {
                        ordinal: 1,
                        sequence: 0,
                        root: Hash32::new([1; 32]),
                        previous_chain: Hash32::new([2; 32]),
                        from_identity: from_authority.identity(),
                        from_schema: from.original_schema(),
                        identity: to_authority.identity(),
                        schema: to.original_schema(),
                        state: &[],
                        admission: successor.map_or_else(
                            |missing| upgrade::Admission::Refused(missing, None),
                            upgrade::Admission::Successor,
                        ),
                    });
                    let admitted = matches!(
                        decided,
                        Ok(plan) if plan.kind() == upgrade::Kind::ProgramSuccessor
                    );
                    Ok((premise, admitted))
                })
            })
        })
    })
    .unwrap_or_else(|error| panic!("{error}"))
}

/// Two directions, not one equality: the classifier decides only the
/// structural kind, which is the shell's policy comparison (premise 1),
/// while Tier A admission also needs law 991, both Step premises and the
/// two programs' equivalence. So the policy comparison accepts exactly the
/// pairs the classifier calls identical or program successors, and Tier A
/// admits only program successors.
#[test]
fn the_policy_comparison_is_the_classifier_and_tier_a_admits_only_program_successors() {
    let mut admitted = Vec::new();
    for pair in pairs() {
        for (old, new, direction) in [
            (&pair.old, &pair.new, "forward"),
            (&pair.new, &pair.old, "back"),
        ] {
            let kind = classify(old, new).kind();
            let (premise, tier_a) = shell(old, new);
            let label = format!("{} {direction}", pair.name);
            assert_eq!(
                premise,
                matches!(kind, Kind::Identical | Kind::ProgramSuccessor),
                "{label}: the shell's premise 1"
            );
            assert!(
                !tier_a || kind == Kind::ProgramSuccessor,
                "{label}: Tier A admitted a {kind:?} pair"
            );
            if tier_a {
                admitted.push(label);
            }
        }
    }
    // The control: the shell does admit both adoptions, in both directions.
    assert_eq!(
        admitted,
        [
            "withdrawal-queue-adoption forward",
            "withdrawal-queue-adoption back",
            "spend-approval-adoption forward",
            "spend-approval-adoption back",
        ]
    );
}

#[test]
fn the_kinds_are_tried_in_their_fixed_precedence() {
    let none = Facts {
        same_policy: false,
        successor: false,
        same_but_names: false,
        same_shapes: false,
        same_names: false,
        same_schema: false,
        same_channels: false,
        state_differs: false,
        same_rest: false,
    };
    let decide = |facts: Facts| {
        Kind::PRECEDENCE
            .into_iter()
            .find(|kind| kind.holds(&facts))
            .unwrap_or(Kind::Unrelated)
    };
    let identical = Facts {
        same_policy: true,
        successor: true,
        same_but_names: true,
        same_shapes: true,
        same_names: true,
        same_schema: true,
        same_channels: true,
        same_rest: true,
        ..none
    };
    // Equal policies meet the program-successor and rule-change conditions
    // too; the precedence decides.
    assert!(Kind::ProgramSuccessor.holds(&identical) && Kind::RuleChange.holds(&identical));
    assert_eq!(decide(identical), Kind::Identical);
    let successor = Facts {
        same_policy: false,
        same_but_names: false,
        ..identical
    };
    assert!(Kind::RuleChange.holds(&successor));
    assert_eq!(decide(successor), Kind::ProgramSuccessor);
    let rename = Facts {
        same_policy: false,
        successor: false,
        same_names: false,
        same_schema: false,
        ..identical
    };
    assert_eq!(decide(rename), Kind::Rename);
    // Names with any other change are no rename, and no rule change.
    let mixed = Facts {
        same_but_names: false,
        ..rename
    };
    assert_eq!(decide(mixed), Kind::Unrelated);
    let layout = Facts {
        state_differs: true,
        same_rest: true,
        same_channels: true,
        ..none
    };
    assert_eq!(decide(layout), Kind::LayoutChange);
    for other in [
        Facts {
            same_rest: false,
            ..layout
        },
        Facts {
            same_channels: false,
            ..layout
        },
    ] {
        assert_eq!(decide(other), Kind::Unrelated);
    }
    let rule = Facts {
        same_schema: true,
        same_shapes: true,
        same_names: true,
        same_rest: true,
        same_channels: true,
        ..none
    };
    assert_eq!(decide(rule), Kind::RuleChange);
    let channels = Facts {
        same_channels: false,
        ..rule
    };
    assert_eq!(decide(channels), Kind::Unrelated);
    assert_eq!(decide(none), Kind::Unrelated);
}

#[test]
fn alignment_keeps_order_and_pairs_each_run() {
    use Aligned::{Added, Changed, Removed, Same};
    assert_eq!(align(b"abc", b"abc"), [Same(0, 0), Same(1, 1), Same(2, 2)]);
    assert_eq!(
        align(b"abc", b"axc"),
        [Same(0, 0), Changed(1, 1), Same(2, 2)]
    );
    assert_eq!(
        align(b"abc", b"abxc"),
        [Same(0, 0), Same(1, 1), Added(2), Same(2, 3)]
    );
    assert_eq!(align(b"abc", b"ac"), [Same(0, 0), Removed(1), Same(2, 1)]);
    assert_eq!(align(b"ab", b"xyb"), [Changed(0, 0), Added(1), Same(1, 2)]);
    assert_eq!(align(b"", b"ab"), [Added(0), Added(1)]);
    assert_eq!(align(b"ab", b""), [Removed(0), Removed(1)]);
    // A moved item is unaligned at both of its places.
    assert_eq!(
        align(b"abc", b"bca"),
        [Removed(0), Same(1, 0), Same(2, 1), Added(2)]
    );
}

#[test]
fn rendered_rules_read_back_as_the_same_tree() {
    let mut checked = 0;
    for pair in pairs() {
        for side in [&pair.old, &pair.new] {
            let rules = super::super::rules::Rules::read(&side.files.rules)
                .unwrap_or_else(|error| panic!("{error}"));
            let trees = rules
                .cases
                .iter()
                .flat_map(|case| std::iter::once(&case.when).chain(case.post.values()))
                .chain(rules.variables.values());
            for tree in trees {
                let text = expr::render(tree);
                assert_eq!(
                    expr::parse(&text).as_ref(),
                    Ok(tree),
                    "{}: {text}",
                    pair.name
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn a_rebuilt_model_must_be_the_generated_contract() {
    let pair = pair("case-guard-changed");
    let old_adoptions = pair.old.files.adoption_sources();
    let new_adoptions = pair.new.files.adoption_sources();
    // The new side's files with the old side's generated contract.
    let refused = between(
        pair.old.files.sources(&old_adoptions),
        &pair.old.generated,
        pair.new.files.sources(&new_adoptions),
        &pair.old.generated,
    )
    .err()
    .unwrap_or_else(|| panic!("a mismatched model is refused"));
    assert_eq!(refused.side, Side::New);
    assert_eq!(refused.error.place(), "v2/policy.zcve");
}
