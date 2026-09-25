//! Connects the induction steps of claims 600 and 601 in `project.zeno` to
//! this application.
//!
//! Claim 600 restates law 500 as an invariant over the vault before a
//! decision, and claim 601 bounds the pause. Each assumes action laws on
//! every committing decision: 501, 502, and 503 for solvency; 502 and 503
//! for the pause. `zeno-fcis prove` checks each induction step with an SMT
//! solver, whose `unsat` is attested, not independently checked: every
//! transition that satisfies the assumed laws and starts in a vault that
//! satisfies the invariant ends in one. That result says something about
//! this application only together with the checks here:
//! - the law checker's own observer, `laws::state_observations`, reads every
//!   field each invariant reads, and each invariant has a definite value on
//!   every admitted vault, the value the README's words give it;
//! - each invariant holds on the exact genesis vault, as that observer sees
//!   it;
//! - the law manifest enforces each assumed law on every committing
//!   decision;
//! - the law manifest enforces each law exactly on the decisions
//!   `project.zeno` declares for it, the scopes elaboration checked the
//!   claim's groups against.
//!
//! A rejection leaves the vault unchanged, so it needs no law. This
//! application never commits a failure, because the law checker refuses
//! every one (law 508), so its action laws are enforced on every committing
//! decision.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use withdrawal_queue::{
    authority, bindings::GeneratedProject, create, delivery::Destination, generated::*,
    laws::state_observations, profile,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawManifest, ScopeMismatch,
};
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_spec::{
    ClaimDecl, ClaimFormula, ClaimMode, EvalLimits, EvalOutcome, ProjectSpec, ProjectionRoot,
    StableId, TraceStep, claim_paths, evaluate_invariant, invariant_at,
};

const VAULT_STAYS_SOLVENT: u32 = 600;
const PAUSE_STAYS_IN_RANGE: u32 = 601;
const VAULT_SOLVENT: u32 = 500;
const FUNDS_CONSERVED: u32 = 501;
const MAX_BALANCE: i128 = 4;
const PAUSE_TICKS: i128 = 2;
/// Every vault the schema admits: five balances, three statuses and five
/// amounts per lane, three pauses, and two values each of must-serve and
/// priority.
const ADMITTED_VAULTS: usize = 5 * 3 * 5 * 3 * 5 * 3 * 2 * 2;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "withdrawal-queue-claims-{}-{}",
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

#[allow(clippy::too_many_arguments)]
fn vault(
    balance: i128,
    lane_a: LaneStatus,
    amount_a: i128,
    lane_b: LaneStatus,
    amount_b: i128,
    pause: i128,
    must_serve: bool,
    priority: Lane,
) -> Vault {
    Vault {
        balance: Money(balance),
        lane_a,
        amount_a: Money(amount_a),
        lane_b,
        amount_b: Money(amount_b),
        pause: PauseTicks(pause),
        must_serve: Flag(must_serve),
        priority,
    }
}

fn claim(project: &ProjectSpec, id: u32) -> &ClaimDecl {
    project
        .claim(StableId::new(id).unwrap())
        .unwrap_or_else(|| panic!("claim {id} is declared"))
}

/// Observes a vault under `pre.` paths through the law checker's own observer.
fn observe(state: &Vault) -> TraceStep {
    TraceStep::try_new(state_observations(ProjectionRoot::Pre, state).unwrap()).unwrap()
}

/// The definite value an invariant has on a vault where the words say it
/// `holds`, or not.
fn outcome(holds: bool) -> EvalOutcome {
    if holds {
        EvalOutcome::True
    } else {
        EvalOutcome::False
    }
}

