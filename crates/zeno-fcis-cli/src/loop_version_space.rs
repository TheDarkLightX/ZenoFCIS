//! LLM-generated shell for requirements-only questions. The door proposes
//! counts; the existing library law evaluator and replayed transform checker
//! decide the final program. No existing core is read or invented as a target.

use super::{STATELESS_DEADLINE, Stages};
use crate::neural_loop::program_json::{encode, program_to_json};
use crate::transform::{self, sha256_hex};
use clap::Args;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;
use zeno_fcis_loop_doors::expr::{self, Allow, Expr, Op};
use zeno_fcis_loop_doors::lower::lower_outputs;
use zeno_fcis_loop_doors::subject::tuples;
use zeno_fcis_loop_doors::version_space::{
    self as V, Budget, Limits, Proposal, Requirement, Specification, Stop,
};
use zeno_fcis_synthesis::finite::{
    Domain, Op as Instruction, Program, V2Resource, v2_laws as laws, v2_zero_limits,
};

const SCHEMA: &str = "zeno-fcis/loop-version-space/1";

#[derive(Args, Clone, Copy, Default)]
pub(crate) struct Arguments {
    #[arg(long, requires = "specification")]
    max_work: Option<u64>,
    #[arg(long, requires = "specification")]
    max_domain: Option<u64>,
    #[arg(long, requires = "specification")]
    max_meanings: Option<usize>,
    #[arg(long, requires = "specification")]
    max_cells: Option<u64>,
    #[arg(long, requires = "specification")]
    max_conflict_subsets: Option<u64>,
}
impl Arguments {
    fn limits(self) -> Limits {
        let defaults = Limits::default();
        Limits {
            work: self.max_work.unwrap_or(defaults.work),
            domain: self.max_domain.unwrap_or(defaults.domain),
            meanings: self.max_meanings.unwrap_or(defaults.meanings),
            cells: self.max_cells.unwrap_or(defaults.cells),
            conflict_subsets: self
                .max_conflict_subsets
                .unwrap_or(defaults.conflict_subsets),
        }
    }
}

