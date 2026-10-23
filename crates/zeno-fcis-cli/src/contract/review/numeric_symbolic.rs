//! Advisory full-domain law queries. Solvers are injected; only native
//! Authority evaluations can confirm counterexamples. No runtime authority.
mod encode;
pub(crate) mod lemma;
mod native;
mod samples;
#[cfg(test)]
mod tests;

use super::{ContractError, ReviewSources, evaluate, mutants, unsigned};
use crate::contract::{
    declarations::{Declarations, Source},
    model::{Contract, ScalarDomain},
    policy,
    rules::{Class, Rules},
    schema, schema_commitment,
};
use crate::symbolic::{
    smt::{Script, Sort, Term, Values},
    verdict::{Answer, Answers, Query, Solve},
};
use encode::Encoding;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use zeno_fcis_synthesis::finite::V2NumericAtom as Atom;

pub(crate) const MODE: &str = "solver-checked (z3 and cvc5 agree)";
fn error(reason: impl Into<String>) -> ContractError {
    ContractError::new("symbolic numeric laws", reason)
}

/// Count the exact declared input product without constructing its values.
/// Overflow denotes an over-cap domain, never a truncated enumeration.
pub(crate) fn over_cap(project: &str, rules: &str, cap: u64) -> Result<bool, ContractError> {
    let rules = Rules::read_for_project(project, rules)?;
    let decl = Declarations::read(project, &rules.leaf_bindings)?;
    let sc = schema::encode(&decl)?;
    let c = Contract::build(&decl, &rules, schema_commitment(&sc)?)?;
    Ok(size(&c).is_none_or(|n| n > cap as u128))
}
fn size(c: &Contract<'_>) -> Option<u128> {
    c.input_domains
        .iter()
        .enumerate()
        .try_fold(1u128, |n, (i, d)| {
            let count = match unsigned::input_leaf(c, i) {
                crate::contract::model::InputLeaf::Sum { variants, .. } => {
                    Some(variants.len() as u128)
                }
                _ => match d {
                    ScalarDomain::Bool => Some(2),
                    ScalarDomain::Int { min, max } => min.abs_diff(*max).checked_add(1),
                    ScalarDomain::Unsigned { min, max } => max.checked_sub(*min)?.checked_add(1),
                },
            }?;
            n.checked_mul(count)
        })
}
#[derive(Debug)]
enum Verdict {
    Holds,
    Refuted(Json),
    Unknown(String),
}
/// Matching conclusive answers are mandatory, including for refutations.
fn judge(
    answers: &Answers,
    replay: impl Fn(&BTreeMap<String, i128>) -> Result<Json, String>,
) -> Verdict {
    match (&answers.cvc5, &answers.z3) {
        (Answer::Unsat { .. }, Answer::Unsat { .. }) => Verdict::Holds,
        (Answer::Sat(a), Answer::Sat(b)) => match (replay(a), replay(b)) {
            (Ok(w), Ok(_)) => Verdict::Refuted(w),
            (a, b) => Verdict::Unknown(format!("model replay failed: {a:?}; {b:?}")),
        },
        _ => Verdict::Unknown("UNKNOWN: solvers did not agree on a conclusive answer".into()),
    }
}
fn ask(
    script: &Script,
    term: Term,
    label: &str,
    solve: &mut Solve<'_>,
    replay: impl Fn(&BTreeMap<String, i128>) -> Result<Json, String>,
) -> Json {
    let mut script = script.clone();
    script.assert(term);
    let source = script.source();
    let answers = solve(&Query {
        label,
        source: &source,
        #[cfg(test)]
        script: &script,
    });
    let mut result = match judge(&answers, replay) {
        Verdict::Holds => json!({"status":"holds","evidence":MODE}),
        Verdict::Refuted(w) => {
            json!({"status":"refuted","counterexample":w,"evidence":"native Authority replay"})
        }
        Verdict::Unknown(reason) => json!({"status":"UNKNOWN","reason":reason}),
    };
    result["solvers"] = answers.json();
    result["query_sha256"] = json!(crate::transform::sha256_hex(source.as_bytes()));
    result["query"] = json!(label);
    result
}
fn decode(c: &Contract<'_>, model: &BTreeMap<String, i128>) -> Result<Vec<Atom<'static>>, String> {
    c.input_domains
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let get = |name: String| {
                model
                    .get(&name)
                    .copied()
                    .ok_or_else(|| format!("model lacks {name}"))
            };
            Ok(match d {
                ScalarDomain::Bool => match get(format!("x{i}"))? {
                    0 => Atom::Bool(false),
                    1 => Atom::Bool(true),
                    _ => return Err("non-Boolean model input".into()),
                },
                ScalarDomain::Int { min, max } => {
                    let n = get(format!("x{i}"))?;
                    if n < *min || n > *max {
                        return Err("input outside signed range".into());
                    }
                    Atom::I128(n)
                }
                ScalarDomain::Unsigned { min, max } => {
                    let n = if *max <= i128::MAX as u128 {
                        u128::try_from(get(format!("x{i}"))?)
                            .map_err(|_| "negative unsigned input")?
                    } else {
                        let h =
                            u64::try_from(get(format!("x{i}h"))?).map_err(|_| "bad high limb")?;
                        let l =
                            u64::try_from(get(format!("x{i}l"))?).map_err(|_| "bad low limb")?;
                        ((h as u128) << 64) | l as u128
                    };
                    if n < *min || n > *max {
                        return Err("input outside unsigned range".into());
                    }
                    Atom::U128(n)
                }
            })
        })
        .collect()
}
fn assignment(e: &Encoding, row: &[Atom<'static>]) -> Term {
    Term::and(
        e.inputs
            .iter()
            .zip(row)
            .map(|(v, a)| {
                let t = match a {
                    Atom::Bool(b) => Term::Bool(*b),
                    Atom::I128(n) => Term::Int(*n),
                    Atom::U128(n) => encode::uint(*n),
                    _ => Term::Bool(false),
                };
                Term::eq(v.term.clone(), t)
            })
            .collect(),
    )
}
fn witness(c: &Contract<'_>, row: &[Atom<'static>], outcome: &evaluate::Outcome) -> Json {
    json!({"input":c.bindings.iter().zip(row).map(|((s,id),v)|json!({"source":s.prefix(),"field":id,"value":unsigned::atom_json(*v)})).collect::<Vec<_>>(),"values":row.iter().copied().map(unsigned::atom_json).collect::<Vec<_>>(),"authority":unsigned::normalized(outcome)})
}
/// Guard the actual emitted formula against the native Authority on boundary,
/// arithmetic-edge and deterministic random tuples for every command variant.
fn guard_inner(
    c: &Contract<'_>,
    schema: &[u8],
    e: &Encoding,
    solve: &mut Solve<'_>,
) -> Result<Json, ContractError> {
    native::with_evaluator(c, schema, |evaluate| {
        let mut rows = samples::rows(c)?;
        let mut edge_checks = Vec::new();
        if e.edges.len() > 4096 {
            return Ok(
                json!({"status":"UNKNOWN","reason":"more than 4096 arithmetic edge questions"}),
            );
        }
        for (i, edge) in e.edges.iter().enumerate() {
            let found = std::cell::RefCell::new(Vec::new());
            let replay = |model: &BTreeMap<String, i128>| {
                let row = decode(c, model)?;
                let actual = evaluate(&row).map_err(|e| e.to_string())?;
                let result = witness(c, &row, &actual);
                found.borrow_mut().push(row);
                Ok(result)
            };
            // A known input can cheaply realize a branch-selection edge.
            // A miss for one input never proves an edge globally unreachable.
            let probe = if matches!(edge, Term::Eq(a, _) if **a == e.outputs[0].term) {
                Some(ask(
                    &e.script,
                    Term::and(vec![edge.clone(), assignment(e, &rows[0])]),
                    &format!("{}-arithmetic-edge-{i}-sample", e.label),
                    solve,
                    replay,
                ))
            } else {
                None
            };
            let result = match probe {
                Some(probe) if probe["status"] != "holds" => probe,
                probe => {
                    let mut result = ask(
                        &e.script,
                        edge.clone(),
                        &format!("{}-arithmetic-edge-{i}", e.label),
                        solve,
                        replay,
                    );
                    if let Some(probe) = probe {
                        result["sample_probe"] = probe;
                    }
                    result
                }
            };
            if result["status"] == "UNKNOWN" {
                return Ok(
                    json!({"status":"UNKNOWN","reason":"arithmetic edge generation did not complete with solver agreement","failure":result}),
                );
            }
            for row in found.into_inner() {
                if !rows.contains(&row) {
                    rows.push(row);
                }
            }
            edge_checks.push(result);
        }
        let mut batches = Vec::new();
        // A fixed input grounds the arithmetic. Combining several inputs
        // in one disjunction needlessly makes solvers choose nonlinear cases.
        // Per-input questions preserve both existence and mismatch checks.
        for (i, batch) in rows.chunks(1).enumerate() {
            // A mismatch-only query could pass vacuously if auxiliary
            // constraints accidentally removed a native input. Independent
            // fixed copies must also have a model for every sampled input.
            let mut concrete = Script::default();
            for (j, row) in batch.iter().enumerate() {
                let inputs = row
                    .iter()
                    .map(|v| match v {
                        Atom::Bool(b) => encode::literal(&crate::contract::graph::Atom::Bool(*b)),
                        Atom::I128(n) => encode::literal(&crate::contract::graph::Atom::I128(*n)),
                        Atom::U128(n) => encode::literal(&crate::contract::graph::Atom::U128(*n)),
                        _ => unreachable!("guard inputs are numeric or variant codes"),
                    })
                    .collect();
                let mut fixed =
                    Encoding::new(c, &format!("sample{i}r{j}"), concrete, Some(inputs))?;
                let actual = evaluate(row)?;
                let matches = native::matches(c, &fixed, &actual)?;
                fixed.script.assert(matches);
                concrete = fixed.script;
            }
            let mut existence = ask(
                &concrete,
                Term::Bool(true),
                &format!("{}-sample-existence-{i}", e.label),
                solve,
                |_| {
                    for row in batch {
                        evaluate(row).map_err(|e| e.to_string())?;
                    }
                    Ok(json!({"native_samples":batch.len()}))
                },
            );
            if existence["status"] != "refuted" {
                return Ok(
                    json!({"status":"UNKNOWN","reason":"encoding did not realize every native sample","failure":existence}),
                );
            }
            existence["status"] = json!("holds");
            existence["evidence"] = json!(MODE);
            existence
                .as_object_mut()
                .ok_or_else(|| error("numeric guard query report is not an object"))?
                .remove("counterexample");
            batches.push(existence);
            let mut mismatches = Vec::new();
            for row in batch {
                let actual = evaluate(row)?;
                mismatches.push(Term::and(vec![
                    assignment(e, row),
                    Term::not(native::matches(c, e, &actual)?),
                ]));
            }
            let result = ask(
                &e.script,
                Term::or(mismatches),
                &format!("{}-encoding-guard-{i}", e.label),
                solve,
                |model| {
                    let row = decode(c, model)?;
                    let actual = evaluate(&row).map_err(|e| e.to_string())?;
                    Ok(witness(c, &row, &actual))
                },
            );
            if result["status"] != "holds" {
                return Ok(
                    json!({"status":"UNKNOWN","reason":"encoding differential guard did not pass","samples":rows.len(),"failure":result}),
                );
            }
            batches.push(result);
        }
        Ok(
            json!({"status":"holds","samples":rows.len(),"seed":samples::SEED.to_string(),"random_per_command":samples::RANDOM,"arithmetic_edges":edge_checks,"checks":batches}),
        )
    })?
}
fn guard(
    c: &Contract<'_>,
    schema: &[u8],
    e: &Encoding,
    solve: &mut Solve<'_>,
) -> Result<Json, ContractError> {
    match guard_inner(c, schema, e, solve) {
        Ok(report) => Ok(report),
        Err(error) => Ok(json!({"status":"UNKNOWN","reason":error.reason()})),
    }
}

