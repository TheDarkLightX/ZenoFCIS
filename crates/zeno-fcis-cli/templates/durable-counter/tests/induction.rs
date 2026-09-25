//! Connects the proof of claim 600 in `project.zeno` to this application.
//!
//! Claim 600 states an invariant, both counters are nonnegative, and assumes
//! law 501 on accepts and law 502 on committed failures. `zeno-fcis prove`
//! checks its induction step with an SMT solver, whose `unsat` is attested,
//! not independently checked: every transition that satisfies those laws and
//! starts in a state satisfying the invariant ends in one. That result says
//! something about this application only together with the checks here:
//! - the law checker's own observer reads every field the invariant reads,
//!   and the invariant has a definite value on every admitted state;
//! - the invariant holds on the exact genesis state, as that observer sees it;
//! - the law manifest enforces each assumed law on the decisions the claim
//!   assumes it on;
//! - the law manifest enforces each law exactly on the decisions
//!   `project.zeno` declares for it, the scopes elaboration checked the
//!   claim's groups against.
//!
//! A rejection leaves the state unchanged, so it needs no law.

use durable_counter::{
    authority, create, delivery::Destination, generated::*, laws::state_observations, profile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawManifest, ScopeMismatch,
};
use zeno_fcis_spec::{
    ClaimMode, EvalLimits, EvalOutcome, ProjectionRoot, StableId, TraceStep, claim_paths,
    evaluate_invariant,
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
const ACCEPTED_INCREMENT: u32 = 501;

/// Observes a state under `pre.` paths through the law checker's own observer.
fn observe(state: &CounterState) -> TraceStep {
    TraceStep::try_new(state_observations(ProjectionRoot::Pre, state).to_vec()).unwrap()
}

#[test]
fn the_law_checker_observes_every_field_the_invariant_reads() {
    let project = profile::project();
    let claim = project
        .claim(StableId::new(NEVER_NEGATIVE).unwrap())
        .expect("claim 600 is declared");
    let sample = observe(&CounterState {
        count: CounterValue(1),
        failures: CounterValue(2),
    });
    for path in claim_paths(claim) {
        assert!(
            sample
                .observations()
                .iter()
                .any(|observation| observation.path() == path),
            "the observer omits {path:?}"
        );
    }
    // On every admitted state the invariant has a definite value within the
    // limits the law checker uses: no missing projection, no exhausted limit.
    for count in 0..=3 {
        for failures in 0..=3 {
            let state = CounterState {
                count: CounterValue(count),
                failures: CounterValue(failures),
            };
            assert_eq!(
                evaluate_invariant(claim, &observe(&state), EvalLimits::default()),
                Some(EvalOutcome::True),
                "count {count}, failures {failures}"
            );
        }
    }
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

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest();
    let law = profile::id(ACCEPTED_INCREMENT);
    let definitions = manifest
        .definitions()
        .iter()
        .map(|definition| {
            if definition.id() != law {
                return definition.clone();
            }
            let (scope, genesis) = rebind(definition);
            LawDefinition::try_new(
                definition.id(),
                definition.name().clone(),
                definition.kind(),
                scope,
                genesis,
                definition.claim_hash(),
                definition.checker_profile_hash(),
                definition.evidence_requirement(),
            )
            .unwrap()
        })
        .collect();
    LawManifest::try_new(manifest.families().to_vec(), definitions).unwrap()
}

#[test]
fn the_manifest_enforces_the_scopes_the_project_declares() {
    let project = profile::project();
    // Every law with a formula declares its scope, so each one is compared.
    assert!(
        project
            .laws()
            .iter()
            .all(|law| law.applicability().is_some())
    );
    assert_eq!(profile::manifest().check_declared_scopes(&project), Ok(()));
    // Law 501 is declared `on accept`, without `, genesis`: a manifest that
    // enforced it on every commit, or applied it at genesis, is reported.
    let law = profile::id(ACCEPTED_INCREMENT);
    let elsewhere = rebound(|shipped| (DecisionScope::Committing, shipped.genesis_applicability()));
    assert_eq!(
        elsewhere.check_declared_scopes(&project),
        Err(vec![ScopeMismatch::Scope {
            law,
            declared: DecisionScope::Accept,
            enforced: DecisionScope::Committing,
        }])
    );
    let at_genesis = rebound(|shipped| (shipped.scope(), GenesisApplicability::Required));
    assert_eq!(
        at_genesis.check_declared_scopes(&project),
        Err(vec![ScopeMismatch::Genesis {
            law,
            declared: false
        }])
    );
}