// Reject duplicate properties and nonintegral numbers at every nesting level.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded integral JSON with unique keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Strict(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(de::Error::custom("duplicate JSON property"));
                    }
                    let Strict(v) = a.next_value()?;
                    values.insert(k, v);
                }
                Ok(Strict(Value::Object(values)))
            }
        }
        d.deserialize_any(Read)
    }
}
fn bad(reason: impl Into<String>) -> Stop {
    Stop::Refused(reason.into())
}
fn fields(v: &Value, wanted: &[&str]) -> Result<(), Stop> {
    let m = v.as_object().ok_or_else(|| bad("version-space-object"))?;
    if m.len() != wanted.len() || wanted.iter().any(|k| !m.contains_key(*k)) {
        return Err(bad("version-space-fields"));
    }
    Ok(())
}
fn list(v: &Value) -> Result<&[Value], Stop> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| bad("version-space-list"))
}
fn text(v: &Value) -> Result<String, Stop> {
    v.as_str()
        .filter(|s| s.is_ascii())
        .map(str::to_owned)
        .ok_or_else(|| bad("version-space-ascii-text"))
}
fn domain(v: &Value) -> Result<Domain, Stop> {
    if v == &json!({"kind":"Bool"}) {
        return Ok(Domain::Bool);
    }
    fields(v, &["kind", "min", "max"])?;
    let min = v["min"]
        .as_i64()
        .ok_or_else(|| bad("version-space-domain"))?;
    let max = v["max"]
        .as_i64()
        .ok_or_else(|| bad("version-space-domain"))?;
    if v["kind"] != "Int" || min > max {
        return Err(bad("version-space-domain"));
    }
    Ok(Domain::Int { min, max })
}
fn point(v: &Value, domains: &[Domain]) -> Result<Vec<i64>, Stop> {
    let values = list(v)?;
    if values.len() != domains.len() {
        return Err(bad("version-space-tuple-arity"));
    }
    values
        .iter()
        .zip(domains)
        .map(|(v, d)| {
            let n = if *d == Domain::Bool {
                v.as_bool().map(i64::from).or_else(|| v.as_i64())
            } else {
                v.as_i64()
            }
            .ok_or_else(|| bad("version-space-tuple-kind"))?;
            if !d.contains(n) {
                return Err(bad("version-space-value-outside-domain"));
            }
            Ok(n)
        })
        .collect()
}
fn specification(data: &[u8]) -> Result<Specification, Stop> {
    if !data.is_ascii() {
        return Err(bad("version-space-ascii-json"));
    }
    let Strict(v) =
        serde_json::from_slice(data).map_err(|e| bad(format!("version-space-json:{e}")))?;
    fields(
        &v,
        &["schema", "inputs", "outputs", "sketch", "requirements"],
    )?;
    if v["schema"] != SCHEMA {
        return Err(bad("version-space-schema"));
    }
    let mut names = Vec::new();
    let mut inputs = Vec::new();
    for f in list(&v["inputs"])? {
        fields(f, &["name", "domain"])?;
        names.push(text(&f["name"])?);
        inputs.push(domain(&f["domain"])?);
    }
    let outputs = list(&v["outputs"])?
        .iter()
        .map(domain)
        .collect::<Result<Vec<_>, _>>()?;
    let sketch = list(&v["sketch"])?
        .iter()
        .map(|s| {
            Ok(expr::parse(
                &text(s)?,
                0,
                Allow {
                    holes: true,
                    params: false,
                    keep: false,
                },
            )?)
        })
        .collect::<Result<Vec<_>, Stop>>()?;
    let sketch = expr::renumber(&sketch)?;
    let mut requirements = Vec::new();
    for r in list(&v["requirements"])? {
        if r.get("law").is_some() {
            fields(r, &["id", "law"])?;
            requirements.push(Requirement::Law {
                id: text(&r["id"])?,
                formula: expr::parse(
                    &text(&r["law"])?,
                    0,
                    Allow {
                        holes: false,
                        params: false,
                        keep: false,
                    },
                )?,
            });
        } else {
            fields(r, &["id", "input", "output"])?;
            requirements.push(Requirement::Example {
                id: text(&r["id"])?,
                input: point(&r["input"], &inputs)?,
                output: point(&r["output"], &outputs)?,
            });
        }
    }
    let spec = Specification {
        names,
        inputs,
        outputs,
        sketch,
        requirements,
    };
    spec.validate()?;
    Ok(spec)
}

/// Lift already admitted finite predicates to the existing typed law IR.
/// This is representation conversion; evaluation remains library-owned.
fn law_nodes(program: &Program) -> Result<Vec<laws::Op<'static>>, Stop> {
    program
        .nodes()
        .iter()
        .map(|op| {
            Ok(match *op {
                Instruction::Input(k) => laws::Op::Observe(laws::Observation::Initial(
                    k.checked_add(1).ok_or_else(|| bad("law-field"))?,
                )),
                Instruction::Bool(v) => laws::Op::Literal(laws::Atom::Bool(v)),
                Instruction::Int(v) => laws::Op::Literal(laws::Atom::I128(i128::from(v))),
                Instruction::Add(a, b) => laws::Op::Add(usize::from(a), usize::from(b)),
                Instruction::Sub(a, b) => laws::Op::Sub(usize::from(a), usize::from(b)),
                Instruction::Eq(a, b) => laws::Op::Eq(usize::from(a), usize::from(b)),
                Instruction::Lt(a, b) => laws::Op::Lt(usize::from(a), usize::from(b)),
                Instruction::And(a, b) => laws::Op::And(usize::from(a), usize::from(b)),
                Instruction::Not(a) => laws::Op::Not(usize::from(a)),
                Instruction::Select(c, a, b) => {
                    laws::Op::Select(usize::from(c), usize::from(a), usize::from(b))
                }
                _ => return Err(bad("version-space-unsupported-law-instruction")),
            })
        })
        .collect()
}

// Invocation-local measured law work survives a later refusal or budget stop.
#[derive(Default, serde::Serialize)]
struct LawUsage {
    calls: u64,
    steps: u64,
    reads: u64,
}

#[cfg(test)]
fn stub_comparison(name: &str) -> bool {
    std::env::var_os("ZENO_LOOP_VERSION_SPACE_STUB_COMPARISON").is_some()
        || std::env::var_os(name).is_some()
}

