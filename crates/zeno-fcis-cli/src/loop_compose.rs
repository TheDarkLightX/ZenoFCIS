//! `zeno-fcis loop decompose --compose`: step 4 of a decomposition. The
//! architecture, with every component's case table, becomes a composed-domain
//! composition (`zeno-fcis/checked-composition/3`): one generated contract per
//! component and a manifest that binds each component's inputs to the request
//! and to earlier components' state. Then:
//!
//! - the composition's own joint step (`JointComposition::joint_step`, merge
//!   order, the first reject ending the step with no change, the first
//!   committed failure committing its prefix) runs on every input of the
//!   core's declared domain, and its outcome is compared with the core's
//!   decision there;
//! - the composition's obligation check (`check_domain`) runs on the
//!   manifest as `contract check-composition` would.
//!
//! Every submitted guard is kept as written. The contract form needs a last
//! case of `true`, so where the submitted last guard is not literally
//! `true` a catch-all is appended: a reject with a reason of its own, which
//! the core never gives, so any input that reaches it differs from the
//! core. The door reports the weaker of this verdict and steps 1 to 3's, so
//! a step-3 blame is never turned into `accepted`.
//!
//! This module writes the contracts and the manifest; it implements no joint
//! step and decides nothing itself. Equality with the core holds over the
//! core's declared finite domain only. The generated contracts carry one
//! state law (their fields' declared bounds) and invariants and assumptions
//! of `true`, so a `proved` obligation check says the composition is well
//! formed and its bindings stay in their domains, not that it behaves as the
//! core; the comparison says that.
//!
//! This module is LLM-generated.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value, json};
use zeno_fcis_contract::checked_composition::{
    DOMAIN_CHECKER, DOMAIN_SCHEMA, DomainSources, JointComposition, LeafSources, OutcomeKind,
    check_domain,
};
use zeno_fcis_contract::internals::declarations::{Declarations, Kind as TypeKind, Source};
use zeno_fcis_contract::internals::model::Contract;
use zeno_fcis_contract::internals::review::domain::{self, LeafDomain, Position};
use zeno_fcis_contract::internals::rules::{Class, Constant, Rules};
use zeno_fcis_contract::internals::{policy, schema, schema_commitment};
use zeno_fcis_loop_doors::expr::{Expr, Kind, Op, compare_side};

use super::decompose::Part;
use super::judges::{Core, Result, Stop};
use crate::transform::sha256_hex;

/// Type IDs of the generated contracts start here, field IDs at
/// [`FIELD_BASE`]; variant and reason IDs keep the core's, so values and
/// reasons compare directly.
const TYPE_BASE: u32 = 1000;
const FIELD_BASE: u32 = 2000;
const COMPONENT_ID: u32 = 4000;
const LAW_ID: u32 = 5000;

fn refuse<T>(reason: impl Into<String>) -> Result<T> {
    Err(Stop::Invalid(reason.into()))
}

/// The core's positions with their declared domains and variant names.
struct Fields {
    positions: Vec<Position>,
    variants: Vec<Vec<(u16, String)>>,
    reasons: Vec<(u32, String, u32)>,
    /// The genesis value of each state position.
    genesis: Vec<i64>,
}

