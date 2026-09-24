//! Connects the proof of claim 600 in `project.zeno` to this application.
//!
//! Claim 600 states an invariant, both counters are nonnegative, and assumes
//! law 501 on accepts and law 502 on committed failures. `zeno-fcis prove`
//! checks its induction step: every transition that satisfies those laws and
//! starts in a state satisfying the invariant ends in one. That result says
//! something about this application only together with the two checks here:
//! the invariant holds on the exact genesis state, and the law manifest
//! enforces each assumed law on the decisions the claim assumes it on.
//! A rejection leaves the state unchanged, so it needs no law.

use durable_counter::{authority, create, delivery::Destination, generated::*, profile};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_spec::{
    ClaimMode, EvalLimits, EvalOutcome, Observation, ProjectionPath, ProjectionRoot, StableId,
    TraceStep, evaluate_invariant,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "durable-counter-induction-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const NEVER_NEGATIVE: u32 = 600;

/// Observes a state under `pre.` paths, the same paths the law checker uses.
fn observe(state: &CounterState) -> TraceStep {
    let path = |field: u32| {
        ProjectionPath::try_new(
            ProjectionRoot::Pre,
            vec![StableId::new(100).unwrap(), StableId::new(field).unwrap()],
        )
        .unwrap()
    };
    TraceStep::try_new(vec![
        Observation::new(path(110), state.count.0),
        Observation::new(path(111), state.failures.0),
    ])
    .unwrap()
}

#[test]
fn the_invariant_holds_on_the_exact_genesis_state() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = create(
        &temp.0.join("counter.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let genesis = CounterState::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    let project = profile::project();
    let claim = project
        .claim(StableId::new(NEVER_NEGATIVE).unwrap())
        .expect("claim 600 is declared");
    assert_eq!(claim.mode(), ClaimMode::Inductive);
    assert_eq!(
        evaluate_invariant(claim, &observe(&genesis), EvalLimits::default()),
        Some(EvalOutcome::True)
    );
    // The same check refuses a state the invariant excludes.
    let negative = CounterState {
        count: CounterValue(-1),
        failures: CounterValue(0),
    };
    assert_eq!(
        evaluate_invariant(claim, &observe(&negative), EvalLimits::default()),
        Some(EvalOutcome::False)
    );
}

#[test]
fn the_manifest_enforces_every_law_the_proof_assumes() {
    let project = profile::project();
    let claim = project
        .claim(StableId::new(NEVER_NEGATIVE).unwrap())
        .expect("claim 600 is declared");
    let ids = |group: &[StableId]| {
        group
            .iter()
            .map(|law| profile::id(law.get()))
            .collect::<Vec<_>>()
    };
    let assumptions = claim.assumptions();
    assert_eq!(
        profile::manifest().check_step_assumptions(
            &ids(assumptions.every_commit()),
            &ids(assumptions.accepts()),
            &ids(assumptions.committed_failures()),
        ),
        Ok(())
    );
    // Law 501 is checked only on accepts, so assuming it on every commit is refused.
    assert!(
        profile::manifest()
            .check_step_assumptions(&[profile::id(501)], &[], &[])
            .is_err()
    );
}