fn library_law(
    nodes: &[laws::Op<'static>],
    root: usize,
    values: &[i64],
    domains: &[Domain],
    retained: &mut LawUsage,
) -> Result<(), Stop> {
    #[cfg(test)]
    if stub_comparison("ZENO_LOOP_VERSION_SPACE_STUB_LAW_COMPARISON") {
        return Ok(());
    }
    let fields: Vec<laws::Field<'static>> = values
        .iter()
        .zip(domains)
        .enumerate()
        .map(|(k, (v, d))| {
            Ok(laws::Field {
                id: u16::try_from(k + 1).map_err(|_| bad("law-field"))?,
                value: if *d == Domain::Bool {
                    laws::Atom::Bool(*v == 1)
                } else {
                    laws::Atom::I128(i128::from(*v))
                },
            })
        })
        .collect::<Result<Vec<_>, Stop>>()?;
    let yes = [laws::Op::Literal(laws::Atom::Bool(true))];
    let dummy = |id, kind, scope, genesis| laws::Law {
        id,
        kind,
        scope,
        genesis,
        program: laws::Program {
            nodes: &yes,
            root: 0,
        },
    };
    // Required metadata families, as in the contract judges. Only the selected
    // predicate is nontrivial. No transition/adoption authority is conferred.
    let set = [
        dummy(1, laws::Kind::StateInvariant, laws::Scope::Committing, true),
        dummy(2, laws::Kind::RejectNoAuthority, laws::Scope::Reject, false),
        dummy(
            3,
            laws::Kind::CommittedFailureEffects,
            laws::Scope::CommittedFailure,
            false,
        ),
        dummy(
            4,
            laws::Kind::DecisionConformance,
            laws::Scope::Always,
            false,
        ),
        dummy(5, laws::Kind::InitialCondition, laws::Scope::Always, true),
        laws::Law {
            id: 6,
            kind: laws::Kind::AuthoritySubjectRecipient,
            scope: laws::Scope::Always,
            genesis: true,
            program: laws::Program { nodes, root },
        },
    ];
    let frame = laws::Frame::Genesis {
        initial: laws::RootView::Record(&fields),
    };
    let limits = v2_zero_limits()
        .with_limit(V2Resource::Step, 4096)
        .with_limit(V2Resource::Read, 4096)
        .with_limit(V2Resource::Byte, 65536)
        .with_limit(V2Resource::Depth, 128)
        .with_limit(V2Resource::Candidate, 64);
    let (result, usage, diagnostics, _) =
        laws::evaluate(&set, &[1, 2, 3, 4, 5, 6], &frame, limits).into_parts();
    retained.calls += 1;
    retained.steps += usage.used(V2Resource::Step);
    retained.reads += usage.used(V2Resource::Read);
    if matches!(result, Err(laws::Failure::Budget(_))) {
        return Err(Stop::Inconclusive(format!(
            "version-space-library-law:{result:?}"
        )));
    }
    if result.is_ok()
        && diagnostics
            .iter()
            .any(|d| d.id == 6 && d.verdict == laws::Verdict::Satisfied)
    {
        Ok(())
    } else {
        Err(bad(format!("version-space-library-law:{result:?}")))
    }
}

/// Candidate-bound obligations: all output nodes are forced eager by
/// self-equality and bounds, even when no requirement mentions that output.
fn obligation(
    spec: &Specification,
    expressions: &[Expr],
    r: Option<&Requirement>,
) -> Result<Program, Stop> {
    let mut predicate = Expr::Int(1);
    for (e, d) in expressions.iter().zip(&spec.outputs) {
        let total = Expr::bin(Op::Eq, e.clone(), e.clone());
        predicate = Expr::bin(Op::And, predicate, total);
        if *d != Domain::Bool {
            let (lo, hi) = d.bounds();
            predicate = Expr::bin(
                Op::And,
                predicate,
                Expr::bin(Op::Ge, e.clone(), Expr::Int(lo)),
            );
            predicate = Expr::bin(
                Op::And,
                predicate,
                Expr::bin(Op::Le, e.clone(), Expr::Int(hi)),
            );
        }
    }
    if let Some(r) = r {
        let bound = expr::substitute(&spec.predicate(r), &mut |n| {
            if let Expr::Var(name) = n {
                for (j, e) in expressions.iter().enumerate() {
                    if *name == format!("out{j}") {
                        return Ok(Some(e.clone()));
                    }
                }
            }
            Ok(None)
        })?;
        predicate = Expr::bin(Op::And, predicate, bound);
    }
    let template = V::carrier(&spec.inputs, &[Domain::Bool])?;
    let bool_vocab = expr::Vocabulary::new(
        spec.names.clone(),
        spec.inputs.clone(),
        vec![Domain::Bool],
        &BTreeMap::new(),
    )?;
    Ok(lower_outputs(&[predicate], &bool_vocab, &template)?)
}

/// Existing transform comparison and receipt replay, with owned evidence.
fn checked_obligation(
    reference: &[u8],
    bytes: &[u8],
    limits: transform::Limits,
) -> Result<Value, Stop> {
    #[cfg(test)]
    if stub_comparison("ZENO_LOOP_VERSION_SPACE_STUB_TRANSFORM_COMPARISON") {
        return Ok(Value::Null);
    }
    let checked = transform::check(reference, bytes, limits).map_err(|e| match e {
        transform::Rejection::Inconclusive(s) => Stop::Inconclusive(format!("final-check:{s:?}")),
        other => bad(format!("version-space-final-check:{other:?}")),
    })?;
    let receipt = checked.receipt();
    match transform::replay(&receipt, reference, bytes, limits.input_tuples) {
        transform::Replay::Matched => {}
        transform::Replay::OverCap { .. }
        | transform::Replay::NotEquivalent(transform::Rejection::Inconclusive(_)) => {
            return Err(Stop::Inconclusive(
                "version-space-final-replay-budget".into(),
            ));
        }
        _ => return Err(bad("version-space-final-replay")),
    }
    Ok(json!({"verdict":"equivalent","replay":"matched",
        "receipt_sha256":sha256_hex(&receipt),"receipt":serde_json::from_slice::<Value>(&receipt).map_err(|_| bad("receipt-json"))?,
        "reference":reference.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "obligation":bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()}))
}

fn final_check(
    spec: &Specification,
    expressions: &[Expr],
    budget: &mut Budget,
    retained: &mut LawUsage,
) -> Result<Value, Stop> {
    let vocab = spec.vocabulary()?;
    let program = lower_outputs(
        expressions,
        &vocab,
        &V::carrier(&spec.inputs, &spec.outputs)?,
    )?;
    let requirements = spec.requirement_programs()?;
    let laws = requirements
        .iter()
        .map(law_nodes)
        .collect::<Result<Vec<_>, _>>()?;
    let cols = zeno_fcis_loop_doors::subject::columns(&spec.inputs, budget.limits().domain)?;
    let domain = usize::try_from(tuples(&spec.inputs))
        .map_err(|_| Stop::Inconclusive("domain-budget".into()))?;
    for k in 0..domain {
        budget.spend(program.nodes().len() as u64)?;
        let input: Vec<i64> = cols.iter().map(|c| c[k]).collect();
        let output = program
            .evaluate(&input)
            .map_err(|e| bad(format!("candidate-not-total:{e:?}")))?;
        let values: Vec<i64> = input.into_iter().chain(output).collect();
        let domains: Vec<Domain> = spec.inputs.iter().chain(&spec.outputs).copied().collect();
        for (r, nodes) in requirements.iter().zip(&laws) {
            budget.spend(
                (r.nodes().len() as u64)
                    .saturating_mul(2)
                    .saturating_add(16),
            )?;
            library_law(
                nodes,
                usize::from(r.roots()[0]),
                &values,
                &domains,
                retained,
            )?;
        }
    }
    let reference = Program::try_new(
        spec.inputs.clone(),
        vec![Domain::Bool],
        vec![Instruction::Bool(true)],
        vec![0],
    )
    .map_err(|e| bad(format!("version-space-obligation:{e:?}")))?;
    let reference = encode(&reference).map_err(|e| bad(format!("version-space-encoding:{e:?}")))?;
    let limits = transform::Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: budget.limits().domain,
    };
    let mut checks = Vec::new();
    for r in std::iter::once(None).chain(spec.requirements.iter().map(Some)) {
        let predicate = obligation(spec, expressions, r)?;
        // Conservative charge for checker and replay; their own meters also apply.
        budget.spend(
            (domain as u64)
                .saturating_mul((predicate.nodes().len() as u64).saturating_add(2))
                .saturating_mul(2),
        )?;
        let bytes = encode(&predicate).map_err(|e| bad(format!("version-space-encoding:{e:?}")))?;
        let mut check = checked_obligation(&reference, &bytes, limits)?;
        check["requirement"] = json!(r.map(Requirement::id));
        checks.push(check);
    }
    let bytes = encode(&program).map_err(|e| bad(format!("version-space-encoding:{e:?}")))?;
    Ok(
        json!({"status":"checked","decided_by":"library-law-evaluator-and-transform-check","checker":transform::CHECKER,
        "domain":domain,"requirements":spec.requirements.iter().map(Requirement::id).collect::<Vec<_>>(),
        "expressions":expressions.iter().map(expr::print_symbolic).collect::<Vec<_>>(),
        "program":program_to_json(&program),"sha256":sha256_hex(&bytes),"checks":checks,"law_usage":retained,
        "scope":"every requirement on every declared input; no intent beyond the sketch"}),
    )
}

