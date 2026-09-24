//! Connects the proof of claim 600 in `project.zeno` to this application.
//!
//! Claim 600 states the strikes bound as an invariant, and assumes laws 501
//! and 502 on accepts and law 503 on committed failures. `zeno-fcis prove`
//! checks its induction step with an SMT solver, whose `unsat` is attested,
//! not independently checked: every transition that satisfies those laws and
//! starts within the bound ends within it. That result says something about
//! this application only together with the checks here:
//! - the law checker's own observer, `trace_step`, reads every field the
//!   invariant reads, and the invariant has a definite value on every
//!   admitted standing;
//! - the invariant holds on the exact genesis standing, as that observer sees
//!   it;
//! - the law manifest enforces each assumed law on the decisions the claim
//!   assumes it on.
//!
//! A rejection leaves the standing unchanged, so it needs no law.

use compliance_gateway::{
    authority, context, create, delivery::Destination, generated::*, laws::trace_step, profile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_spec::{
    ClaimMode, EvalLimits, EvalOutcome, StableId, TraceStep, claim_paths, evaluate_invariant,
};

const STRIKES_STAY_IN_BOUNDS: u32 = 600;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "compliance-gateway-claims-{}-{}",
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

/// Observes a standing through the law checker's own observer. The invariant
/// reads only the state before the decision, so the other inputs are fixed.
fn observe(standing: &Standing) -> TraceStep {
    let command = GatewayCommand {
        action: GatewayAction::Screen,
        region: Region::Allowed,
        amount_band: AmountBand(0),
        counterparty_risk: CounterpartyRisk::Low,
    };
    trace_step(standing, standing, &command, &context(0, false)).unwrap()
}

fn strikes(value: i128) -> Standing {
    Standing {
        strikes: Strikes(value),
    }
}

#[test]
fn claim_600_assumes_each_law_on_the_decisions_the_manifest_checks_it_on() {
    let project = profile::project();
    let claim = project
        .claim(StableId::new(STRIKES_STAY_IN_BOUNDS).unwrap())
        .expect("claim 600 is declared");
    assert_eq!(claim.mode(), ClaimMode::Inductive);
    assert_eq!(project.claims().len(), 1);
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
    // Law 503 is checked only on committed failures, so assuming it on every
    // commit is refused.
    assert!(
        profile::manifest()
            .check_step_assumptions(&[profile::id(503)], &[], &[])
            .is_err()
    );
}

#[test]
fn the_law_checker_observes_every_field_the_invariant_reads() {
    let project = profile::project();
    let claim = project
        .claim(StableId::new(STRIKES_STAY_IN_BOUNDS).unwrap())
        .expect("claim 600 is declared");
    let sample = observe(&strikes(1));
    for path in claim_paths(claim) {
        assert!(
            sample
                .observations()
                .iter()
                .any(|observation| observation.path() == path),
            "the observer omits {path:?}"
        );
    }
    // Every admitted standing gives the invariant a definite value within the
    // limits the law checker uses; every one of them is within the bound.
    for value in 0..=3 {
        assert_eq!(
            evaluate_invariant(claim, &observe(&strikes(value)), EvalLimits::default()),
            Some(EvalOutcome::True),
            "strikes {value}"
        );
    }
    assert_eq!(
        evaluate_invariant(claim, &observe(&strikes(4)), EvalLimits::default()),
        Some(EvalOutcome::False)
    );
}

#[test]
fn the_invariant_holds_on_the_exact_genesis_standing() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = create(
        &temp.0.join("gateway.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let genesis = Standing::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    let project = profile::project();
    let claim = project
        .claim(StableId::new(STRIKES_STAY_IN_BOUNDS).unwrap())
        .expect("claim 600 is declared");
    assert_eq!(
        evaluate_invariant(claim, &observe(&genesis), EvalLimits::default()),
        Some(EvalOutcome::True)
    );
}