/// Full-domain obligations, only after the contract-specific differential guard.
pub(crate) fn review(
    sources: ReviewSources<'_>,
    solve: &mut Solve<'_>,
    include_mutants: bool,
) -> Result<Json, ContractError> {
    let rules = Rules::read_for_project(sources.project, sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let c = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let mut report = json!({"schema":"zeno-fcis/numeric-law-review/1","authority":"none","mode":"symbolic","scope":"all declared inputs; eager arithmetic and candidate construction; no reachability claim","sources":{"project_sha256":crate::transform::sha256_hex(sources.project.as_bytes()),"rules_sha256":crate::transform::sha256_hex(sources.rules.as_bytes()),"schema_sha256":crate::transform::sha256_hex(&schema)},"domain_size":size(&c).map(|n|n.to_string()),"status":"UNKNOWN"});
    report["case_coverage"] =
        super::case_coverage::Coverage::not_checked("domain not fully enumerated; symbolic path")
            .packet;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        report["reason"] = json!(
            "symbolic numeric review requires a standalone contract; retained lineage replay is unsupported"
        );
        return Ok(report);
    }
    let e = match Encoding::new(&c, "p", Script::default(), None) {
        Ok(e) => e,
        Err(err) => {
            report["reason"] = json!(err.reason());
            return Ok(report);
        }
    };
    let guarded = guard(&c, &schema, &e, solve)?;
    let passed = guarded["status"] == "holds";
    report["encoding_guard"] = guarded;
    if !passed {
        return Ok(report);
    }
    let mut obligations = Vec::new();
    // Arithmetic/creation refusals remain visible; they cannot masquerade as
    // vacuous preservation of laws on an empty candidate set.
    let constructed = Term::or(
        e.cases
            .iter()
            .map(|b| Term::and(vec![b.selected.clone(), b.constructed.clone()]))
            .collect(),
    );
    // Constructibility implies program_ok. Its complement is either a
    // first failed execution predicate or failed construction after success.
    // Prefixes only partition the original failure set; they cannot erase an
    // earlier failed/UNKNOWN obligation or supply new admission assumptions.
    fn conjuncts(term: &Term, out: &mut Vec<Term>) {
        match term {
            Term::And(terms) => {
                for term in terms {
                    conjuncts(term, out);
                }
            }
            _ => out.push(term.clone()),
        }
    }
    let mut predicates = Vec::new();
    conjuncts(&e.program_ok, &mut predicates);
    let mut prefix = Vec::new();
    let mut failures = Vec::new();
    for (i, predicate) in predicates.into_iter().enumerate() {
        let mut failed = prefix.clone();
        failed.push(Term::not(predicate.clone()));
        failures.push((format!("technical-refusal-program-{i}"), Term::and(failed)));
        prefix.push(predicate);
    }
    failures.push((
        "technical-refusal-construction".into(),
        Term::and(vec![e.program_ok.clone(), Term::not(constructed)]),
    ));
    for (label, bad) in failures {
        obligations.push(ask(
            &e.script,
            bad,
            &label,
            solve,
            |model| {
                let row = decode(&c, model)?;
                let actual = native::evaluate(&c, &schema, &row).map_err(|e| e.to_string())?;
                if matches!(&actual, evaluate::Outcome::Refused(r) if r.class != evaluate::RefusalClass::Law) {
                    Ok(witness(&c, &row, &actual))
                } else {
                    Err("native Authority did not confirm a technical refusal".into())
                }
            },
        ));
    }
    for (i, case) in e.cases.iter().enumerate() {
        let mut premise = vec![case.selected.clone(), case.constructed.clone()];
        for (id, holds) in &case.laws {
            let mut bad = premise.clone();
            bad.push(Term::not(holds.clone()));
            let mut result = ask(
                &e.script,
                Term::and(bad),
                &format!("case-{i}-law-{id}"),
                solve,
                |model| {
                    let row = decode(&c, model)?;
                    let actual = native::evaluate(&c, &schema, &row).map_err(|e| e.to_string())?;
                    if matches!(&actual,evaluate::Outcome::Refused(r) if r.class==evaluate::RefusalClass::Law && r.law==Some(*id))
                    {
                        Ok(witness(&c, &row, &actual))
                    } else {
                        Err(format!("native Authority did not confirm law {id}"))
                    }
                },
            );
            result["case"] = json!(i);
            result["law"] = json!(id);
            obligations.push(result);
            premise.push(holds.clone());
        }
    }
    let status = if obligations.iter().any(|r| r["status"] == "UNKNOWN") {
        "UNKNOWN"
    } else if obligations.iter().any(|r| r["status"] == "refuted") {
        "refuted"
    } else {
        "solver-checked"
    };
    report["status"] = json!(status);
    report["evidence"] = json!(if status == "solver-checked" {
        MODE
    } else {
        "none"
    });
    report["obligations"] = json!(obligations);
    report["examples"] = native::examples(&c, &schema, sources.examples)?;
    if report["examples"]["status"] == "refuted" {
        report["status"] = json!("refuted");
        report["evidence"] = json!("native Authority replay");
    }
    if include_mutants {
        report["mutants"] = mutant_checks(&c, &schema, &rules, &declarations, &e, solve)?;
    }
    Ok(report)
}
fn mutant_checks(
    c: &Contract<'_>,
    schema: &[u8],
    rules: &Rules,
    d: &Declarations,
    e: &Encoding,
    solve: &mut Solve<'_>,
) -> Result<Json, ContractError> {
    let mut reports = Vec::new();
    for m in mutants::catalog(rules, d) {
        let candidate = match Contract::build(d, &m.rules, c.commitment) {
            Ok(c) => c,
            Err(err) => {
                reports.push(json!({"id":m.id,"status":"generator-refused","reason":err.reason()}));
                continue;
            }
        };
        let ce = match Encoding::new(&candidate, &m.id, e.script.clone(), Some(e.inputs.clone())) {
            Ok(e) => e,
            Err(err) => {
                reports.push(json!({"id":m.id,"status":"UNKNOWN","reason":err.reason()}));
                continue;
            }
        };
        let cg = guard(&candidate, schema, &ce, solve)?;
        if cg["status"] != "holds" {
            reports.push(json!({"id":m.id,"status":"UNKNOWN","encoding_guard":cg}));
            continue;
        }
        let same = native::equivalent(c, e, &candidate, &ce)?;
        let mut result = ask(
            &ce.script,
            Term::not(same),
            &format!("mutant-{}", m.id),
            solve,
            |model| {
                let row = decode(c, model)?;
                let before = native::evaluate(c, schema, &row).map_err(|e| e.to_string())?;
                let after =
                    native::evaluate(&candidate, schema, &row).map_err(|e| e.to_string())?;
                if before == after {
                    return Err("native decisions do not distinguish mutant".into());
                }
                Ok(
                    json!({"input":witness(c,&row,&before),"mutant_authority":unsigned::normalized(&after)}),
                )
            },
        );
        result["id"] = json!(m.id);
        result["encoding_guard"] = cg;
        reports.push(result);
    }
    Ok(json!(reports))
}
