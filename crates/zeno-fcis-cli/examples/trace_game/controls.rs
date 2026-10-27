//! Isolated failure-semantics controls using existing composition fixtures.
use super::{
    engine::*,
    native::{self, Table},
};
use serde_json::{Value, json};
use zeno_fcis_contract::checked_composition::LeafSources;

pub(crate) struct Control {
    pub(crate) table: Table,
    pub(crate) fault: String,
    pub(crate) disabled: Option<String>,
}

pub(crate) fn fixture(fault: &str, disabled: Option<&str>) -> Result<Control, String> {
    // The composition fault fixtures, compiled from the CLI's exact source snapshots.
    let sources = vec![
        LeafSources {
            project: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/counter/project.zeno.txt"
            )
            .into(),
            rules: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/counter/v2/policy.json.txt"
            )
            .into(),
        },
        LeafSources {
            project: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/tripwire/project.zeno.txt"
            )
            .into(),
            rules: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/tripwire/v2/policy.json.txt"
            )
            .into(),
        },
        LeafSources {
            project: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/budget/project.zeno.txt"
            )
            .into(),
            rules: include_str!(
                "../../source-snapshots/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/budget/v2/policy.json.txt"
            )
            .into(),
        },
    ];
    let names = ["counter", "wire", "budget"];
    let binds = [
        json!({"action":"150","authorized":"authorized"}),
        json!({"fire":"op == 1","authorized":"authorized"}),
        json!({"amount":"qty","authorized":"authorized"}),
    ];
    let fields = [vec!["value"], vec!["tripped", "touches"], vec!["remaining"]];
    let mut components = vec![];
    for i in 0..3 {
        let [project, policy, program] = native::pins(&sources[i])?;
        components.push(json!({"name":names[i],"path":names[i],"project_sha256":project,"policy_sha256":policy,"program_sha256":program,
            "owns":fields[i].iter().map(|f|format!("{}.{f}",names[i])).collect::<Vec<_>>(),"participates":"true","bind":binds[i],"assume":"true","invariant":"true"}));
    }
    let manifest=json!({"schema":"zeno-fcis/checked-composition/3","checker":"zeno-fcis/composed-domain-checker/1","name":"failure_control",
        "request":[{"name":"op","domain":{"kind":"int","min":0,"max":1}},{"name":"qty","domain":{"kind":"int","min":1,"max":2}},
            {"name":"authorized","domain":{"kind":"bool"}}],"components":components,"couplings":[]}).to_string();
    let requests = native::product(&[vec![0, 1], vec![1, 2], vec![0, 1]]);
    Ok(Control {
        table: native::joint(&manifest, &sources, &requests)?,
        fault: fault.into(),
        disabled: disabled.map(str::to_owned),
    })
}

pub(crate) fn expected(state: &[i128], r: &[i128]) -> Outcome {
    let (kind, reason, component, post) = if r[2] == 0 {
        ("Reject", Some(200), Some("counter".into()), state.to_vec())
    } else if state[0] == 2 {
        ("Reject", Some(201), Some("counter".into()), state.to_vec())
    } else if r[0] == 1 {
        (
            "CommittedFailure",
            Some(201),
            Some("wire".into()),
            vec![state[0] + 1, 1, state[2], state[3]],
        )
    } else if state[3] < r[1] {
        ("Reject", Some(201), Some("budget".into()), state.to_vec())
    } else {
        (
            "Accept",
            None,
            None,
            vec![
                state[0] + 1,
                state[1],
                (state[2] + 1).min(2),
                state[3] - r[1],
            ],
        )
    };
    Outcome {
        kind: kind.into(),
        reason,
        component,
        post,
        events: vec![],
    }
}

impl Model for Control {
    fn starts(&self) -> Vec<Vec<i128>> {
        vec![vec![0, 0, 0, 1]]
    }
    fn initial_violations(&self, _: &[i128]) -> Vec<String> {
        vec![]
    }
    fn moves(&self, _: &Snapshot, _: u8) -> Vec<Move> {
        native::product(&[vec![0, 1], vec![1, 2], vec![0, 1]])
            .into_iter()
            .map(|request| Move { actor: 0, request })
            .collect()
    }
    fn step(
        &self,
        _: &[i128],
        _: u8,
        before: &Snapshot,
        action: &Move,
    ) -> Result<CheckedStep, String> {
        let mut actual = self.table.step(&before.state, &action.request)?;
        if self.fault == "rollback-committed" && actual.kind == "CommittedFailure" {
            actual.post = before.state.clone();
        }
        if self.fault == "commit-rejected-prefix"
            && actual.kind == "Reject"
            && actual.component.as_deref() == Some("budget")
        {
            actual.post[0] += 1;
            actual.post[2] = (actual.post[2] + 1).min(2);
        }
        let mut violations = vec![];
        if self.disabled.as_deref() != Some("composition-frame")
            && actual != expected(&before.state, &action.request)
        {
            violations.push("composition-frame".into());
        }
        if self.disabled.as_deref() != Some("unauthorized-commit")
            && action.request[2] == 0
            && actual.post != before.state
        {
            violations.push("unauthorized-commit".into());
        }
        let mut next = before.clone();
        next.state = actual.post.clone();
        next.time += 1;
        Ok(CheckedStep {
            next,
            violations,
            outcome: actual,
            completed: 0,
        })
    }
}

impl Control {
    pub(crate) fn metadata(&self) -> Value {
        json!({"fault":self.fault,"disabled":self.disabled,"model":"failure-semantics","horizon":2,"powers":"three coalitions; all eight commands","only_objective":null,"unfair_delay":false,"native_mutation":false,"boundary":"isolated outcome adapter mutant; live JointComposition unchanged"})
    }
}