fn fields(core: &Core) -> Result<Fields> {
    let rules = Rules::read(core.rules_text())?;
    let declarations = Declarations::read(core.project(), &rules.leaf_bindings)?;
    // Generated leaves and the core decision projection have no outbox.
    // Refuse even unused channels: this profile cannot compare deliveries.
    if !declarations.channels.is_empty() || rules.cases.iter().any(|case| !case.outbox.is_empty()) {
        return refuse("compose-core-deliveries-unsupported");
    }
    let positions = domain::positions(&declarations)?;
    // A request root without fields (one value of a sum type, say) is a
    // request position like any other; a state root must be a record.
    if positions
        .iter()
        .any(|p| p.field.is_none() && p.source == Source::State)
    {
        return refuse("compose-needs-a-record-state");
    }
    let variants = positions
        .iter()
        .map(|p| match &p.domain {
            LeafDomain::Sum { type_id, .. } => match declarations.kind(*type_id) {
                Ok(TypeKind::Sum(list)) => list.iter().map(|v| (v.id, v.name.clone())).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
        .collect();
    let mut reasons: Vec<(u32, String, u32)> = declarations
        .reasons
        .iter()
        .map(|id| {
            let (name, precedence) = declarations
                .notes
                .reasons
                .get(id)
                .cloned()
                .unwrap_or_else(|| (format!("reason_{id}"), *id));
            (*id, name, precedence)
        })
        .collect();
    reasons.sort_unstable();
    let genesis = positions
        .iter()
        .filter(|p| p.source == Source::State)
        .map(|p| match p.field.and_then(|f| rules.genesis.get(&f)) {
            Some(Constant::Bool(b)) => Ok(i64::from(*b)),
            Some(Constant::Int(v)) => {
                i64::try_from(*v).map_err(|_| Stop::Invalid("compose-genesis-range".into()))
            }
            None => refuse("compose-genesis-missing"),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Fields {
        positions,
        variants,
        reasons,
        genesis,
    })
}

/// A core name as a generated contract names it: a root without fields is
/// named after its root (`command`, `context`), which contract variables
/// may not be, so it gets a suffix.
fn leaf_name(name: &str) -> String {
    if matches!(name, "state" | "command" | "context") {
        format!("{name}_value")
    } else {
        name.to_owned()
    }
}

/// The name of the catch-all's reason, unless the core already uses it.
const NO_CASE: &str = "no_case_fires";

/// An ID for the catch-all's reason that no declaration of a generated
/// contract uses: past the core's reasons, skipping its variants and the
/// generated IDs.
fn free_reason_id(f: &Fields) -> Result<u32> {
    let taken = |id: u32| {
        f.reasons.iter().any(|(r, ..)| *r == id)
            || f.variants
                .iter()
                .flatten()
                .any(|(v, _)| u32::from(*v) == id)
            || matches!(id, 1 | 2 | 100..=102)
            || id >= TYPE_BASE
    };
    let mut id = f.reasons.iter().map(|(r, ..)| *r).max().unwrap_or(199) + 1;
    while taken(id) {
        if id >= TYPE_BASE {
            return refuse("compose-no-free-reason-id");
        }
        id += 1;
    }
    Ok(id)
}

/// `node` as contract rules text over the core's names (the policy's
/// variables give each name its path), `truth` when its position takes a
/// truth value: there the literals 1 and 0 are written `true` and `false`.
/// Nothing else is rewritten: the contract compiler type-checks the text,
/// so a number in a truth position is refused there, never reinterpreted.
fn rule_text(core: &Core, node: &Expr, truth: bool) -> Result<String> {
    Ok(match node {
        Expr::Int(1) if truth => "true".to_owned(),
        Expr::Int(0) if truth => "false".to_owned(),
        Expr::Int(v) => v.to_string(),
        Expr::Var(name) => leaf_name(name),
        Expr::Not(a) => format!("!({})", rule_text(core, a, true)?),
        Expr::If(c, a, b) => format!(
            "choose({}, {}, {})",
            rule_text(core, c, true)?,
            rule_text(core, a, truth)?,
            rule_text(core, b, truth)?
        ),
        Expr::Bin(Op::Mul, ..) => return refuse("compose-rule-multiplication"),
        Expr::Bin(op, a, b) => {
            let sides = match op {
                Op::And | Op::Or | Op::Implies => true,
                Op::Eq | Op::Ne => compare_side(a, b, &core.vocab)? == Kind::Bool,
                _ => false,
            };
            format!(
                "({}) {} ({})",
                rule_text(core, a, sides)?,
                op.symbol(),
                rule_text(core, b, sides)?
            )
        }
        _ => return refuse("compose-rule-has-a-hole-or-parameter"),
    })
}

/// One generated contract and its manifest entry.
struct Leaf {
    name: String,
    project: String,
    policy: String,
    entry: Value,
    /// Nodes of the component's decision program.
    nodes: usize,
    /// Whether a catch-all case was appended after the submitted ones.
    catch_all: bool,
}

/// A field of a generated contract: its source, its name, the core position
/// whose type it copies (`None`: a one-value placeholder), and its binding.
struct LeafField {
    source: Source,
    name: String,
    position: Option<usize>,
    bind: Option<String>,
}

fn leaf(core: &Core, f: &Fields, parts: &[Part], k: usize) -> Result<Leaf> {
    let part = &parts[k];
    let owner_of: BTreeMap<usize, usize> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.owns.iter().map(move |f| (*f, i)))
        .collect();
    let mut leaf_fields: Vec<LeafField> = Vec::new();
    let owns = !part.owns.is_empty();
    if owns {
        let mut owned = part.owns.clone();
        owned.sort_unstable();
        for position in owned {
            leaf_fields.push(LeafField {
                source: Source::State,
                name: leaf_name(&core.names[position]),
                position: Some(position),
                bind: None,
            });
        }
    } else {
        // A read-only component keeps one state field with a single value,
        // bound to a constant.
        leaf_fields.push(LeafField {
            source: Source::State,
            name: "mark".into(),
            position: None,
            bind: Some("0".into()),
        });
    }
    let mut reads = part.reads.clone();
    reads.sort_unstable();
    reads.dedup();
    for position in reads.into_iter().filter(|p| !part.owns.contains(p)) {
        let source = f.positions[position].source;
        let (source, bind) = match source {
            Source::State => {
                let Some(owner) = owner_of.get(&position) else {
                    return refuse(format!("compose-unowned-read:{}", core.names[position]));
                };
                (
                    Source::Command,
                    format!(
                        "{}.{}",
                        parts[*owner].name,
                        leaf_name(&core.names[position])
                    ),
                )
            }
            other => (other, core.names[position].clone()),
        };
        leaf_fields.push(LeafField {
            source,
            name: leaf_name(&core.names[position]),
            position: Some(position),
            bind: Some(bind),
        });
    }
    for (source, name) in [(Source::Command, "request"), (Source::Context, "context")] {
        if !leaf_fields.iter().any(|l| l.source == source) {
            leaf_fields.push(LeafField {
                source,
                name: format!("{name}_mark"),
                position: None,
                bind: Some("0".into()),
            });
        }
    }
    // Field names only need to be unique within their record, so a renamed
    // root (`command_value`) or a placeholder can meet a real field of that
    // name. Every name must be unique across the component's roots, since
    // `variables` and `bind` are keyed by it; nothing is renamed further.
    let mut seen = BTreeSet::new();
    for field in &leaf_fields {
        if !seen.insert(field.name.as_str()) {
            return refuse(format!(
                "compose-renamed-name-collides:{}:{}",
                part.name, field.name
            ));
        }
    }
    // project.zeno
    let mut project = format!(
        "zeno 1;\nproject 1 {};\nnamespace 2 loop;\ntype 100 state State;\ntype 101 command Command;\ntype 102 context Context;\n",
        part.name
    );
    let mut bindings = Map::new();
    let mut leaf_bindings = Map::new();
    let mut variables = Map::new();
    let mut genesis = Map::new();
    let mut bounds = Vec::new();
    let mut variant_lines = String::new();
    // One data type per core sum type, so its variants are declared once.
    let mut sum_types: BTreeMap<u32, u32> = BTreeMap::new();
    for (j, field) in leaf_fields.iter().enumerate() {
        let type_id = TYPE_BASE + u32::try_from(j).unwrap_or(0);
        let field_id = FIELD_BASE + u32::try_from(j).unwrap_or(0);
        let domain = field.position.map(|p| &f.positions[p].domain);
        let (lo, hi) = match domain {
            None => {
                project.push_str(&format!("type {type_id} int Mark{j} in 0..=0;\n"));
                (0, 0)
            }
            Some(LeafDomain::Bool) => {
                project.push_str(&format!("type {type_id} bool Flag{j};\n"));
                leaf_bindings.insert(type_id.to_string(), json!(["Bool"]));
                (0, 1)
            }
            Some(LeafDomain::Int { min, max }) => {
                project.push_str(&format!("type {type_id} int Range{j} in {min}..={max};\n"));
                (*min, *max)
            }
            Some(LeafDomain::Sum {
                type_id: core_type,
                variants,
            }) => {
                if !sum_types.contains_key(core_type) {
                    sum_types.insert(*core_type, type_id);
                    project.push_str(&format!("type {type_id} data Choice{j};\n"));
                    for (id, name) in &f.variants[field.position.unwrap_or(0)] {
                        variant_lines.push_str(&format!("variant {id} {type_id} {name} none;\n"));
                    }
                }
                let ids = variants.iter().map(|v| i64::from(*v));
                (ids.clone().min().unwrap_or(0), ids.max().unwrap_or(0))
            }
        };
        let (root, prefix) = match field.source {
            Source::State => (100, "pre"),
            Source::Command => (101, "command"),
            Source::Context => (102, "context"),
        };
        let field_type = match domain {
            Some(LeafDomain::Sum {
                type_id: core_type, ..
            }) => sum_types.get(core_type).copied().unwrap_or(type_id),
            _ => type_id,
        };
        project.push_str(&format!(
            "field {field_id} {root} {} {field_type};\n",
            field.name
        ));
        variables.insert(
            field.name.clone(),
            json!(format!("{prefix}.{root}.{field_id}")),
        );
        if let Some(bind) = &field.bind {
            bindings.insert(field.name.clone(), json!(bind));
        }
        if field.source == Source::State {
            let value = match field.position {
                Some(p) => f.genesis[p],
                None => 0,
            };
            genesis.insert(
                field_id.to_string(),
                if domain == Some(&LeafDomain::Bool) {
                    json!(value == 1)
                } else {
                    json!(value)
                },
            );
            bounds.push(format!(
                "{lo} <= post.100.{field_id} && post.100.{field_id} <= {hi}"
            ));
        }
    }
    project.push_str(&variant_lines);
    // The contract form refuses a declared reason no case uses, so each
    // component declares only its own cases' reasons, ranked in the core's
    // order (IDs kept, so reasons compare with the core's directly).
    let mut used: Vec<&(u32, String, u32)> = f
        .reasons
        .iter()
        .filter(|(id, ..)| {
            part.cases
                .iter()
                .any(|case| case.class != Class::Accept && case.reason == Some(*id))
        })
        .collect();
    used.sort_by_key(|(id, _, precedence)| (*precedence, *id));
    for (rank, (id, name, _)) in used.iter().enumerate() {
        project.push_str(&format!("reason {id} {name} precedence {rank};\n"));
    }
    // The catch-all's own reason: an ID and a name the core does not use,
    // ranked last.
    let catch_all = part
        .cases
        .last()
        .is_none_or(|case| case.when != Expr::Int(1));
    let no_case = if catch_all {
        if f.reasons.iter().any(|(_, name, _)| name == NO_CASE) {
            return refuse(format!("compose-reason-name-taken:{NO_CASE}"));
        }
        let id = free_reason_id(f)?;
        project.push_str(&format!(
            "reason {id} {NO_CASE} precedence {};\n",
            used.len()
        ));
        Some(id)
    } else {
        None
    };
    project.push_str(&format!(
        "component {COMPONENT_ID} {} {{\n  owns 100;\n  reads pre.100;\n  writes post.100;\n  contexts context.102;\n  budget steps 64;\n}}\nmerge [{COMPONENT_ID}];\nlaw {LAW_ID} bounds on commit, genesis = {};\n",
        part.name,
        bounds.join(" && ")
    ));
    // v2/policy.json
    let field_id_of = |position: usize| -> Option<u32> {
        leaf_fields
            .iter()
            .position(|l| l.source == Source::State && l.position == Some(position))
            .map(|j| FIELD_BASE + u32::try_from(j).unwrap_or(0))
    };
    let mut cases = Vec::new();
    for case in &part.cases {
        let mut post = Map::new();
        if case.class != Class::Reject {
            for (position, value) in &case.sets {
                let Some(id) = field_id_of(*position) else {
                    return refuse(format!("compose-case-sets-unowned:{}", part.name));
                };
                post.insert(
                    id.to_string(),
                    json!(rule_text(core, value, core.bools[*position])?),
                );
            }
        }
        cases.push(json!({
            "when": rule_text(core, &case.when, true)?,
            "class": case.class.name(),
            "reason": if case.class == Class::Accept { Value::Null } else { json!(case.reason) },
            "post": post,
            "outbox": [],
        }));
    }
    if let Some(no_case) = no_case {
        cases.push(json!({
            "when": "true",
            "class": "Reject",
            "reason": no_case,
            "post": {},
            "outbox": [],
        }));
    }
    let mut law_kinds = Map::new();
    law_kinds.insert(LAW_ID.to_string(), json!("StateInvariant"));
    let policy_value = json!({
        "schema": "zeno-fcis/template-declarative-policy/2",
        "template": format!("loop-{}", part.name),
        "roots": {"state": 100, "command": 101, "context": 102},
        "leaf_bindings": leaf_bindings,
        "variables": variables,
        "cases": cases,
        "genesis": genesis,
        "law_kinds": law_kinds,
    });
    let policy_text = serde_json::to_string_pretty(&policy_value)
        .map_err(|_| Stop::Invalid("compose-policy".into()))?
        + "\n";
    let (program, nodes) = program_identity(&project, &policy_text)?;
    let owns_list: Vec<Value> = if owns {
        leaf_fields
            .iter()
            .filter(|l| l.source == Source::State)
            .map(|l| json!(format!("{}.{}", part.name, l.name)))
            .collect()
    } else {
        Vec::new()
    };
    let entry = json!({
        "name": part.name,
        "path": part.name,
        "project_sha256": sha256_hex(project.as_bytes()),
        "policy_sha256": sha256_hex(policy_text.as_bytes()),
        "program_sha256": program,
        "owns": owns_list,
        "participates": "true",
        "bind": bindings,
        "assume": "true",
        "invariant": if owns { "true".to_owned() } else { "mark == 0".to_owned() },
    });
    Ok(Leaf {
        name: part.name.clone(),
        project,
        policy: policy_text,
        entry,
        nodes,
        catch_all,
    })
}

/// The bound program's identity, as the composition checker pins it, and
/// the program's node count.
fn program_identity(project: &str, policy_text: &str) -> Result<(String, usize)> {
    let rules = Rules::read(policy_text)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let contract = Contract::build(&declarations, &rules, commitment)?;
    let nodes = contract.program()?.nodes().len();
    let identity = policy::with_authority(&contract, &schema, |authority| {
        sha256_hex(authority.identity())
    })?;
    Ok((identity, nodes))
}

/// The architecture as a composed-domain composition: the manifest's text
/// and each component's files, in merge order.
fn composition(core: &Core, parts: &[Part]) -> Result<(String, Vec<Leaf>)> {
    if parts.iter().any(|p| !is_identifier(&p.name)) {
        return refuse("compose-component-names-must-be-lowercase-identifiers");
    }
    let f = fields(core)?;
    let leaves = (0..parts.len())
        .map(|k| leaf(core, &f, parts, k))
        .collect::<Result<Vec<_>>>()?;
    let request: Vec<Value> = (core.state_width..core.names.len())
        .map(|k| {
            let domain = match &f.positions[k].domain {
                LeafDomain::Bool => json!({"kind": "bool"}),
                LeafDomain::Int { min, max } => json!({"kind": "int", "min": min, "max": max}),
                LeafDomain::Sum { variants, .. } => json!({"kind": "int",
                    "min": variants.iter().min().copied().unwrap_or(0),
                    "max": variants.iter().max().copied().unwrap_or(0)}),
            };
            json!({"name": core.names[k], "domain": domain})
        })
        .collect();
    let manifest = json!({
        "schema": DOMAIN_SCHEMA,
        "checker": DOMAIN_CHECKER,
        "name": "loop-decomposition",
        "request": request,
        "components": leaves.iter().map(|l| l.entry.clone()).collect::<Vec<_>>(),
        "couplings": [],
    });
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|_| Stop::Invalid("compose-manifest".into()))?
        + "\n";
    Ok((text, leaves))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Step 4: the composed architecture's joint step on every input of the
/// core's domain against the core's decision, and the composition's
/// obligation check. `export`, when given, is a new directory that
/// receives the manifest and the generated contracts, whatever the verdict
/// (a refuted composition is exported too).
pub(super) fn compose(core: &Core, parts: &[Part], export: Option<&Path>) -> Result<Value> {
    if core.decisions.iter().any(Option::is_none) {
        return refuse("compose-core-has-authority-refusals");
    }
    let (manifest, leaves) = composition(core, parts)?;
    if let Some(dir) = export {
        // `create_dir` fails on an existing directory: nothing is overwritten.
        std::fs::create_dir(dir).map_err(|error| {
            Stop::Invalid(format!(
                "compose-export-needs-a-new-directory: {}: {error}",
                dir.display()
            ))
        })?;
        let write = |path: &Path, text: &str| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| Stop::Invalid(format!("{}: {error}", parent.display())))?;
            }
            std::fs::write(path, text)
                .map_err(|error| Stop::Invalid(format!("{}: {error}", path.display())))
        };
        write(&dir.join("composition.json"), &manifest)?;
        for leaf in &leaves {
            write(&dir.join(&leaf.name).join("project.zeno"), &leaf.project)?;
            write(&dir.join(&leaf.name).join("v2/policy.json"), &leaf.policy)?;
        }
    }
    let sources: Vec<LeafSources> = leaves
        .iter()
        .map(|l| LeafSources {
            project: l.project.clone(),
            rules: l.policy.clone(),
        })
        .collect();
    let domain_sources = DomainSources {
        manifest: &manifest,
        components: &sources,
    };
    let joint = JointComposition::from_sources(&domain_sources)
        .map_err(|refusal| Stop::Invalid(format!("composition refused: {}", refusal.json())))?;
    // Where each global state slot and request slot sits among the core's
    // positions.
    // Each global state slot by its full path (owning component, field).
    let owned: BTreeMap<(&str, String), usize> = parts
        .iter()
        .flat_map(|part| {
            part.owns
                .iter()
                .map(move |p| ((part.name.as_str(), leaf_name(&core.names[*p])), *p))
        })
        .collect();
    let slot_of: Vec<usize> = joint
        .state_layout()
        .iter()
        .map(|slot| {
            owned
                .get(&(slot.component.as_str(), slot.field.clone()))
                .copied()
        })
        .collect::<Option<_>>()
        .ok_or_else(|| Stop::Invalid("compose-state-layout".into()))?;
    let request_of: Vec<usize> = joint
        .request_layout()
        .iter()
        .map(|slot| core.names.iter().position(|n| *n == slot.name))
        .collect::<Option<_>>()
        .ok_or_else(|| Stop::Invalid("compose-request-layout".into()))?;
    let mut differs = 0usize;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut first: Option<Value> = None;
    for ordinal in 0..core.tuples() {
        let tuple = core.tuple(ordinal);
        let global: Vec<i64> = slot_of.iter().map(|p| tuple[*p]).collect();
        let request: Vec<i64> = request_of.iter().map(|p| tuple[*p]).collect();
        let outcome = joint
            .joint_step(&global, &request)
            .map_err(|error| Stop::Invalid(format!("compose-joint-input: {error:?}")))?;
        let Some(d) = core.decisions[ordinal] else {
            return refuse("compose-core-has-authority-refusals");
        };
        let successor = core.state(d.next);
        let mut composed_post = tuple[..core.state_width].to_vec();
        for (slot, position) in slot_of.iter().enumerate() {
            composed_post[*position] = outcome.successor[slot];
        }
        let (class, reason) = match &outcome.kind {
            OutcomeKind::Accept => (Some(Class::Accept), None),
            OutcomeKind::Reject { reason, .. } => (Some(Class::Reject), *reason),
            OutcomeKind::CommittedFailure { reason, .. } => {
                (Some(Class::CommittedFailure), *reason)
            }
            _ => (None, None),
        };
        let same = class == Some(d.class)
            && (d.class == Class::Accept || reason == d.reason)
            && composed_post == successor;
        if !same {
            differs += 1;
            let kind = match &outcome.kind {
                OutcomeKind::Refused { component, class } => {
                    format!("refused by {component} ({})", class.name())
                }
                OutcomeKind::Reject { component, .. } => format!("rejected by {component}"),
                OutcomeKind::CommittedFailure { component, .. } => {
                    format!("failed in {component}")
                }
                _ => "accepted".to_owned(),
            };
            *kinds.entry(kind).or_default() += 1;
            first.get_or_insert_with(|| {
                json!({
                    "ordinal": ordinal,
                    "input": core.shown(ordinal),
                    "core": core.decision_json(d.class, d.reason, Some(d.next)),
                    "composed": outcome.json(),
                })
            });
        }
    }
    let checked = check_domain(&domain_sources)
        .map(|result| result.json())
        .unwrap_or_else(|refusal| json!({"verdict": "refused", "refusal": refusal.json()}));
    if let Some(dir) = export {
        let verdict = json!({
            "schema": "zeno-fcis/loop-composed-verdict/1",
            "status": if differs > 0 { "refuted" } else if checked["verdict"] == "proved" { "accepted" } else { "obligations-not-proved" },
            "differ": differs, "first_difference": first,
            "core_digest": core.source_digest(), "core_digest_kind": "loop-contract-sources/1",
            "manifest_sha256": crate::transform::sha256_hex(manifest.as_bytes()),
            "decided_by": "composition-checker", "authority": "none",
            "obligations": checked["verdict"],
        });
        let bytes = serde_json::to_vec_pretty(&verdict)
            .map_err(|error| Stop::Invalid(error.to_string()))?;
        std::fs::write(dir.join("verdict.json"), bytes)
            .map_err(|error| Stop::Invalid(format!("compose-export-verdict: {error}")))?;
    }
    Ok(json!({
        "manifest_schema": DOMAIN_SCHEMA,
        "components": leaves.iter().map(|l| json!({"name": l.name, "nodes": l.nodes})).collect::<Vec<_>>(),
        "nodes_total": leaves.iter().map(|l| l.nodes).sum::<usize>(),
        "decided_by": "composition-checker",
        "joint_step": "the composition checker's own (JointComposition::joint_step)",
        "catch_all": leaves.iter().filter(|l| l.catch_all).map(|l| l.name.clone()).collect::<Vec<_>>(),
        "inputs": core.tuples(),
        "differ": differs,
        "equal_to_core": differs == 0,
        "differences": kinds,
        "first_difference": first,
        "obligations": {
            "verdict": checked["verdict"],
            "result": checked,
        },
        "exported": export.map(|p| p.display().to_string()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delivery_source_is_refused_before_composition() {
        let witness =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loop-compose-deliveries");
        let core = Core::load(&witness).unwrap_or_else(|error| panic!("core: {error:?}"));
        assert_eq!(core.decisions.len(), 8);
        assert!(core.decisions.iter().all(Option::is_some));
        assert_eq!(
            core.decisions
                .iter()
                .flatten()
                .filter(|decision| decision.class == Class::Accept)
                .count(),
            2
        );
        assert!(matches!(
            compose(&core, &[], None),
            Err(Stop::Invalid(reason)) if reason == "compose-core-deliveries-unsupported"
        ));
    }
}
