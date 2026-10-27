//! Two fixed synthetic adapters. Reference requirements never come from a proposer.
use super::{
    engine::*,
    native::{self, Table, case, leaf},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Powers {
    pub(crate) accounts: u8,
    pub(crate) signers: u8,
    pub(crate) oracle_choices: u8,
    pub(crate) approval_ages: u8,
    pub(crate) arbitrary_order: bool,
}
impl Powers {
    pub(crate) const FULL: Self = Self {
        accounts: 3,
        signers: 1,
        oracle_choices: 3,
        approval_ages: 3,
        arbitrary_order: true,
    };
    pub(crate) fn includes(self, other: Self) -> bool {
        self.accounts & other.accounts == other.accounts
            && self.signers & other.signers == other.signers
            && self.oracle_choices & other.oracle_choices == other.oracle_choices
            && self.approval_ages & other.approval_ages == other.approval_ages
            && (self.arbitrary_order || !other.arbitrary_order)
    }
    pub(crate) fn meet(self, other: Self) -> Self {
        Self {
            accounts: self.accounts & other.accounts,
            signers: self.signers & other.signers,
            oracle_choices: self.oracle_choices & other.oracle_choices,
            approval_ages: self.approval_ages & other.approval_ages,
            arbitrary_order: self.arbitrary_order && other.arbitrary_order,
        }
    }
    pub(crate) fn join(self, other: Self) -> Self {
        Self {
            accounts: self.accounts | other.accounts,
            signers: self.signers | other.signers,
            oracle_choices: self.oracle_choices | other.oracle_choices,
            approval_ages: self.approval_ages | other.approval_ages,
            arbitrary_order: self.arbitrary_order || other.arbitrary_order,
        }
    }
}

pub(crate) struct Fixture {
    pub(crate) name: String,
    pub(crate) fault: String,
    pub(crate) table: Table,
    pub(crate) powers: Powers,
    pub(crate) only_objective: Option<String>,
    pub(crate) disabled_objective: Option<String>,
    pub(crate) unfair_delay: bool,
}

pub(crate) fn core(fault: &str) -> Result<Fixture, String> {
    core_with_guard(fault, None)
}
pub(crate) fn core_repair(guard: &str) -> Result<Fixture, String> {
    core_with_guard("lawful", Some(guard))
}
pub(crate) fn core_guard_control(guard: &str, fault: &str) -> Result<Fixture, String> {
    core_with_guard(fault, Some(guard))
}
fn core_with_guard(fault: &str, guard: Option<&str>) -> Result<Fixture, String> {
    let mut cases = vec![];
    if fault == "reject-all" {
        cases.push(case("true", "Reject", Some(203), json!({})));
    }
    cases.push(case("!allowed", "Reject", Some(200), json!({})));
    let authorization = guard.map_or_else(
        || {
            if fault == "accounting" {
                "actor != 0 && op == 3".into()
            } else if fault == "unauthorized" {
                "actor != 0 && op == 0".into()
            } else {
                "actor != 0 && (op == 0 || op == 3)".into()
            }
        },
        |g| format!("(actor != 0 && op == 0) || (!({g}) && op == 3)"),
    );
    cases.push(case(&authorization, "Reject", Some(200), json!({})));
    if fault != "stale" {
        cases.push(case("op == 3 && age > 0", "Reject", Some(201), json!({})));
    }
    cases.push(case(
        "balance + paid > 2 || (pending && balance == 0)",
        "Reject",
        Some(203),
        json!({}),
    ));
    cases.push(case(
        "(op == 0 || op == 3) && (balance == 0 || pending)",
        "Reject",
        Some(202),
        json!({}),
    ));
    cases.push(case("op == 1 && !pending", "Reject", Some(202), json!({})));
    if fault == "alarm-reset" {
        cases.push(case("op == 1 && alarm", "Reject", Some(203), json!({})));
    }
    if fault == "accounting" {
        cases.push(case(
            "op == 0 && actor != 0",
            "Accept",
            None,
            json!({"110":"balance - 1"}),
        ));
    }
    cases.push(case("op == 0", "Accept", None, json!({"111":true})));
    let movement = if fault == "wrong-effect" {
        json!({"111":false,"113":false})
    } else if fault == "arithmetic" {
        json!({"110":"div_floor(balance, 0)"})
    } else {
        json!({"110":"balance - 1","111":false,"112":"paid + 1","113":false})
    };
    cases.push(case("op == 1 || op == 3", "Accept", None, movement));
    cases.push(case("op == 2", "Accept", None, json!({"113":true})));
    cases.push(case("true", "Accept", None, json!({"113":false})));
    let mut source = leaf(
        "withdrawal",
        &[
            ("balance", "units"),
            ("pending", "bool"),
            ("paid", "units"),
            ("alarm", "bool"),
        ],
        &[("op", "op"), ("actor", "units"), ("age", "units")],
        &[("allowed", "bool")],
        json!(cases),
        json!({"110":2,"111":false,"112":0,"113":false}),
    );
    if fault == "refusal" {
        // A law-induced technical refusal, not an approved rejection repair.
        source
            .project
            .push_str("law 501 refuses_legitimate on accept = command.101.120 != 0;\n");
        let mut rules: Value = serde_json::from_str(&source.rules).map_err(|e| e.to_string())?;
        rules["law_kinds"]["501"] = json!("DebitCreditEffectEquality");
        source.rules = rules.to_string();
    }
    if fault == "lawful" {
        let mut rules: Value = serde_json::from_str(&source.rules).map_err(|e| e.to_string())?;
        let unauthorized = "allowed == 0 || (actor != 0 && (op == 0 || op == 3))";
        let stale = "op == 3 && age > 0";
        let invalid = "balance + paid > 2 || (pending == 1 && balance == 0)";
        let unavailable =
            "((op == 0 || op == 3) && (balance == 0 || pending == 1)) || (op == 1 && pending == 0)";
        let eligible =
            format!("!({unauthorized}) && !({stale}) && !({invalid}) && !({unavailable})");
        let declared=[
            (501,"reason_unauthorized","reject",format!("decision_reason == 200 -> ({unauthorized})"),"RejectNoAuthority"),
            (502,"reason_stale","reject",format!("decision_reason == 201 -> !({unauthorized}) && ({stale})"),"RejectNoAuthority"),
            (503,"reason_invalid","reject",format!("decision_reason == 203 -> !({unauthorized}) && !({stale}) && ({invalid})"),"RejectNoAuthority"),
            (504,"reason_unavailable","reject",format!("decision_reason == 202 -> !({unauthorized}) && !({stale}) && !({invalid}) && ({unavailable})"),"RejectNoAuthority"),
            (505,"legitimate_acceptance","reject",format!("!({eligible}) && (decision_reason == 200 || decision_reason == 201 || decision_reason == 202 || decision_reason == 203)"),"RejectNoAuthority"),
            (506,"acceptance_eligibility","accept",eligible,"DebitCreditEffectEquality"),
            (507,"complete_frame","accept","((op == 1 || op == 3) -> post.100.110 == balance - 1 && post.100.111 == 0 && post.100.112 == paid + 1 && post.100.113 == 0) && (op == 0 -> post.100.110 == balance && post.100.111 == 1 && post.100.112 == paid && post.100.113 == alarm) && (op == 2 -> post.100.110 == balance && post.100.111 == pending && post.100.112 == paid && post.100.113 == 1) && ((op == 4 || op == 5) -> post.100.110 == balance && post.100.111 == pending && post.100.112 == paid && post.100.113 == 0)".into(),"DebitCreditEffectEquality"),
            (508,"no_unintended_failure","failure","false".into(),"CommittedFailureEffects")];
        let parsed = zeno_fcis_contract::internals::rules::Rules::read(&rules.to_string())
            .map_err(|e| e.to_string())?;
        for (id, name, scope, formula, kind) in declared {
            use zeno_fcis_contract::internals::expr;
            let formula = expr::render(&expr::expand(&expr::parse(&formula)?, &parsed.variables)?);
            source
                .project
                .push_str(&format!("law {id} {name} on {scope} = {formula};\n"));
            rules["law_kinds"][id.to_string()] = json!(kind);
        }
        source.rules = rules.to_string();
    }
    Ok(Fixture {
        name: "withdrawal".into(),
        fault: fault.into(),
        table: native::authority_table(&source)?,
        powers: Powers::FULL,
        only_objective: None,
        disabled_objective: None,
        unfair_delay: false,
    })
}

pub(crate) fn composition(fault: &str) -> Result<Fixture, String> {
    let gov = leaf(
        "governance",
        &[("admin", "bool")],
        &[("keys", "units"), ("age", "units")],
        &[("allowed", "bool")],
        json!([
            case("!allowed", "Reject", Some(200), json!({})),
            case(
                if fault == "lawful" {
                    "keys < 2"
                } else {
                    "keys == 0"
                },
                "Reject",
                Some(200),
                json!({})
            ),
            case(
                if fault == "stale" { "false" } else { "age > 0" },
                "Reject",
                Some(201),
                json!({})
            ),
            case("true", "Accept", None, json!({"110":true}))
        ]),
        json!({"110":false}),
    );
    let collateral = leaf(
        "collateral",
        &[("listed", "units")],
        &[("bad", "bool"), ("keys", "units")],
        &[("admin", "bool")],
        json!([
            case(
                if fault == "lawful" {
                    "bad"
                } else {
                    "bad && !admin"
                },
                "Reject",
                Some(203),
                json!({})
            ),
            case("!bad && keys < 2", "Reject", Some(200), json!({})),
            case("true", "Accept", None, json!({"110":"choose(bad, 2, 1)"}))
        ]),
        json!({"110":0}),
    );
    let oracle = leaf(
        "oracle",
        &[("price", "bool")],
        &[("skew", "bool")],
        &[("listed", "units")],
        json!([
            case("listed == 0", "Reject", Some(202), json!({})),
            case(
                if fault == "lawful" { "skew" } else { "false" },
                "Reject",
                Some(203),
                json!({})
            ),
            case("true", "Accept", None, json!({"110":"skew"}))
        ]),
        json!({"110":false}),
    );
    let vault = leaf(
        "vault",
        &[("reserve", "units"), ("paid", "units"), ("debt", "units")],
        &[("drain", "bool")],
        &[("listed", "units"), ("price", "bool")],
        json!([
            case("reserve == 0", "Reject", Some(202), json!({})),
            case(
                "drain && !(listed == 2 && price)",
                "Reject",
                Some(203),
                json!({})
            ),
            case(
                "drain",
                "Accept",
                None,
                json!({"110":0,"111":"paid + reserve","112":"debt + reserve"})
            ),
            case(
                "true",
                "Accept",
                None,
                json!({"110":"reserve - 1","111":"paid + 1"})
            )
        ]),
        json!({"110":2,"111":0,"112":0}),
    );
    let sources = vec![gov, collateral, oracle, vault];
    let bindings = [
        json!({"keys":"keys","age":"age","allowed":"true"}),
        json!({"bad":"bad","keys":"keys","admin":"governance.admin"}),
        json!({"skew":"skew","listed":"collateral.listed"}),
        json!({"drain":"op == 3","listed":"collateral.listed","price":"oracle.price"}),
    ];
    let names = ["governance", "collateral", "oracle", "vault"];
    let fields = [
        vec!["admin"],
        vec!["listed"],
        vec!["price"],
        vec!["reserve", "paid", "debt"],
    ];
    let mut components = vec![];
    for i in 0..4 {
        let [project, policy, program] = native::pins(&sources[i])?;
        components.push(json!({"name":names[i],"path":names[i],"project_sha256":project,"policy_sha256":policy,
            "program_sha256":program,"owns":fields[i].iter().map(|f|format!("{}.{f}",names[i])).collect::<Vec<_>>(),
            "participates":if i==3 {"op == 3 || op == 4".into()} else {format!("op == {i}")},"bind":bindings[i],
            "assume":"true","invariant":if i==3 {"reserve + paid <= 2 && debt <= paid"} else {"true"}}));
    }
    let manifest=json!({"schema":"zeno-fcis/checked-composition/3","checker":"zeno-fcis/composed-domain-checker/1","name":"trace_game",
        "request":[{"name":"op","domain":{"kind":"int","min":0,"max":5}},
            {"name":"keys","domain":{"kind":"int","min":0,"max":2}},
            {"name":"age","domain":{"kind":"int","min":0,"max":1}},
            {"name":"bad","domain":{"kind":"bool"}},{"name":"skew","domain":{"kind":"bool"}}],
        "components":components,"couplings":[]}).to_string();
    let requests = composition_alphabet()
        .iter()
        .map(|m| m.request.clone())
        .collect::<Vec<_>>();
    Ok(Fixture {
        name: "composition".into(),
        fault: fault.into(),
        table: native::joint(&manifest, &sources, &requests)?,
        powers: Powers::FULL,
        only_objective: None,
        disabled_objective: None,
        unfair_delay: false,
    })
}

pub(crate) fn composition_alphabet() -> Vec<Move> {
    // Full declared alphabet in canonical order. No attack trace or path hint.
    vec![
        (0, 1, 0, 0, 0),
        (0, 1, 1, 0, 0),
        (1, 1, 0, 1, 0),
        (1, 2, 0, 0, 0),
        (2, 0, 0, 0, 0),
        (2, 0, 0, 0, 1),
        (3, 0, 0, 0, 0),
        (4, 0, 0, 0, 0),
        (5, 0, 0, 0, 0),
    ]
    .into_iter()
    .map(|(op, keys, age, bad, skew)| Move {
        actor: if keys > 0 { 2 } else { 0 },
        request: vec![op, keys, age, bad, skew],
    })
    .collect()
}

pub(crate) fn core_alphabet() -> Vec<Move> {
    // Normalized command capabilities exclude the duplicate Tick opcode 5,
    // non-Withdraw age arguments, and allowed=false (bound policy context).
    (0..5)
        .flat_map(|op| {
            (0..2).flat_map(move |actor| {
                (0..if op == 3 { 2 } else { 1 }).map(move |age| Move {
                    actor: actor as u8,
                    request: vec![op, actor, age, 1],
                })
            })
        })
        .collect()
}

pub(crate) fn approved_core(state: &[i128], action: &Move) -> Outcome {
    let [balance, pending, paid, alarm]: [i128; 4] = state.try_into().unwrap_or([0; 4]);
    let [op, actor, age, allowed]: [i128; 4] =
        action.request.as_slice().try_into().unwrap_or([0; 4]);
    let reason = if allowed == 0 || (actor != 0 && (op == 0 || op == 3)) {
        Some(200)
    } else if op == 3 && age > 0 {
        Some(201)
    } else if balance + paid > 2 || (pending != 0 && balance == 0) {
        Some(203)
    } else if ((op == 0 || op == 3) && (balance == 0 || pending != 0)) || (op == 1 && pending == 0)
    {
        Some(202)
    } else {
        None
    };
    let mut post = state.to_vec();
    if reason.is_none() {
        match op {
            0 => post[1] = 1,
            1 | 3 => {
                post[0] = balance - 1;
                post[1] = 0;
                post[2] = paid + 1;
                post[3] = 0;
            }
            2 => post[3] = 1,
            _ => post[3] = 0,
        }
    }
    let _ = alarm;
    Outcome {
        kind: if reason.is_some() { "Reject" } else { "Accept" }.into(),
        reason,
        component: None,
        post,
        events: vec![],
    }
}

pub(crate) fn approved_composition(state: &[i128], action: &Move) -> Outcome {
    let op = action.request[0];
    let mut post = state.to_vec();
    let mut component = None;
    let reason = match op {
        0 => {
            component = Some("governance".into());
            if action.request[1] < 2 {
                Some(200)
            } else if action.request[2] > 0 {
                Some(201)
            } else {
                post[0] = 1;
                None
            }
        }
        1 => {
            component = Some("collateral".into());
            if action.request[3] != 0 {
                Some(203)
            } else if action.request[1] < 2 {
                Some(200)
            } else {
                post[1] = 1;
                None
            }
        }
        2 => {
            component = Some("oracle".into());
            if state[1] == 0 {
                Some(202)
            } else if action.request[4] != 0 {
                Some(203)
            } else {
                post[2] = 0;
                None
            }
        }
        3 | 4 => {
            component = Some("vault".into());
            if state[3] == 0 {
                Some(202)
            } else if op == 3 && !(state[1] == 2 && state[2] == 1) {
                Some(203)
            } else {
                if op == 3 {
                    post[3] = 0;
                    post[4] += state[3];
                    post[5] += state[3];
                } else {
                    post[3] -= 1;
                    post[4] += 1;
                }
                None
            }
        }
        _ => None,
    };
    if reason.is_none() && post[3..].iter().any(|value| *value > 2) {
        return Outcome {
            kind: "Refused".into(),
            reason: None,
            component: Some("vault".into()),
            post: state.to_vec(),
            events: vec![],
        };
    }
    if reason.is_none() {
        component = None;
    }
    Outcome {
        kind: if reason.is_some() { "Reject" } else { "Accept" }.into(),
        reason,
        component,
        post,
        events: vec![],
    }
}

impl Fixture {
    fn objective_enabled(&self, name: &str) -> bool {
        self.only_objective.as_ref().is_none_or(|p| p == name)
            && self.disabled_objective.as_ref().is_none_or(|p| p != name)
    }
    pub(crate) fn metadata(&self) -> Value {
        json!({"model":self.name,"fault":self.fault,"powers":self.powers,"alphabet":"full-callers-normalized",
        "horizon":4,"unit":"one command, rejection, tick or refusal",
        "deadline":if self.name=="withdrawal"{json!({"events":2,"request_start_at_most":4,"max_processing_postponement":1})}else{Value::Null},
        "only_objective":self.only_objective,"disabled_objective":self.disabled_objective,"unfair_delay":self.unfair_delay,
        "arithmetic":"checked native i128; fixture currency is one integer unit; fixed reference value 1",
        "valuation":"fixed coalition beneficiary; no external funding, fees or attacker debt. Composition debt is the protocol's uncompensated liability to the original vault owner, not a loan owed by the attacker; maximum loss is this liability. Native paid aggregates outflows; recipient identity is a model observation, not a native delivery claim.",
        "command_capabilities":"core: op 0..4, actors 0/1, fresh/stale Withdraw, other ages fresh, allowed=1 bound context; duplicate native Tick op5 excluded. composition: fixed argument variants, both account labels for every unprivileged command; governance/listing use privileged environment label 2",
        "attacker_observation":"full state; repair observation is alarm plus current request",
        "blind_spots":["flash liquidity","atomic batches","extra accounts or keys","external funding/markets","real scheduler faults","native delivery completeness","unbounded progress","authority objective checks value reaching a non-entitled modeled recipient, not no-value Accepts","composition request has no caller: drain/exit caller authority and recipients are model labels only"],
        "legitimacy":"independent approved reference, every prefix including attack/legitimate interleavings; candidate never selects labels",
        "claim":"checked up to K under the declared capabilities","monitor_adoption":false})
    }
}

impl Model for Fixture {
    fn starts(&self) -> Vec<Vec<i128>> {
        if self.name == "withdrawal" {
            vec![vec![1, 0, 0, 0], vec![2, 0, 0, 0]]
        } else {
            vec![vec![0, 0, 0, 2, 0, 0]]
        }
    }
    fn initial_violations(&self, start: &[i128]) -> Vec<String> {
        if self.name == "composition" && start[5] > 0 && self.objective_enabled("insolvency") {
            vec!["insolvency".into()]
        } else {
            vec![]
        }
    }
    fn moves(&self, snapshot: &Snapshot, coalition: u8) -> Vec<Move> {
        if self.name == "withdrawal" {
            if !self.unfair_delay
                && snapshot
                    .pending
                    .as_ref()
                    .is_some_and(|p| snapshot.time > p.submitted)
            {
                return vec![Move {
                    actor: 0,
                    request: vec![1, 0, 0, 1],
                }];
            }
            core_alphabet()
                .into_iter()
                .filter(|a| {
                    (self.powers.arbitrary_order || a.request[0] == 4)
                        && (a.actor == 0
                            || (coalition & self.powers.accounts & (1 << a.actor)) != 0)
                        && (self.powers.approval_ages & (1 << a.request[2])) != 0
                })
                .collect()
        } else {
            composition_alphabet()
                .into_iter()
                .flat_map(|a| {
                    if a.request[0] >= 2 {
                        (0..2)
                            .filter(|actor| coalition & self.powers.accounts & (1 << actor) != 0)
                            .map(|actor| Move {
                                actor,
                                request: a.request.clone(),
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![a]
                    }
                })
                .filter(|a| {
                    (self.powers.arbitrary_order || a.request[0] == 5)
                        && (a.request[0] != 0 || self.powers.signers != 0)
                        && self.powers.approval_ages & (1 << a.request[2]) != 0
                        && (a.request[0] != 2
                            || self.powers.oracle_choices & (1 << a.request[4]) != 0)
                })
                .collect()
        }
    }
    fn loss(&self, snapshot: &Snapshot, _: u8) -> i128 {
        if self.name == "composition" {
            snapshot.state[5]
        } else {
            (snapshot.state[2] - snapshot.reference[2]).max(0)
        }
    }
    fn step(
        &self,
        _start: &[i128],
        _coalition: u8,
        snapshot: &Snapshot,
        action: &Move,
    ) -> Result<CheckedStep, String> {
        let mut outcome = self.table.step(&snapshot.state, &action.request)?;
        let required = if self.name == "withdrawal" {
            approved_core(&snapshot.reference, action)
        } else {
            approved_composition(&snapshot.reference, action)
        };
        let mut next = snapshot.clone();
        next.time += 1;
        next.state = outcome.post.clone();
        next.reference = required.post.clone();
        let mut violations: Vec<String> = vec![];
        if required.kind == "Accept"
            && (outcome.kind != "Accept"
                || outcome.post != required.post
                || outcome.reason != required.reason)
        {
            violations.push("legitimacy".into());
        }
        let op = action.request[0];
        let mut completed = 0;
        if self.name == "withdrawal" {
            // Process is a permissionless keeper payment to the owner. Only
            // direct Withdraw selects the caller as recipient in this model.
            if outcome.post[2] > snapshot.state[2] && op == 3 && action.actor != 0 {
                violations.push("unauthorized-value".into());
            }
            if outcome.kind == "Accept" && op == 3 && action.request[2] > 0 {
                violations.push("stale-approval".into());
            }
            if outcome.post[0] + outcome.post[2] != snapshot.state[0] + snapshot.state[2] {
                violations.push("accounting-loss".into());
            }
            if required.kind == "Accept" && op == 0 {
                next.pending = Some(Pending {
                    id: next.time,
                    submitted: next.time,
                    entitlement: required.post[2] + 1,
                });
                next.monitor = 0;
            } else if let Some(p) = &snapshot.pending {
                if outcome.kind == "Accept"
                    && (op == 1 || op == 3)
                    && outcome.post[2] >= p.entitlement
                {
                    next.pending = None;
                    next.monitor = 0;
                    completed = 1;
                } else {
                    next.monitor = (snapshot.monitor + 1).min(2);
                    if next.time >= p.submitted + 2 {
                        violations.push("exit-deadline".into());
                    }
                }
            }
            if outcome.kind == "Accept" {
                outcome.events.push(
                    ["request", "exit", "alarm", "withdraw", "tick"]
                        .get(op as usize)
                        .unwrap_or(&"tick")
                        .to_string(),
                );
            }
        } else {
            if outcome.kind == "Accept" && op == 0 && action.request[1] < 2 {
                violations.push("privileged-authority".into());
            }
            if outcome.kind == "Accept" && op == 0 && action.request[2] > 0 {
                violations.push("stale-approval".into());
            }
            if outcome.post[1] == 2 {
                violations.push("bad-collateral".into());
            }
            if outcome.post[2] != 0 {
                violations.push("oracle-integrity".into());
            }
            if outcome.post[3] + outcome.post[4] != snapshot.state[3] + snapshot.state[4] {
                violations.push("accounting-loss".into());
            }
            if outcome.post[5] > 0 && outcome.post[1] != 1 {
                violations.push("insolvency".into());
            }
            if outcome.post[5] > 0 {
                violations.push("coalition-gain".into());
            }
            if outcome.kind == "Accept" {
                let event = ["escalated", "listed", "price", "drain", "exit", "tick"][op as usize];
                outcome.events.push(event.into());
                if (snapshot.stage == 0 && op == 0)
                    || (snapshot.stage == 1 && op == 1 && action.request[3] != 0)
                    || (snapshot.stage == 2 && op == 2 && action.request[4] != 0)
                    || (snapshot.stage == 3 && op == 3)
                {
                    next.stage += 1;
                }
                if next.stage == 4 {
                    violations.push("ordered-drain".into());
                }
            }
        }
        violations.retain(|v| self.objective_enabled(v));
        violations.sort();
        violations.dedup();
        Ok(CheckedStep {
            next,
            violations,
            outcome,
            completed,
        })
    }
}