fn stopped(stop: Stop, budget: Option<&Budget>, revision: Option<&str>) -> (u8, Value) {
    let (exit, status, reason) = match stop {
        Stop::Refused(s) => (crate::INVALID, "refused", s),
        Stop::Inconclusive(s) => (crate::BLOCKED, "inconclusive", s),
    };
    let mut report = json!({"schema":SCHEMA,"door":"version-space","authority":"none","status":status,
        "detail":{"revision":revision,"count":null,"question":null,"candidate":null,"reason":reason,"attempts_spent":0,
            "budget":budget.map(usage)}});
    super::search_limits::annotate("version-space", status, &mut report["detail"]);
    (exit, report)
}
fn usage(b: &Budget) -> Value {
    json!({"work":b.work(),"cumulative_table_cells":b.cells(),"conflict_subsets":b.subsets(),
        "limits":{"max_work":b.limits().work,"max_domain":b.limits().domain,"max_meanings":b.limits().meanings,
            "max_cells":b.limits().cells,"max_conflict_subsets":b.limits().conflict_subsets}})
}

pub(super) fn run(path: &Path, arguments: Arguments) -> (u8, Value) {
    let mut stages = Stages::new(Instant::now(), STATELESS_DEADLINE, 0);
    let bytes = match super::read_bytes(path) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let revision = sha256_hex(&bytes);
    let spec = match specification(&bytes) {
        Ok(s) => s,
        Err(e) => return stopped(e, None, Some(&revision)),
    };
    let mut budget = match Budget::new(arguments.limits()) {
        Ok(b) => b,
        Err(e) => return stopped(e, None, Some(&revision)),
    };
    let proposal = match stages.run(true, || V::analyze(&spec, &mut budget)) {
        Err(e) => return stopped(Stop::Inconclusive(e), Some(&budget), Some(&revision)),
        Ok(Err(e)) => return stopped(e, Some(&budget), Some(&revision)),
        Ok(Ok(p)) => p,
    };
    let mut detail = json!({"revision":revision,"count":null,"question":null,"candidate":null,
        "claims":"proposal","decided_by":"door-enumeration","attempts_spent":0});
    let status = match proposal {
        Proposal::Question { count, question: q } => {
            let mut question = json!({"input":q.input,"ordinal":q.ordinal,
                "answers":q.answers.iter().map(|a| json!({"output":a.output,"remaining":a.remaining})).collect::<Vec<_>>(),
                "worst_remaining":q.worst_remaining,"sum_squared_buckets":q.sum_squared_buckets,
                "picker":"min largest bucket, then sum of squares, then input ordinal",
                "revision":revision,"requires_owner_confirmation":true});
            question["id"] = json!(format!(
                "q{}",
                &sha256_hex(question.to_string().as_bytes())[..48]
            ));
            detail["count"] = json!(count);
            detail["question"] = question;
            "question"
        }
        Proposal::Empty { conflict } => {
            detail["count"] = json!(0);
            detail["conflict"] = json!({"requirements":conflict,"minimum_cardinality":true,
                "relative_to":"declared sketch and finite domains","claims":"proposal"});
            "no-completion"
        }
        Proposal::Singleton(m) => {
            let mut retained = LawUsage::default();
            let checked = match stages
                .run(false, || {
                    final_check(&spec, &m.expressions, &mut budget, &mut retained)
                })
                .map_err(Stop::Inconclusive)
                .and_then(|r| r)
            {
                Err(e) => {
                    let (code, mut report) = stopped(e, Some(&budget), Some(&revision));
                    report["detail"]["law_usage"] = json!(retained);
                    return (code, report);
                }
                Ok(c) => c,
            };
            detail["count"] = json!(1);
            detail["candidate"] = checked;
            "determined"
        }
    };
    detail["budget"] = usage(&budget);
    super::search_limits::annotate("version-space", status, &mut detail);
    (
        0,
        json!({"schema":SCHEMA,"door":"version-space","status":status,"authority":"none","detail":detail}),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    fn lying_singleton_spec() -> Specification {
        specification(br#"{"schema":"zeno-fcis/loop-version-space/1","inputs":[{"name":"x","domain":{"kind":"Int","min":0,"max":1}}],"outputs":[{"kind":"Int","min":0,"max":1}],"sketch":["?c[0..1]"],"requirements":[{"id":"zero","input":[0],"output":[0]}]}"#).unwrap()
    }
    #[test]
    fn final_checker_rejects_a_lying_singleton() {
        let spec = lying_singleton_spec();
        let mut budget = Budget::new(Limits::default()).unwrap();
        assert!(
            final_check(
                &spec,
                &[Expr::Int(1)],
                &mut budget,
                &mut LawUsage::default()
            )
            .is_err(),
            "stubbed acceptance must fail this control"
        );
    }
    #[test]
    fn final_law_checker_rejects_a_lying_singleton() {
        let spec = lying_singleton_spec();
        let requirements = spec.requirement_programs().unwrap();
        let predicate = &requirements[0];
        let nodes = law_nodes(predicate).unwrap();
        let domains: Vec<Domain> = spec.inputs.iter().chain(&spec.outputs).copied().collect();
        assert!(
            library_law(
                &nodes,
                usize::from(predicate.roots()[0]),
                &[0, 0],
                &domains,
                &mut LawUsage::default()
            )
            .is_ok(),
            "the same law and frame must accept the correct output"
        );
        let wrong = library_law(
            &nodes,
            usize::from(predicate.roots()[0]),
            &[0, 1],
            &domains,
            &mut LawUsage::default(),
        );
        assert!(
            matches!(wrong, Err(Stop::Refused(reason)) if reason.contains("Violated")),
            "the law comparison alone must reject the wrong output"
        );
    }
    #[test]
    fn final_transform_checker_rejects_a_lying_singleton() {
        let spec = lying_singleton_spec();
        let predicate = obligation(&spec, &[Expr::Int(1)], Some(&spec.requirements[0])).unwrap();
        let reference = Program::try_new(
            spec.inputs.clone(),
            vec![Domain::Bool],
            vec![Instruction::Bool(true)],
            vec![0],
        )
        .unwrap();
        let limits = transform::Limits {
            steps: transform::DEFAULT_STEP_LIMIT,
            input_tuples: 2,
        };
        let good = obligation(&spec, &[Expr::Int(0)], Some(&spec.requirements[0])).unwrap();
        let reference = encode(&reference).unwrap();
        assert!(
            checked_obligation(&reference, &encode(&good).unwrap(), limits).is_ok(),
            "the same checker must accept and replay the correct obligation"
        );
        let wrong = checked_obligation(&reference, &encode(&predicate).unwrap(), limits);
        assert!(
            matches!(wrong, Err(Stop::Refused(reason)) if reason.contains("Counterexample")),
            "the transform comparison alone must reject a lying obligation"
        );
    }
    #[test]
    fn duplicate_json_keys_are_refused() {
        assert!(specification(br#"{"schema":"a","schema":"b"}"#).is_err());
    }
}
