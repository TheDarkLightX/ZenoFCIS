//! Offline internal trace-game v1 fixture harness; no publication or adoption API.
#![forbid(unsafe_code)]
#[path = "trace_game/controls.rs"]
mod controls;
#[path = "trace_game/engine.rs"]
mod engine;
#[path = "trace_game/models.rs"]
mod models;
#[path = "trace_game/monitor.rs"]
mod monitor;
#[path = "trace_game/native.rs"]
mod native;
#[path = "trace_game/spot.rs"]
mod spot;

use engine::{Limits, Model};
use serde_json::{Value, json};
use std::{fs, path::Path};
use zeno_fcis_loop_doors::{
    expr::{self, Allow},
    version_space::{self as door, Budget, Proposal, Specification},
};
use zeno_fcis_synthesis::finite::Domain;

fn limits(horizon: u8) -> Limits {
    Limits {
        horizon,
        transitions: 200_000,
        frontier: 120_000,
        nodes: 200_000,
        deadline_after_work: u64::MAX,
        wall_millis: None,
    }
}

fn proposals() -> Result<(Value, String), String> {
    let mut specification = Specification {
        names: vec!["actor".into()],
        inputs: vec![Domain::Int { min: 0, max: 1 }],
        outputs: vec![Domain::Bool],
        sketch: vec![
            expr::parse(
                "?{true | actor == 0 | false}",
                0,
                Allow {
                    holes: true,
                    params: false,
                    keep: false,
                },
            )
            .map_err(|e| e.to_string())?,
        ],
        requirements: vec![],
    };
    let mut budget = Budget::new(door::Limits::default()).map_err(|e| format!("{e:?}"))?;
    let mut questions = vec![];
    for _ in 0..3 {
        match door::analyze(&specification, &mut budget).map_err(|e| format!("{e:?}"))? {
            Proposal::Question { count, question } => {
                let output = vec![i128::from(question.input[0] == 0)];
                questions.push(json!({"count":count,"input":question.input,"answer":output,"source":"frozen approved owner/actor relation; not inferred from attack labels"}));
                specification.requirements.push(door::Requirement::Example {
                    id: format!("approved_{}", questions.len()),
                    input: question.input,
                    output,
                });
            }
            Proposal::Singleton(meaning) => {
                if meaning.table != vec![vec![1], vec![0]] {
                    return Err("door guard disagrees with approved relation".into());
                }
                let guard = expr::print_symbolic(&meaning.expressions[0]);
                specification.requirements.push(door::Requirement::Example {
                    id: "conflicting_answer_control".into(),
                    input: vec![0],
                    output: vec![0],
                });
                if !matches!(
                    door::analyze(&specification, &mut budget),
                    Ok(Proposal::Empty { .. })
                ) {
                    return Err("question conflict not preserved".into());
                }
                return Ok((
                    json!({"door":"existing requirements-only version space","count":3,"questions":questions,
                    "approved_grammar":["true","actor == 0","false"],"selected_guard":guard,"selected_table":meaning.table,"conflicting_answer":"empty version space; requirements unchanged",
                    "work":budget.work(),"authority":"none; native rebuilt candidate and independent game checks decide eligibility",
                    "intentional_change":"guard actor == owner; no equivalence to vulnerable original is claimed"}),
                    guard,
                ));
            }
            _ => return Err("approved repair grammar exhausted".into()),
        }
    }
    Err("door did not converge within declared grammar".into())
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn regressions(model: &impl Model, run: &engine::Run) -> Result<Vec<Value>, String> {
    let mut records = vec![];
    for (property, id) in &run.witnesses {
        let node = &run.nodes[*id];
        let start = model.starts()[node.start].clone();
        let original = engine::trace(run, *id);
        let minimized = engine::minimize(model, &start, node.coalition, &original, property);
        if !engine::replay(model, &start, node.coalition, &original, property)
            || !engine::replay(model, &start, node.coalition, &minimized, property)
        {
            return Err("regression replay".into());
        }
        records.push(json!({"property":property,"start":start,"coalition":node.coalition,"original":original,"minimized":minimized,
            "minimality":"contiguous deletions and zero argument shrinks; no global shortest claim"}));
    }
    Ok(records)
}

fn retain(
    fixture: &models::Fixture,
    setting: Limits,
    out: &Path,
    label: &str,
) -> Result<Value, String> {
    let run = engine::search(fixture, setting);
    let regressions = regressions(fixture, &run)?;
    let corpus = corpus(fixture)?;
    let mut metadata = fixture.metadata();
    metadata["horizon"] = json!(setting.horizon);
    if fixture.name == "withdrawal" {
        metadata["deadline"]["request_start_at_most"] =
            json!(setting.horizon.saturating_sub(2).min(4));
    }
    let summary = json!({"label":label,"status":run.status,"stop":run.stop,"completed_depth":run.completed_depth,
        "transitions":run.transitions,"nodes":run.nodes.len(),"completed_requests":run.completed_requests,
        "open_requests":run.open_requests,"late_requests":run.late_requests,"maximum_loss":run.maximum_loss,"witnesses":regressions,"corpus":corpus});
    write(
        &out.join(format!("{label}.json")),
        &json!({"schema":"zeno-fcis/trace-game/1","metadata":metadata,"limits":setting,"table":fixture.table.json(),"run":run,"regressions":regressions,"corpus":corpus}),
    )?;
    println!(
        "{label}: {} / depth {} / {} transitions",
        summary["status"], summary["completed_depth"], summary["transitions"]
    );
    Ok(summary)
}

fn corpus(fixture: &models::Fixture) -> Result<Value, String> {
    let mut state = fixture.starts().last().ok_or("corpus start")?.clone();
    let (requests, expected) = if fixture.name == "withdrawal" {
        (
            vec![
                vec![0, 0, 0, 1],
                vec![0, 0, 0, 1],
                vec![1, 0, 0, 1],
                vec![2, 0, 0, 1],
                vec![3, 0, 0, 1],
                vec![3, 0, 0, 1],
            ],
            vec![
                ("Accept", None, vec![2, 1, 0, 0]),
                ("Reject", Some(202), vec![2, 1, 0, 0]),
                ("Accept", None, vec![1, 0, 1, 0]),
                ("Accept", None, vec![1, 0, 1, 1]),
                ("Accept", None, vec![0, 0, 2, 0]),
                ("Reject", Some(202), vec![0, 0, 2, 0]),
            ],
        )
    } else {
        (
            vec![
                vec![1, 2, 0, 0, 0],
                vec![2, 0, 0, 0, 0],
                vec![4, 0, 0, 0, 0],
                vec![4, 0, 0, 0, 0],
            ],
            vec![
                ("Accept", None, vec![0, 1, 0, 2, 0, 0]),
                ("Accept", None, vec![0, 1, 0, 2, 0, 0]),
                ("Accept", None, vec![0, 1, 0, 1, 1, 0]),
                ("Accept", None, vec![0, 1, 0, 0, 2, 0]),
            ],
        )
    };
    let mut rows = vec![];
    let mut pass = true;
    for (request, (kind, reason, post)) in requests.into_iter().zip(expected) {
        let actual = fixture.table.step(&state, &request)?;
        let agrees = actual.kind == kind && actual.reason == reason && actual.post == post;
        rows.push(json!({"pre":state,"request":request,"expected":{"kind":kind,"reason":reason,"post":post},"actual":actual,"pass":agrees}));
        pass &= agrees;
        state = actual.post;
    }
    Ok(
        json!({"name":if fixture.name=="withdrawal"{"request-retry-alarm-recovery-exit-boundary"}else{"listing-price-withdrawal-exit"},"pass":pass,"rows":rows}),
    )
}

fn execute(out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for fault in ["lawful", "rollback-committed", "commit-rejected-prefix"] {
        for disabled in [None, Some("composition-frame"), Some("unauthorized-commit")] {
            let fixture = controls::fixture(fault, disabled)?;
            for row in &fixture.table.rows {
                if row.outcome != controls::expected(&row.state, &row.request) {
                    return Err("complete-failure-semantics-table".into());
                }
            }
            let run = engine::search(&fixture, limits(2));
            if (fault != "lawful" && disabled != Some("composition-frame"))
                != (run.status == "counterexample")
            {
                return Err("failure-control-did-not-fire".into());
            }
            let witnesses=run.witnesses.iter().map(|(p,id)|{
                let n=&run.nodes[*id];let t=engine::trace(&run,*id);
                json!({"property":p,"coalition":n.coalition,"trace":t,"replayed":engine::replay(&fixture,&[0,0,0,1],n.coalition,&t,p)})
            }).collect::<Vec<_>>();
            write(
                &out.join(format!(
                    "semantics-{fault}-{}.json",
                    disabled.unwrap_or("enabled")
                )),
                &json!({"schema":"zeno-fcis/trace-game-semantics-control/1","metadata":fixture.metadata(),"table":fixture.table.json(),"regressions":regressions(&fixture,&run)?,"run":run,"witnesses":witnesses}),
            )?;
        }
    }
    let spot_input =
        serde_json::from_slice(&fs::read(out.join("spot-input.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut spot_control = spot::Control::new(spot_input)?;
    for disabled in [
        None,
        Some("coalition-gain"),
        Some("realized-coalition-gain"),
        Some("composition-frame"),
    ] {
        spot_control.disabled_objective = disabled.map(str::to_owned);
        let run = engine::search(&spot_control, limits(3));
        write(
            &out.join(format!("spot-{}.json", disabled.unwrap_or("control"))),
            &json!({"schema":"zeno-fcis/trace-game-spot-control/1","input":spot_control.packet(),
                "regressions":regressions(&spot_control,&run)?,"run":run}),
        )?;
    }
    let mut summaries = vec![];
    let (grammar, guard) = proposals()?;
    let mut lawful = models::core_repair(&guard)?;
    // Every candidate, including over-refusal shortcuts, gets both directions.
    for fault in [
        "unauthorized",
        "lawful",
        "reject-all",
        "refusal",
        "wrong-effect",
        "alarm-reset",
        "stale",
        "arithmetic",
        "accounting",
    ] {
        if fault == "lawful" {
            summaries.push(retain(&lawful, limits(4), out, "core-lawful")?);
        } else {
            let fixture = models::core(fault)?;
            summaries.push(retain(&fixture, limits(4), out, &format!("core-{fault}"))?);
        }
    }
    let deny_withdraw = models::core_guard_control("false", "deny-withdraw")?;
    summaries.push(retain(
        &deny_withdraw,
        limits(4),
        out,
        "core-deny-withdraw",
    )?);
    for fault in ["vulnerable", "stale", "lawful"] {
        let fixture = models::composition(fault)?;
        summaries.push(retain(
            &fixture,
            limits(4),
            out,
            &format!("composition-{fault}"),
        )?);
    }
    summaries.push(retain(&lawful, limits(5), out, "core-next-bound")?);
    summaries.push(retain(
        &models::composition("lawful")?,
        limits(5),
        out,
        "composition-next-bound",
    )?);
    let mut joined = models::composition("vulnerable")?;
    joined.only_objective = Some("coalition-gain".into());
    let full = joined.powers;
    let mut key_only = full;
    key_only.oracle_choices = 1;
    let mut price_only = full;
    price_only.signers = 0;
    if !full.includes(key_only)
        || !full.includes(price_only)
        || key_only.join(price_only) != full
        || !key_only.includes(key_only.meet(price_only))
    {
        return Err("capability-lattice".into());
    }
    for (label, power) in [
        ("key-only", key_only),
        ("price-only", price_only),
        ("joined", full),
    ] {
        joined.powers = power;
        summaries.push(retain(&joined, limits(4), out, label)?);
    }
    // Incomplete runs are permanent artifacts, including measured refusal work.
    for (label, setting) in [
        (
            "wall-time-stop",
            Limits {
                wall_millis: Some(0),
                ..limits(4)
            },
        ),
        (
            "node-stop",
            Limits {
                nodes: 7,
                ..limits(4)
            },
        ),
        (
            "work-stop",
            Limits {
                transitions: 7,
                ..limits(4)
            },
        ),
        (
            "time-stop",
            Limits {
                deadline_after_work: 7,
                ..limits(4)
            },
        ),
        (
            "frontier-stop",
            Limits {
                frontier: 7,
                ..limits(4)
            },
        ),
    ] {
        summaries.push(retain(&lawful, setting, out, label)?);
    }
    lawful.unfair_delay = true;
    summaries.push(retain(
        &lawful,
        Limits {
            transitions: 100_000,
            ..limits(4)
        },
        out,
        "unfair-delay",
    )?);
    lawful.unfair_delay = false;
    let mut controls = vec![];
    // Each one-event fault violates just its assigned objective. All other
    // objectives remain enabled; an unrelated disabled objective still catches it.
    for (fault, objective) in [
        ("unauthorized", "unauthorized-value"),
        ("stale", "stale-approval"),
        ("accounting", "accounting-loss"),
    ] {
        let mut fixture = models::core(fault)?;
        for (label, disabled) in [
            ("control", None),
            ("disabled", Some(objective)),
            ("unrelated", Some("oracle-integrity")),
        ] {
            fixture.disabled_objective = disabled.map(str::to_owned);
            let record = retain(
                &fixture,
                limits(1),
                out,
                &format!("{label}-core-{objective}"),
            )?;
            if (label == "disabled") != (record["status"] == "checked-through-K") {
                return Err(format!("load-bearing control:{objective}"));
            }
            controls.push(record);
        }
    }
    let mut vulnerable = models::core("unauthorized")?;
    vulnerable.only_objective = Some("unauthorized-value".into());
    let attack = engine::search(&vulnerable, limits(2));
    let id = *attack
        .witnesses
        .get("unauthorized-value")
        .ok_or("no original attack")?;
    let node = &attack.nodes[id];
    let trace = engine::trace(&attack, id);
    let start = &vulnerable.starts()[node.start];
    if engine::replay(&lawful, start, node.coalition, &trace, "unauthorized-value") {
        return Err("repair-regression".into());
    }
    write(
        &out.join("summary.json"),
        &json!({"schema":"zeno-fcis/trace-game-summary/1","runs":summaries,"assigned_controls":controls,
        "repair_door":grammar,"retained_regression":{"start":start,"coalition":node.coalition,"trace":trace,"original_violates":true,"candidate_blocks":true},
        "monitor":monitor::suggestions()?,"catalogue":{"X0":"legitimacy","X1":"accounting-loss","X2":"Spot mint control only; general share inflation omitted",
            "X3":"Spot rounding specification refutation only","X4":"coalition-gain and Spot control","X5":"oracle-integrity (two prices only)","X6":"bad-collateral",
            "X7":"privileged-authority","X8":"stale-approval (no nonce replay model)","X9":"insolvency","X10":"exit-deadline"},
        "dependency_limits":{"a63":"reason-specific law patch absent; independent complete scalar relation required","zal_t":"lowering patch absent; monitor reading inert","refine_check":"not present; not used"},
        "claim":"checked up to K under the declared capabilities","reductions":"none","adoption":false}),
    )?;
    println!("trace-game artifacts written; independent replay is still required");
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: trace_game OUTPUT_DIRECTORY [--tables-only]");
        std::process::exit(64);
    }
    let result = if args.len() == 3 && args[2] == "--tables-only" {
        (|| {
            fs::create_dir_all(&args[1]).map_err(|e| e.to_string())?;
            for (name, fixture) in [
                ("lawful", models::core("lawful")?),
                (
                    "guard-true",
                    models::core_guard_control("true", "unauthorized")?,
                ),
                (
                    "guard-false",
                    models::core_guard_control("false", "deny-withdraw")?,
                ),
                ("refusal", models::core("refusal")?),
                ("arithmetic", models::core("arithmetic")?),
                ("vulnerable-composition", models::composition("vulnerable")?),
                ("lawful-composition", models::composition("lawful")?),
            ] {
                write(
                    &Path::new(&args[1]).join(format!("{name}.json")),
                    &json!({"metadata":fixture.metadata(),"table":fixture.table.json()}),
                )?;
            }
            Ok(())
        })()
    } else if args.len() == 2 {
        execute(Path::new(&args[1]))
    } else {
        Err("unknown mode".into())
    };
    if let Err(error) = result {
        eprintln!("trace-game: {error}");
        std::process::exit(1);
    }
}
