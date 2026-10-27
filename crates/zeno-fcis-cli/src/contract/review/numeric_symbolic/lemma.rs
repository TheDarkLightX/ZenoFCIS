//! Symbolic induction or relations on native admitted committing decisions.
//! Proposed lemmas are never installed as admission laws of the subject.
use super::super::lemma::PreStates;
use super::*;
use crate::contract::{
    graph::{Graph, LawGraph},
    model::Law,
    rules::LawKind,
};
use zeno_fcis_spec::LawScope;
use zeno_fcis_synthesis::finite::{self, v2_composition as c, v2_laws as l};
const ID: u32 = 3_999_999_999;
fn compile(
    project: &str,
    d: &Declarations,
    r: &Rules,
    formula: &str,
) -> Result<Law, ContractError> {
    let parsed = Declarations::read(
        &format!("{project}\nlaw {ID} proposed_lemma on commit, genesis = {formula};"),
        &r.leaf_bindings,
    )?;
    let ast = parsed
        .laws
        .iter()
        .find(|l| l.id == ID)
        .ok_or_else(|| error("missing proposed lemma"))?
        .formula
        .clone();
    let ast = crate::contract::expr::expand_with_limits(&ast, &r.variables, true).map_err(error)?;
    let mut graph = LawGraph::new(d).with_strict(r.unsigned);
    let v = graph.compile_with_limits(&ast, true).map_err(error)?;
    let root = graph.boolean(v).map_err(error)?.index;
    Ok(Law {
        id: ID,
        kind: LawKind::FeeAndRounding,
        scope: LawScope::Committing,
        genesis: true,
        nodes: graph.table.nodes,
        root,
    })
}
fn atom(v: &evaluate::Value) -> Result<Atom<'static>, ContractError> {
    Ok(match v {
        evaluate::Value::Bool(b) => Atom::Bool(*b),
        evaluate::Value::I128(n) => Atom::I128(*n),
        evaluate::Value::U128(n) => Atom::U128(*n),
        evaluate::Value::Sum { type_id, variant } => Atom::Sum {
            type_id: *type_id,
            variant: *variant,
        },
        _ => return Err(error("lemma requires scalar observations")),
    })
}
fn native_law(
    c: &Contract<'_>,
    schema: &[u8],
    law: &Law,
    row: &[Atom<'static>],
    on_pre: bool,
) -> Result<Option<bool>, ContractError> {
    let nodes: Vec<_> = law.nodes.iter().map(policy::law_op).collect();
    let pre = unsigned::fields(c, row, Source::State);
    let command = unsigned::fields(c, row, Source::Command);
    let context = unsigned::fields(c, row, Source::Context);
    let usage = finite::execute_v2(&[], &[], &[], &[], &[], finite::v2_zero_limits()).usage();
    let verdict = |all: &[l::Law<'_>], required: &[u32], frame: &l::Frame<'_>| {
        let mut laws = vec![l::Law {
            id: ID,
            kind: l::Kind::FeeAndRounding,
            scope: l::Scope::Committing,
            genesis: true,
            program: l::Program {
                nodes: &nodes,
                root: law.root,
            },
        }];
        laws.extend(all.iter().map(|v| l::Law {
            id: v.id,
            kind: v.kind,
            scope: v.scope,
            genesis: v.genesis,
            program: l::Program {
                nodes: v.program.nodes,
                root: v.program.root,
            },
        }));
        let limits = finite::v2_zero_limits()
            .with_limit(finite::V2Resource::Step, 1 << 20)
            .with_limit(finite::V2Resource::Read, 1 << 20);
        let (_, _, ds, _) = l::evaluate(&laws, required, frame, limits).into_parts();
        ds.first().and_then(|d| {
            if d.id != ID {
                None
            } else {
                match d.verdict {
                    l::Verdict::Satisfied => Some(true),
                    l::Verdict::Refused(l::Failure::Violated | l::Failure::Undefined) => {
                        Some(false)
                    }
                    _ => None,
                }
            }
        })
    };
    let run = |all: &[l::Law<'_>], required: &[u32]| -> Result<Option<bool>, ContractError> {
        if on_pre {
            return Ok(verdict(
                all,
                required,
                &l::Frame::Genesis {
                    initial: unsigned::view(c, 0, &pre),
                },
            ));
        }
        let actual = native::evaluate(c, schema, row)?;
        let evaluate::Outcome::Decision(d) = actual else {
            return Ok(None);
        };
        if d.class == Class::Reject {
            return Ok(None);
        }
        let post = d
            .post
            .iter()
            .map(|(id, v)| {
                Ok(c::Field {
                    id: *id,
                    value: atom(v)?,
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        let patch = post
            .iter()
            .filter_map(|f| {
                let before = pre.iter().find(|p| p.id == f.id)?.value;
                (before != f.value).then_some(l::Patch {
                    field: f.id,
                    before,
                    after: f.value,
                })
            })
            .collect::<Vec<_>>();
        let candidate = l::Candidate {
            class: if d.class == Class::Accept {
                c::Class::Accept
            } else {
                c::Class::CommittedFailure
            },
            reason: d.reason,
            post: l::RootView::Record(&post),
            patch: &patch,
            effects: &[],
            outbox: &[],
            reads: &[],
            attempts: &[],
            usage,
        };
        Ok(verdict(
            all,
            required,
            &l::Frame::Transition {
                pre: unsigned::view(c, 0, &pre),
                command: unsigned::view(c, 1, &command),
                context: unsigned::view(c, 2, &context),
                candidate: &candidate,
            },
        ))
    };
    if c.unsigned {
        policy::with_numeric_authority(c, schema, |a| {
            run(a.descriptor().laws, a.descriptor().required)
        })?
    } else {
        policy::with_authority(c, schema, |a| {
            run(a.descriptor().laws, a.descriptor().required)
        })?
    }
}
pub(crate) fn check(
    project: &str,
    rules: &str,
    formulas: &[String],
    transition: bool,
    solve: &mut Solve<'_>,
) -> Result<Json, ContractError> {
    let r = Rules::read_for_project(project, rules)?;
    if !r.adoptions.is_empty() || !r.evolutions.is_empty() {
        return Ok(
            json!({"mode":"symbolic","status":"UNKNOWN","authority":"none","reason":"symbolic numeric lemmas require a standalone contract; retained lineage replay is unsupported"}),
        );
    }
    let d = Declarations::read(project, &r.leaf_bindings)?;
    let schema = schema::encode(&d)?;
    let c = Contract::build(&d, &r, schema_commitment(&schema)?)?;
    let mut results = Vec::new();
    for (index, formula) in formulas.iter().enumerate() {
        let law = compile(project, &d, &r, formula)?;
        // This replay profile intentionally excludes delivery/trace selectors.
        if law.nodes.iter().any(|node|matches!(node,crate::contract::graph::LawOp::Observe(o)|crate::contract::graph::LawOp::ObserveWhen(_,o,_) if !matches!(o,crate::contract::graph::Observation::Pre(_)|crate::contract::graph::Observation::Post(_)|crate::contract::graph::Observation::Initial(_)|crate::contract::graph::Observation::Command(_)|crate::contract::graph::Observation::Context(_)|crate::contract::graph::Observation::CommandRoot|crate::contract::graph::Observation::ContextRoot))) {
            results.push(json!({"formula":formula,"status":"UNKNOWN","reason":"lemma encoding supports scalar input and state observations"}));continue;
        }
        let encoded = (|| {
            let mut e = Encoding::new(&c, &format!("lemma{index}"), Script::default(), None)?;
            let pre = e.law(&c, &law, None, "lemma_pre")?;
            let mut post = Vec::new();
            for i in 0..c.branches.len() {
                post.push(e.law(&c, &law, Some(i), &format!("lemma_b{i}"))?);
            }
            Ok::<_, ContractError>((e, pre, post))
        })();
        // An encoder limit leaves this formula UNKNOWN, as contract review
        // reports it; it is not an invalid input.
        let (e, pre, post) = match encoded {
            Ok(encoded) => encoded,
            Err(err) => {
                results.push(json!({"formula":formula,"mode":"symbolic","status":"UNKNOWN","reason":err.reason()}));
                continue;
            }
        };
        let pre_states = if transition {
            PreStates::TRANSITION
        } else {
            PreStates::Lawful
        };
        let pre_state = pre_states.predicate(Term::Bool(true), || e.lawful.clone());
        let mut result = json!({"formula":formula,"mode":"symbolic","status":"UNKNOWN","scope":if transition{"all native admitted committed decisions"}else{"initiation and induction from lawful pre-states; no reachability enumeration"}});
        let base_guard = guard(&c, &schema, &e, solve)?;
        if base_guard["status"] != "holds" {
            result["encoding_guard"] = base_guard;
            results.push(result);
            continue;
        }
        let rows = samples::rows(&c)?;
        let mut differential = Vec::new();
        for row in &rows {
            let mut expected = Vec::new();
            if !transition && let Some(v) = native_law(&c, &schema, &law, row, true)? {
                expected.push(Term::eq(pre.clone(), Term::Bool(v)));
            }
            if let Some(v) = native_law(&c, &schema, &law, row, false)? {
                expected.push(Term::or(
                    e.cases
                        .iter()
                        .zip(&post)
                        .map(|(b, p)| {
                            Term::and(vec![b.selected.clone(), Term::eq(p.clone(), Term::Bool(v))])
                        })
                        .collect(),
                ));
            }
            differential.push(Term::and(vec![
                assignment(&e, row),
                Term::not(Term::and(expected)),
            ]));
        }
        let differential = ask(
            &e.script,
            Term::or(differential),
            &format!("lemma-{index}-guard"),
            solve,
            |m| {
                let row = decode(&c, m)?;
                Ok(
                    json!({"input":row.iter().copied().map(unsigned::atom_json).collect::<Vec<_>>()}),
                )
            },
        );
        if differential["status"] != "holds" {
            result["encoding_guard"] = differential;
            results.push(result);
            continue;
        }
        result["encoding_guard"] = json!({"subject":base_guard,"lemma":differential});
        let mut initial = samples::rows(&c)?.remove(0);
        for (i, (s, id)) in c.bindings.iter().enumerate() {
            if *s == Source::State {
                let a = &c
                    .genesis
                    .iter()
                    .find(|(field, _)| Some(*field) == *id)
                    .ok_or_else(|| error("missing genesis"))?
                    .1;
                initial[i] = match a {
                    crate::contract::graph::Atom::Bool(b) => Atom::Bool(*b),
                    crate::contract::graph::Atom::I128(n) => Atom::I128(*n),
                    crate::contract::graph::Atom::U128(n) => Atom::U128(*n),
                    crate::contract::graph::Atom::Sum { variant, .. } => {
                        Atom::I128((*variant).into())
                    }
                    _ => return Err(error("unsupported genesis")),
                };
            }
        }
        if !transition && native_law(&c, &schema, &law, &initial, true)? != Some(true) {
            result["status"] = json!("refuted");
            result["reason"] =
                json!("genesis does not satisfy the lemma or its arithmetic is undefined");
            results.push(result);
            continue;
        }
        if transition {
            let commits = Term::or(
                e.cases
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| c.branches[*i].class != Class::Reject)
                    .map(|(_, case)| {
                        Term::and(vec![
                            pre_state.clone(),
                            case.selected.clone(),
                            case.constructed.clone(),
                            Term::and(case.laws.iter().map(|(_, t)| t.clone()).collect()),
                        ])
                    })
                    .collect(),
            );
            let reached = ask(
                &e.script,
                commits,
                &format!("lemma-{index}-nonvacuity"),
                solve,
                |m| {
                    let row = decode(&c, m)?;
                    let actual = native::evaluate(&c, &schema, &row).map_err(|e| e.to_string())?;
                    if matches!(&actual,evaluate::Outcome::Decision(d) if d.class!=Class::Reject) {
                        Ok(witness(&c, &row, &actual))
                    } else {
                        Err("committing witness did not replay".into())
                    }
                },
            );
            if reached["status"] != "refuted" {
                result["reason"] = json!(
                    "no dual-solver native committing witness; transition scope may be vacuous"
                );
                result["nonvacuity"] = reached;
                results.push(result);
                continue;
            }
            result["nonvacuity"] = reached;
        }
        let mut queries = Vec::new();
        for (i, (case, p)) in e.cases.iter().zip(&post).enumerate() {
            if c.branches[i].class == Class::Reject {
                continue;
            }
            let mut bad = vec![
                pre_state.clone(),
                case.selected.clone(),
                case.constructed.clone(),
                Term::and(case.laws.iter().map(|(_, t)| t.clone()).collect()),
                Term::not(p.clone()),
            ];
            if !transition {
                bad.push(pre.clone());
            }
            queries.push(ask(
                &e.script,
                Term::and(bad),
                &format!("lemma-{index}-case-{i}"),
                solve,
                |m| {
                    let row = decode(&c, m)?;
                    if !transition
                        && native_law(&c, &schema, &law, &row, true).map_err(|e| e.to_string())?
                            != Some(true)
                    {
                        return Err("lemma precondition did not replay".into());
                    }
                    if native_law(&c, &schema, &law, &row, false).map_err(|e| e.to_string())?
                        != Some(false)
                    {
                        return Err("native admitted transition did not refute lemma".into());
                    }
                    let actual = native::evaluate(&c, &schema, &row).map_err(|e| e.to_string())?;
                    Ok(witness(&c, &row, &actual))
                },
            ));
        }
        result["status"] = json!(if queries.iter().any(|q| q["status"] == "UNKNOWN") {
            "UNKNOWN"
        } else if queries.iter().any(|q| q["status"] == "refuted") {
            if transition {
                "refuted"
            } else {
                "not-inductive"
            }
        } else {
            "solver-checked"
        });
        result["evidence"] = json!(if result["status"] == "solver-checked" {
            MODE
        } else {
            "none"
        });
        result["obligations"] = json!(queries);
        results.push(result);
    }
    let status = if results.iter().any(|r| r["status"] == "UNKNOWN") {
        "UNKNOWN"
    } else if results.iter().any(|r| r["status"] == "refuted") {
        "refuted"
    } else if results.iter().any(|r| r["status"] == "not-inductive") {
        "not-inductive"
    } else {
        "solver-checked"
    };
    Ok(
        json!({"mode":"symbolic","status":status,"lemmas":results,"authority":"none","evidence":if status=="solver-checked" {MODE} else {"none"}}),
    )
}