/// Law 500 and claim 600 in the README's words: the balance is within the
/// capacity and covers both lanes' amounts, an empty lane holds nothing and
/// an occupied one at least one unit, and must-serve is set only outside a
/// pause while a lane is pending.
fn solvent(state: &Vault) -> bool {
    let holds = |lane: &LaneStatus, amount: &Money| {
        if *lane == LaneStatus::Empty {
            amount.0 == 0
        } else {
            amount.0 >= 1
        }
    };
    let pending = state.lane_a == LaneStatus::Pending || state.lane_b == LaneStatus::Pending;
    state.balance.0 <= MAX_BALANCE
        && state.balance.0 >= state.amount_a.0 + state.amount_b.0
        && holds(&state.lane_a, &state.amount_a)
        && holds(&state.lane_b, &state.amount_b)
        && (!state.must_serve.0 || (state.pause.0 == 0 && pending))
}

/// Claim 601 in words: the pause is within the controller's range.
fn pause_in_range(state: &Vault) -> bool {
    (0..=PAUSE_TICKS).contains(&state.pause.0)
}

fn admitted_vaults() -> Vec<Vault> {
    use LaneStatus::{Arrived, Empty, Pending};
    let mut vaults = Vec::with_capacity(ADMITTED_VAULTS);
    for balance in 0..=MAX_BALANCE {
        for lane_a in [Empty, Arrived, Pending] {
            for amount_a in 0..=MAX_BALANCE {
                for lane_b in [Empty, Arrived, Pending] {
                    for amount_b in 0..=MAX_BALANCE {
                        for pause in 0..=PAUSE_TICKS {
                            for must_serve in [false, true] {
                                for priority in [Lane::A, Lane::B] {
                                    vaults.push(vault(
                                        balance,
                                        lane_a.clone(),
                                        amount_a,
                                        lane_b.clone(),
                                        amount_b,
                                        pause,
                                        must_serve,
                                        priority.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    vaults
}

#[test]
fn claim_600_restates_law_500_over_the_vault_before_a_decision() {
    let project = profile::project().unwrap();
    let claim = claim(&project, VAULT_STAYS_SOLVENT);
    assert_eq!(claim.mode(), ClaimMode::Inductive);
    let ClaimFormula::Relational(invariant) = claim.formula() else {
        panic!("an inductive claim states a relational invariant");
    };
    let law = project
        .laws()
        .iter()
        .find(|law| law.id().get() == VAULT_SOLVENT)
        .expect("law 500 is declared");
    assert_eq!(
        invariant_at(invariant, ProjectionRoot::Post).as_ref(),
        Some(law.formula())
    );
}

#[test]
fn each_claim_assumes_its_laws_on_every_committing_decision() {
    let project = profile::project().unwrap();
    let manifest = profile::manifest().unwrap();
    assert_eq!(project.claims().len(), 2);
    let ids = |group: &[StableId]| {
        group
            .iter()
            .map(|law| profile::id(law.get()).unwrap())
            .collect::<Vec<_>>()
    };
    for id in [VAULT_STAYS_SOLVENT, PAUSE_STAYS_IN_RANGE] {
        let claim = claim(&project, id);
        assert_eq!(claim.mode(), ClaimMode::Inductive, "claim {id}");
        let assumptions = claim.assumptions();
        assert!(
            assumptions.accepts().is_empty() && assumptions.committed_failures().is_empty(),
            "claim {id} assumes its laws on every commit"
        );
        assert_eq!(
            manifest.check_step_assumptions(
                &ids(assumptions.every_commit()),
                &ids(assumptions.accepts()),
                &ids(assumptions.committed_failures()),
            ),
            Ok(()),
            "claim {id}"
        );
    }
    // The rejection law is not enforced on commits, and there is no law 504,
    // so neither can support a step.
    assert!(
        manifest
            .check_step_assumptions(
                &[profile::id(profile::REJECT_PUBLISHES_NOTHING).unwrap()],
                &[],
                &[]
            )
            .is_err()
    );
    assert!(
        manifest
            .check_step_assumptions(&[profile::id(504).unwrap()], &[], &[])
            .is_err()
    );
}

#[test]
fn the_law_checker_observes_every_field_each_invariant_reads() {
    use LaneStatus::{Arrived, Pending};
    let project = profile::project().unwrap();
    let sample = observe(&vault(3, Pending, 1, Arrived, 2, 1, false, Lane::B));
    for id in [VAULT_STAYS_SOLVENT, PAUSE_STAYS_IN_RANGE] {
        for path in claim_paths(claim(&project, id)) {
            assert!(
                sample
                    .observations()
                    .iter()
                    .any(|observation| observation.path() == path),
                "claim {id}: the observer omits {path:?}"
            );
        }
    }
    // On every admitted vault each invariant has a definite value within the
    // limits the law checker uses, and it is the value the words give it.
    let generated = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = EvalLimits::default();
    let (mut solvent_vaults, mut vaults) = (0, 0);
    for state in admitted_vaults() {
        assert!(
            generated
                .admit_root::<RustCryptoSha256>(&state, ValidationLimits::default())
                .is_ok(),
            "{state:?}"
        );
        let step = observe(&state);
        assert_eq!(
            evaluate_invariant(claim(&project, VAULT_STAYS_SOLVENT), &step, limits),
            Some(outcome(solvent(&state))),
            "{state:?}"
        );
        assert_eq!(
            evaluate_invariant(claim(&project, PAUSE_STAYS_IN_RANGE), &step, limits),
            Some(outcome(pause_in_range(&state))),
            "{state:?}"
        );
        solvent_vaults += usize::from(solvent(&state));
        vaults += 1;
    }
    assert_eq!(vaults, ADMITTED_VAULTS);
    // The schema admits vaults that the solvency invariant excludes, so the
    // invariant is not a consequence of admission.
    assert!(solvent_vaults > 0 && solvent_vaults < vaults);
}

#[test]
fn each_invariant_holds_on_the_exact_genesis_vault() {
    use LaneStatus::{Arrived, Empty};
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = create(
        &temp.0.join("vault.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let genesis = Vault::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    let project = profile::project().unwrap();
    let limits = EvalLimits::default();
    for id in [VAULT_STAYS_SOLVENT, PAUSE_STAYS_IN_RANGE] {
        assert_eq!(
            evaluate_invariant(claim(&project, id), &observe(&genesis), limits),
            Some(EvalOutcome::True),
            "claim {id}"
        );
    }
    // The same check refuses vaults the invariants exclude: a request the
    // balance does not cover, and a pause beyond the controller's range.
    let uncovered = vault(0, Arrived, 1, Empty, 0, 0, false, Lane::A);
    assert_eq!(
        evaluate_invariant(
            claim(&project, VAULT_STAYS_SOLVENT),
            &observe(&uncovered),
            limits
        ),
        Some(EvalOutcome::False)
    );
    let overrun = vault(0, Empty, 0, Empty, 0, PAUSE_TICKS + 1, false, Lane::A);
    assert_eq!(
        evaluate_invariant(
            claim(&project, PAUSE_STAYS_IN_RANGE),
            &observe(&overrun),
            limits
        ),
        Some(EvalOutcome::False)
    );
}

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest().unwrap();
    let law = profile::id(FUNDS_CONSERVED).unwrap();
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
    let project = profile::project().unwrap();
    // Every law with a formula declares its scope, so each one is compared.
    assert!(
        project
            .laws()
            .iter()
            .all(|law| law.applicability().is_some())
    );
    assert_eq!(
        profile::manifest().unwrap().check_declared_scopes(&project),
        Ok(())
    );
    // Law 501 is declared `on commit`, without `, genesis`: a manifest that
    // enforced it on accepts only, or applied it at genesis, is reported.
    let law = profile::id(FUNDS_CONSERVED).unwrap();
    let elsewhere = rebound(|shipped| (DecisionScope::Accept, shipped.genesis_applicability()));
    assert_eq!(
        elsewhere.check_declared_scopes(&project),
        Err(vec![ScopeMismatch::Scope {
            law,
            declared: DecisionScope::Committing,
            enforced: DecisionScope::Accept,
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
