//! Explicit, read-only usage knowledge acquisition and queries. LLM-generated.
use crate::transform::sha256_hex;
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use zeno_fcis_loop_doors::strategy_knowledge as K;

#[derive(Subcommand)]
pub(crate) enum Operation {
    /// Create an empty private assessment layer.
    Init {
        #[arg(long)]
        out: PathBuf,
    },
    /// Preserve legacy curator knowledge as pending, without blessing strategy advice.
    Migrate {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Normalise and deduplicate explicit candidate records; never promotes.
    Curate {
        #[arg(long)]
        source: PathBuf,
    },
    /// Replay candidate ON/OFF, or all promoted lessons against none.
    Assess {
        #[arg(long)]
        suite: PathBuf,
        #[arg(long)]
        candidate: Option<String>,
        #[arg(long, default_value_t = 100_000_000)]
        total_budget: u64,
        #[arg(long, default_value_t = 0)]
        minimum_effect_bps: u16,
        #[arg(long)]
        annotations: Option<PathBuf>,
    },
    /// Pending candidates ordered by observational value; no promotion authority.
    AssessmentQueue {
        #[arg(long)]
        suite: Option<PathBuf>,
    },
    /// Read-only assessment and retraction history.
    AssessmentView,
    /// Propose only declared inheritance lifts and episode-guided narrower scopes.
    Grow,
    /// Import bounded private episode records as observations, never as advice.
    IngestEpisodes {
        #[arg(long)]
        source: PathBuf,
    },
    /// Persist minimal pending retractions after dependency or version changes.
    Refresh,
    /// Summarise private outcome records; observations never promote.
    Telemetry,
    /// Descriptive loop value profiles. Points carry no promotion authority.
    Leaderboard {
        #[arg(long,default_value="held-out",value_parser=["held-out","observational"])]
        population: String,
    },
    /// Replay an exact finite proof or counterexample; no assessment is needed for this fact.
    Certify {
        #[arg(long)]
        original: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long, default_value = "{}")]
        scope: String,
        #[arg(long)]
        refutes: Option<String>,
    },
    /// Exact-subject certified results. Receipt replay is required before reuse.
    Facts {
        #[arg(long)]
        subject: String,
    },
    /// Recheck a recorded equivalence receipt for exactly this subject.
    Reuse {
        #[arg(long)]
        fact: String,
        #[arg(long)]
        subject: PathBuf,
    },
    #[command(hide = true)]
    AssessmentTask {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// All current lessons matching the structured task facts.
    Advise {
        #[arg(long)]
        task: String,
    },
    /// A shortest public type path respecting task blockers and limits.
    Plan {
        #[arg(long)]
        have: String,
        #[arg(long)]
        want: String,
        #[arg(long)]
        task: String,
    },
    /// Replan after a step; supply only actually produced artifact types.
    Replan {
        #[arg(long)]
        have: String,
        #[arg(long)]
        want: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "")]
        produced: String,
    },
    /// A query and its complete finite derivation in this task context.
    Explain {
        query: String,
        #[arg(long, default_value = "")]
        task: String,
    },
    /// Check the catalog, lesson evidence and unresolved conflicts.
    Check,
    /// Query a relation; each answer includes its derivation.
    Query { query: String },
    /// Replay a bounded approved recipe into a new private validation file.
    Replay {
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}
/// Owned acquired bytes; never a decision or publication capability.
pub(crate) struct Loaded {
    pub(crate) sources: Vec<String>,
    pub(crate) bindings: Vec<Value>,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) validations: Vec<K::Validation>,
    pub(crate) validation_problems: Vec<Value>,
    pub(crate) preparation_work: u64,
    pub(crate) assessed: Vec<crate::knowledge_store::Layer>,
}
fn fail(code: &'static str) -> Value {
    json!({"schema":K::USAGE_SCHEMA,"status":if code.ends_with("budget"){"inconclusive"}else{"refused"},"authority":"none","reason":code})
}
pub(crate) fn paths(given: &[PathBuf]) -> Result<Vec<PathBuf>, Value> {
    let out = if given.is_empty() {
        std::env::var_os("ZENO_FCIS_KB")
            .map(|s| std::env::split_paths(&s).collect())
            .unwrap_or_default()
    } else {
        given.to_vec()
    };
    if out.len() > 7 {
        return Err(fail("kb-layer-budget"));
    }
    Ok(out)
}
pub(crate) fn checkout(path: &Path) -> bool {
    path.ancestors().any(|p| {
        p.join("crates/zeno-fcis-cli/Cargo.toml").is_file()
            && p.join("crates/zeno-fcis-loop-doors/Cargo.toml").is_file()
    })
}
pub(crate) fn load(paths: &[PathBuf]) -> Result<Loaded, Value> {
    load_with_limits(paths, K::Limits::default())
}
pub(crate) fn load_with_limits(paths: &[PathBuf], limits: K::Limits) -> Result<Loaded, Value> {
    load_with_checker(paths, limits, None)
}
fn load_with_checker(
    paths: &[PathBuf],
    limits: K::Limits,
    checker: Option<&str>,
) -> Result<Loaded, Value> {
    if paths.len() > 7 {
        return Err(fail("kb-layer-budget"));
    }
    let mut loaded = Loaded {
        sources: vec![K::EXAMPLE.to_owned()],
        bindings: vec![
            json!({"path":"compiled-public-example","sha256":sha256_hex(K::EXAMPLE.as_bytes())}),
        ],
        paths: Vec::new(),
        validations: Vec::new(),
        validation_problems: Vec::new(),
        preparation_work: 0,
        assessed: Vec::new(),
    };
    let (public_validations, preparation_work) = public_validation(limits)?;
    loaded.validations = public_validations;
    loaded.preparation_work = preparation_work;
    (|| {
        let mut bytes = K::EXAMPLE.len();
        let mut validation_files = 0usize;
        let mut checker_sha = checker.map(str::to_owned);
        for p in paths {
            if p.to_str().is_none() {
                return Err(fail("kb-path-utf8"));
            }
            let resolved = std::fs::canonicalize(p).map_err(|_| fail("kb-read"))?;
            if resolved.to_str().is_none() {
                return Err(fail("kb-path-utf8"));
            }
            if checkout(&resolved) {
                return Err(fail("kb-path-must-be-outside-checkout"));
            }
            let metadata = std::fs::symlink_metadata(p).map_err(|_| fail("kb-read"))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(fail("kb-path-must-be-regular-file"));
            }
            let data = crate::transform_command::read_bounded(&resolved, 65_536)
                .map_err(|_| fail("kb-read"))?;
            bytes = bytes.saturating_add(data.len());
            if bytes > 65_536 {
                return Err(fail("kb-text-budget"));
            }
            loaded
                .bindings
                .push(json!({"path":resolved,"sha256":sha256_hex(&data)}));
            let text = if data.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
                if serde_json::from_slice::<Value>(&data)
                    .ok()
                    .is_some_and(|v| v["schema"] == "zeno-fcis/kb-layer/2")
                {
                    let (layer, _) = crate::knowledge_store::live_with_checker(
                        &resolved,
                        checker_sha.as_deref(),
                    )
                    .map_err(fail)?;
                    loaded.preparation_work = loaded.preparation_work.saturating_add(
                        serde_json::to_vec(&layer)
                            .map_err(|_| fail("kb-encoding"))?
                            .len() as u64,
                    );
                    if let Some(binding) = loaded.bindings.last_mut() {
                        binding["effective_sha256"] =
                            json!(crate::knowledge_store::hash(&layer).map_err(fail)?);
                    }
                    loaded.assessed.push(layer);
                    loaded.paths.push(resolved);
                    loaded.sources.push(String::new());
                    continue;
                }
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Layer {
                    schema: String,
                    knowledge: crate::knowledge_evidence::Input,
                    validations: Vec<crate::knowledge_evidence::Input>,
                }
                let layer: Layer =
                    serde_json::from_slice(&data).map_err(|_| fail("kb-layer-schema"))?;
                if layer.schema != "zeno-fcis/kb-layer/1" || layer.validations.len() > 128 {
                    return Err(fail("kb-layer-schema"));
                }
                let root = resolved.parent().ok_or_else(|| fail("kb-layer-parent"))?;
                let knowledge = if layer.knowledge.path.is_absolute() {
                    layer.knowledge.path.clone()
                } else {
                    root.join(&layer.knowledge.path)
                };
                let knowledge = std::fs::canonicalize(&knowledge).map_err(|_| fail("kb-read"))?;
                if checkout(&knowledge) {
                    return Err(fail("kb-path-must-be-outside-checkout"));
                }
                let content =
                    crate::knowledge_evidence::bound_input(root, &layer.knowledge, 65_536)?;
                for validation in layer.validations {
                    validation_files += 1;
                    if validation_files > 32 {
                        return Err(fail("kb-validation-budget"));
                    }
                    if checker_sha.is_none() {
                        checker_sha = Some(crate::knowledge_evidence::checker_sha256()?);
                    }
                    match crate::knowledge_evidence::load(root, &validation, checker_sha.as_deref())
                    {
                        Ok(_) => loaded
                            .validation_problems
                            .push(json!({"reason":"kb-strategy-assessment-required"})),
                        Err(e) => loaded.validation_problems.push(e),
                    }
                }
                bytes = bytes.saturating_add(content.len());
                if bytes > 65_536 {
                    return Err(fail("kb-text-budget"));
                }
                String::from_utf8(content).map_err(|_| fail("kb-utf8"))?
            } else {
                String::from_utf8(data).map_err(|_| fail("kb-utf8"))?
            };
            loaded.paths.push(resolved);
            loaded.sources.push(text);
        }
        Ok(loaded)
    })()
    .map_err(|mut e| {
        e["work"] = json!(preparation_work);
        e
    })
}

pub(crate) fn run(
    loaded: &Loaded,
    request: &K::Request,
    limits: K::Limits,
) -> Result<K::Report, zeno_fcis_loop_doors::strategy_algebra::Stop> {
    if loaded.preparation_work > limits.work {
        return Err(zeno_fcis_loop_doors::strategy_algebra::Stop {
            code: "kb-work-budget",
            inconclusive: true,
            work: loaded.preparation_work,
            trace: Vec::new(),
        });
    }
    let mut remaining = limits;
    remaining.work -= loaded.preparation_work;
    let result = K::run_with_evidence(
        &loaded
            .sources
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        request,
        remaining,
        &loaded.validations,
    );
    result
        .map(|mut r| {
            r.work = r.work.saturating_add(loaded.preparation_work);
            if crate::knowledge_workflow::layer_conflict(&loaded.assessed) {
                match &mut r.outcome {
                    K::Outcome::Checked(problems) => problems.push("kb-assessed-layer-conflict"),
                    K::Outcome::Advised(advice) => {
                        advice.problems.push("kb-assessed-layer-conflict");
                        advice.lessons.clear();
                        advice.facts.clear();
                    }
                    _ => {}
                }
            }
            r
        })
        .map_err(|mut e| {
            e.work = e.work.saturating_add(loaded.preparation_work);
            e
        })
}
fn constant(v: &K::Value) -> Value {
    match v {
        K::Value::Text(s) => json!(s),
        K::Value::Integer(n) => json!(n),
    }
}
fn fact(f: &K::Fact) -> Value {
    json!({"predicate":f.predicate,"arguments":f.arguments.iter().map(constant).collect::<Vec<_>>()})
}
fn explanation(e: &K::Explanation) -> Value {
    json!({"conclusion":fact(&e.conclusion),"trace":e.trace.iter().map(|d|json!({"fact":fact(&d.fact),"rule":d.rule,"premises":d.premises.iter().map(fact).collect::<Vec<_>>(),"absent":d.absent.iter().map(|a|json!({"predicate":a.predicate,"arguments":a.arguments.iter().map(|v|v.as_ref().map(constant)).collect::<Vec<_>>()})).collect::<Vec<_>>(),"comparisons":d.comparisons.iter().map(|c|json!({"operator":c.operator,"left":constant(&c.left),"right":constant(&c.right)})).collect::<Vec<_>>(),"source_layer":d.source_layer,"bindings":d.bindings.iter().map(|(k,v)|(k.clone(),constant(v))).collect::<serde_json::Map<_,_>>()})).collect::<Vec<_>>()})
}
fn exclusions(es: &[K::Exclusion]) -> Value {
    json!(es.iter().map(|e|json!({"step":e.step,"lesson":constant(&e.lesson),"reason":e.reason,"enforced":e.enforced,"constraint":fact(&e.constraint),"explanation":explanation(&e.explanation),"constraint_trace":explanation(&e.constraint_trace),"observed":e.observed.iter().map(explanation).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
fn implementation() -> String {
    let sources = [
        include_str!("knowledge_command.rs"),
        include_str!("knowledge_evidence.rs"),
        include_str!("loop_knowledge.rs"),
        include_str!("loop_command.rs"),
        include_str!("loop_pipeline.rs"),
        include_str!("main.rs"),
        include_str!("checker_process.rs"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_knowledge.rs.txt"),
        include_str!(
            "../source-snapshots/zeno-fcis-loop-doors/src/strategy_knowledge/reasoning.rs.txt"
        ),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_algebra.rs.txt"),
        K::EXAMPLE,
    ];
    let mut bytes = Vec::new();
    for s in sources {
        bytes.extend_from_slice(&(s.len() as u64).to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
    }
    sha256_hex(&bytes)
}
pub(crate) fn report(
    loaded: &Loaded,
    result: Result<K::Report, zeno_fcis_loop_doors::strategy_algebra::Stop>,
) -> (u8, Value) {
    let r = match result {
        Ok(r) => r,
        Err(s) => {
            return (
                if s.inconclusive { 2 } else { 1 },
                json!({"schema":K::USAGE_SCHEMA,"authority":"none","status":if s.inconclusive {"inconclusive"}else{"refused"},"reason":s.code,"work":s.work,"layers":loaded.bindings}),
            );
        }
    };
    let mut v = json!({"schema":K::USAGE_SCHEMA,"authority":"none","implementation_sha256":implementation(),"work":r.work,"rows":r.rows,"states":r.states,"layers":loaded.bindings,"layer_conflicts":r.layer_conflicts,"overrides":r.overrides.iter().map(|o|json!({"key":o.key,"earlier_layer":o.earlier_layer,"later_layer":o.later_layer,"earlier":o.earlier.iter().map(fact).collect::<Vec<_>>(),"later":o.later.iter().map(fact).collect::<Vec<_>>()})).collect::<Vec<_>>(),"validation_problems":loaded.validation_problems,"validations":loaded.validations.iter().map(|e|json!({"lesson":e.lesson,"recipe_sha256":e.recipe_sha256,"dependency_sha256":e.dependency_sha256,"validation_sha256":e.validation_sha256,"last_validation":e.last_validation,"status":format!("{:?}",e.status)})).collect::<Vec<_>>(),"explanations":r.explanations.iter().map(explanation).collect::<Vec<_>>()});
    let code = match r.outcome {
        K::Outcome::Query(rows) => {
            v["status"] = json!("proposed-query");
            v["answers"] = json!(
                rows.iter()
                    .map(|b| b
                        .iter()
                        .map(|(k, v)| (k.clone(), constant(v)))
                        .collect::<serde_json::Map<_, _>>())
                    .collect::<Vec<_>>()
            );
            0
        }
        K::Outcome::Checked(problems) => {
            v["status"] = json!(if problems.is_empty() {
                "consistent"
            } else {
                "inconsistent"
            });
            v["problems"] = json!(problems);
            if problems.is_empty() { 0 } else { 1 }
        }
        K::Outcome::Advised(a) => {
            v["status"] = json!("advised");
            let assessed = if a.problems.is_empty() {
                crate::knowledge_workflow::advice(&loaded.assessed, &a.task_facts)
            } else {
                vec![]
            };
            v["assessed_advice"] = json!(assessed);
            v["lessons"] = json!(
                a.lessons
                    .iter()
                    .map(constant)
                    .chain(assessed.iter().map(|c| json!(c.id)))
                    .collect::<Vec<_>>()
            );
            v["facts"] = json!(a.facts.iter().map(fact).collect::<Vec<_>>());
            v["task_facts"] = json!(a.task_facts.iter().map(fact).collect::<Vec<_>>());
            v["explanations"] = json!(a.explanations.iter().map(explanation).collect::<Vec<_>>());
            v["exclusions"] = exclusions(&a.exclusions);
            v["unknowns"] = json!(a.unknowns.iter().map(fact).collect::<Vec<_>>());
            v["pending"] = json!(a.pending.iter().map(constant).collect::<Vec<_>>());
            v["defeated"] = json!(a.defeated.iter().map(explanation).collect::<Vec<_>>());
            v["problems"] = json!(a.problems);
            0
        }
        K::Outcome::Planned(p) => {
            v["status"] = json!("proposed-plan");
            v["steps"] = json!(p.steps.iter().map(|s| s.name()).collect::<Vec<_>>());
            v["eligible_strategies"] = json!(p.eligible);
            v["selected_strategy"] = json!(p.selected);
            v["next_step"] = json!(p.steps.first().map(|s| s.name()));
            v["execution_rule"] = json!("execute-one-then-replan-from-observed-artifacts");
            v["ranking"]=json!(p.ranking.iter().map(|r|json!({"step":r.step,"preference_penalty":r.preference_penalty,"declared_work":r.declared_work,"explanations":r.explanations.iter().map(explanation).collect::<Vec<_>>()})).collect::<Vec<_>>());
            v["ranking_rule"] = json!(
                "fewest-steps; declared preference penalty, declared work, lexical frontier ties; no cost optimum"
            );
            v["exclusions"] = exclusions(&p.exclusions);
            0
        }
        K::Outcome::NoPlan(es) => {
            v["status"] = json!("inconclusive");
            v["reason"] = json!("kb-bounded-no-plan");
            v["exclusions"] = exclusions(&es);
            2
        }
    };
    (code, v)
}
pub(crate) fn execute(paths: &[PathBuf], op: Operation) -> u8 {
    let result = invoke(paths, op);
    crate::print_json(&result.1);
    result.0
}
pub(crate) fn invoke(given: &[PathBuf], op: Operation) -> (u8, Value) {
    invoke_with_identity(given, op, crate::knowledge_evidence::checker_sha256)
}
// The checker binary can be large and hashing time depends on the host. Its
// identity is acquired once before this unchanged checking-work window; it is
// still compared with every receipt, and expiration still fails inconclusive.
const CHECK_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
fn invoke_with_identity(
    given: &[PathBuf],
    op: Operation,
    identity: impl FnOnce() -> Result<String, Value>,
) -> (u8, Value) {
    if crate::knowledge_workflow::handles(&op) {
        return crate::knowledge_workflow::invoke(given, op);
    }
    if let Operation::Replay { recipe, out } = op {
        return crate::knowledge_evidence::replay(&recipe, &out);
    }
    let checker = match identity() {
        Ok(checker) => checker,
        Err(e) => return (1, e),
    };
    let started = std::time::Instant::now();
    let loaded = match paths(given)
        .and_then(|p| load_with_checker(&p, K::Limits::default(), Some(&checker)))
    {
        Ok(l) => l,
        Err(e) => {
            return (
                if e["status"] == "inconclusive" {
                    2
                } else if e["reason"] == "kb-read" {
                    3
                } else {
                    1
                },
                e,
            );
        }
    };
    let request = match op {
        Operation::Advise { task } => K::Request::Advise(task),
        Operation::Plan { have, want, task } => K::Request::PlanFor {
            have: have.split(',').map(|s| s.trim().to_owned()).collect(),
            want: want.split(',').map(|s| s.trim().to_owned()).collect(),
            task,
        },
        Operation::Replan {
            have,
            want,
            task,
            produced,
        } => K::Request::Replan {
            have: have
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            want: want
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            task,
            produced: produced
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        },
        Operation::Explain { query, task } => K::Request::Explain { query, task },
        Operation::Check => K::Request::Check,
        Operation::Query { query } => K::Request::Query(query),
        Operation::Replay { .. } => return (1, fail("kb-replay-routing")),
        _ => return (1, fail("kb-assessment-routing")),
    };
    let result = run(&loaded, &request, K::Limits::default());
    if started.elapsed() >= CHECK_WINDOW {
        return (
            2,
            json!({"schema":K::USAGE_SCHEMA,"authority":"none","status":"inconclusive","reason":"kb-deadline","layers":loaded.bindings}),
        );
    }
    report(&loaded, result)
}

/// Finite encoding replay for the compiled public declarations only. The
/// expected results are frozen in code, independent of the Datalog derivation.
/// This establishes these declared policies on the two fixtures, not host
/// availability, empirical usefulness, or a universal generalization.
fn public_validation(limits: K::Limits) -> Result<(Vec<K::Validation>, u64), Value> {
    let max_work = limits.work;
    let dependency = sha256_hex(include_bytes!(
        "../source-snapshots/zeno-fcis-loop-doors/src/strategy_algebra.rs.txt"
    ));
    let context = "task(tool,zeno-fcis).task(version,v241).";
    let fixtures = [
        (
            "check-inputs-old",
            "task(subject,check-program).",
            "task(subject,observe).",
            false,
            false,
        ),
        (
            "check-inputs",
            "task(subject,check-program).",
            "task(subject,observe).",
            false,
            false,
        ),
        (
            "containment",
            "task(subject,observe).task(environment,sandbox).",
            "task(subject,observe).task(environment,host).",
            true,
            false,
        ),
        (
            "learner-bound",
            "task(subject,learn-terms).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4096).",
            "task(subject,learn-terms).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4097).",
            false,
            true,
        ),
        (
            "diagnostic-order",
            "task(subject,observe).",
            "task(subject,learn-terms).",
            false,
            false,
        ),
    ];
    let mut used = 0u64;
    let mut admitted = Vec::new();
    for (id, positive, negative, pos_excluded, neg_excluded) in fixtures {
        let (statement,parse_work)=K::lesson_statement(K::EXAMPLE,id,K::Limits{work:max_work.saturating_sub(used),..limits}).map_err(|e|json!({"authority":"none","status":"inconclusive","reason":e.code,"work":used.saturating_add(e.work)}))?;
        used = used.saturating_add(parse_work);
        let mut input = K::Validation {
            statement,
            lesson: id.into(),
            recipe_sha256: dependency.clone(),
            dependency_sha256: dependency.clone(),
            validation_sha256: String::new(),
            mode: "exact-precondition".into(),
            family: "public-interface".into(),
            last_validation: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| fail("kb-public-recipe-clock"))?
                .as_secs(),
            status: K::EvidenceStatus::Validated,
        };
        let mut results = Vec::new();
        for (task, expected_present, expected_exclusion) in [
            (positive, true, pos_excluded),
            (
                negative,
                id == "containment" || id == "learner-bound",
                neg_excluded,
            ),
        ] {
            let task = format!("{context}{task}");
            let limits = K::Limits {
                work: max_work.saturating_sub(used),
                ..limits
            };
            let result = K::run_with_evidence(
                &[K::EXAMPLE],
                &K::Request::Advise(task.clone()),
                limits,
                core::slice::from_ref(&input),
            );
            let r = match result {
                Ok(r) => r,
                Err(e) => {
                    return Err(
                        json!({"authority":"none","status":"inconclusive","reason":e.code,"work":used.saturating_add(e.work)}),
                    );
                }
            };
            used = used.saturating_add(r.work);
            let K::Outcome::Advised(a) = r.outcome else {
                return Err(fail("kb-public-recipe-result"));
            };
            let present = a.lessons.contains(&K::Value::Text(id.into()));
            let excluded = a
                .exclusions
                .iter()
                .any(|e| e.enforced && e.lesson == K::Value::Text(id.into()));
            if present != expected_present
                || excluded != expected_exclusion
                || !a.problems.is_empty()
            {
                return Err(fail("kb-public-recipe-assertion"));
            }
            results.push(json!({"task":task,"present":present,"excluded":excluded,"work":r.work}));
        }
        let recipe = json!({"schema":"zeno-fcis/kb-recipe/1","runner":"compiled-public-policy-v1","lesson":id,"inputs_sha256":sha256_hex(K::EXAMPLE.as_bytes()),"source_sha256":dependency,"results":results});
        input.validation_sha256 = sha256_hex(
            &serde_json::to_vec(&recipe).map_err(|_| fail("kb-public-recipe-encoding"))?,
        );
        admitted.push(input);
    }
    Ok((admitted, used))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kb_slow_checker_identity_does_not_consume_check_window() {
        let started = std::time::Instant::now();
        let (exit, report) = invoke_with_identity(&[], Operation::Check, || {
            std::thread::sleep(CHECK_WINDOW + std::time::Duration::from_millis(100));
            Ok("a".repeat(64))
        });
        assert!(started.elapsed() >= CHECK_WINDOW);
        assert_eq!(exit, 0, "{report}");
        assert_eq!(report["status"], "consistent");
        assert_eq!(report["authority"], "none");
    }

    #[test]
    fn kb_checker_identity_failure_refuses() {
        let (exit, report) =
            invoke_with_identity(&[], Operation::Check, || Err(fail("kb-evidence-checker")));
        assert_eq!(exit, 1);
        assert_eq!(report["reason"], "kb-evidence-checker");
        assert_eq!(report["status"], "refused");
    }
}
