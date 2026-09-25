//! Connects the induction steps of claims 600 and 601 in `project.zeno` to
//! this application.
//!
//! Claim 600 restates law 500 as an invariant over the treasury before a
//! decision: the quote balance keeps the reserve, the base balance is never
//! negative, a day's committed value stays between 0 and the budget, and the
//! swap amounts are held only while a swap is pending. Claim 601 states the
//! budget's part on its own. Each assumes laws 501 and 502 on every
//! committing decision, action laws on accepts (503 to 507 for claim 600,
//! 505 and 507 for claim 601), and law 508 on committed failures.
//! `zeno-fcis prove` checks each induction step with an SMT solver, whose
//! `unsat` is attested, not independently checked: every transition that
//! satisfies the assumed laws and starts in a treasury that satisfies the
//! invariant ends in one. That result says something about this application
//! only together with the checks here:
//! - the law checker's own observer, `laws::state_observations`, reads every
//!   field each invariant reads, and each invariant has a definite value on
//!   every admitted treasury, the value the README's words give it;
//! - each invariant holds on the exact genesis treasury, as that observer
//!   sees it;
//! - the law manifest enforces each assumed law on the decisions the claim
//!   assumes it on;
//! - the law manifest enforces each law exactly on the decisions
//!   `project.zeno` declares for it, the scopes elaboration checked the
//!   claim's groups against.
//!
//! A rejection leaves the treasury unchanged, so it needs no law.

use agent_treasury_guard::{
    authority, bindings::GeneratedProject, create, delivery::Destination, generated::*,
    laws::state_observations, profile, treasury,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawManifest, ScopeMismatch,
};
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_spec::{
    ClaimDecl, ClaimFormula, ClaimMode, EvalLimits, EvalOutcome, ProjectSpec, ProjectionPath,
    ProjectionRoot, StableId, TraceStep, claim_paths, evaluate_invariant, invariant_at,
};

use PendingSwap::{NoSwap, PendingBuy, PendingSell};

const TREASURY_STAYS_WITHIN_LIMITS: u32 = 600;
const BUDGET_NEVER_EXCEEDED: u32 = 601;
const TREASURY_WITHIN_LIMITS: u32 = 500;
const COMMITS_ADVANCE_THE_CLOCK: u32 = 501;
const PROPOSAL_AND_SETTLEMENT_AUTHORITY: u32 = 503;
const FAILURE_REFUNDS_THE_SWAP: u32 = 508;
const BALANCE_BOUND: i128 = 20;
const BUDGET: i128 = 4;
const RESERVE: i128 = 2;
const LARGEST_AMOUNT: i128 = 3;
/// The field the invariants do not read: `last_seen`, 133.
const LAST_SEEN: u32 = 133;
/// Every treasury the schema admits at one tick: two balances of 21 values,
/// five budget positions, three swap states, and four values each of the
/// held amount and minimum. Neither invariant reads the tick, which the
/// observer test checks, so one tick stands for all twelve.
const ADMITTED_TREASURIES: usize = 21 * 21 * 5 * 3 * 4 * 4;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agent-treasury-guard-claims-{}-{}",
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

fn claim(project: &ProjectSpec, id: u32) -> &ClaimDecl {
    project
        .claim(StableId::new(id).unwrap())
        .unwrap_or_else(|| panic!("claim {id} is declared"))
}

/// The three groups of laws a claim assumes, by ID.
fn groups(claim: &ClaimDecl) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let ids = |group: &[StableId]| group.iter().map(|law| law.get()).collect::<Vec<_>>();
    let assumptions = claim.assumptions();
    (
        ids(assumptions.every_commit()),
        ids(assumptions.accepts()),
        ids(assumptions.committed_failures()),
    )
}

/// Observes a treasury under `pre.` paths through the law checker's own observer.
fn observe(state: &Treasury) -> TraceStep {
    TraceStep::try_new(state_observations(ProjectionRoot::Pre, state).unwrap()).unwrap()
}

/// The definite value an invariant has on a treasury where the words say it
/// `holds`, or not.
fn outcome(holds: bool) -> EvalOutcome {
    if holds {
        EvalOutcome::True
    } else {
        EvalOutcome::False
    }
}

/// Claim 601 in the README's words: the day's committed value stays between 0
/// and the budget.
fn budget_kept(state: &Treasury) -> bool {
    (0..=BUDGET).contains(&state.spent_today.0)
}

/// Law 500 and claim 600 in the README's words: the quote balance keeps the
/// reserve, the base balance is never negative, the budget is kept, and the
/// swap amounts are held only while a swap is pending: at least 1 unit, with
/// a minimum that is never negative.
fn within_limits(state: &Treasury) -> bool {
    let held = if state.pending == NoSwap {
        state.pending_amount.0 == 0 && state.pending_min_out.0 == 0
    } else {
        state.pending_amount.0 >= 1 && state.pending_min_out.0 >= 0
    };
    state.quote.0 >= RESERVE && state.base.0 >= 0 && budget_kept(state) && held
}

fn admitted_treasuries() -> Vec<Treasury> {
    let mut treasuries = Vec::with_capacity(ADMITTED_TREASURIES);
    for quote in 0..=BALANCE_BOUND {
        for base in 0..=BALANCE_BOUND {
            for spent_today in 0..=BUDGET {
                for pending in [NoSwap, PendingBuy, PendingSell] {
                    for pending_amount in 0..=LARGEST_AMOUNT {
                        for pending_min_out in 0..=LARGEST_AMOUNT {
                            treasuries.push(treasury(
                                quote,
                                base,
                                spent_today,
                                0,
                                pending.clone(),
                                pending_amount,
                                pending_min_out,
                            ));
                        }
                    }
                }
            }
        }
    }
    treasuries
}

#[test]
fn claim_600_restates_law_500_over_the_treasury_before_a_decision() {
    let project = profile::project();
    let claim = claim(&project, TREASURY_STAYS_WITHIN_LIMITS);
    assert_eq!(claim.mode(), ClaimMode::Inductive);
    let ClaimFormula::Relational(invariant) = claim.formula() else {
        panic!("an inductive claim states a relational invariant");
    };
    let law = project
        .laws()
        .iter()
        .find(|law| law.id().get() == TREASURY_WITHIN_LIMITS)
        .expect("law 500 is declared");
    assert_eq!(
        invariant_at(invariant, ProjectionRoot::Post).as_ref(),
        Some(law.formula())
    );
}

#[test]
fn each_claim_assumes_its_laws_on_the_decisions_the_manifest_checks_them_on() {
    let project = profile::project();
    let manifest = profile::manifest();
    assert_eq!(project.claims().len(), 2);
    let expected = [
        (
            TREASURY_STAYS_WITHIN_LIMITS,
            (vec![501, 502], vec![503, 504, 505, 506, 507], vec![508]),
        ),
        (
            BUDGET_NEVER_EXCEEDED,
            (vec![501, 502], vec![505, 507], vec![508]),
        ),
    ];
    for (id, assumed) in expected {
        let claim = claim(&project, id);
        assert_eq!(claim.mode(), ClaimMode::Inductive, "claim {id}");
        let (every_commit, accepts, failures) = groups(claim);
        assert_eq!(
            (&every_commit, &accepts, &failures),
            (&assumed.0, &assumed.1, &assumed.2)
        );
        let ids = |group: &[u32]| {
            group
                .iter()
                .map(|law| profile::id(*law))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            manifest.check_step_assumptions(&ids(&every_commit), &ids(&accepts), &ids(&failures)),
            Ok(()),
            "claim {id}"
        );
    }
    // A law is assumed only on the decisions its scope covers: the proposal
    // law is checked on accepts, not on every commit; the refund law on
    // committed failures, not on accepts; and the rejection law on no commit.
    let id = |law| profile::id(law);
    assert!(
        manifest
            .check_step_assumptions(&[id(PROPOSAL_AND_SETTLEMENT_AUTHORITY)], &[], &[])
            .is_err()
    );
    assert!(
        manifest
            .check_step_assumptions(&[], &[id(FAILURE_REFUNDS_THE_SWAP)], &[])
            .is_err()
    );
    assert!(
        manifest
            .check_step_assumptions(&[id(profile::REJECT_PUBLISHES_NOTHING)], &[], &[])
            .is_err()
    );
}

#[test]
fn the_law_checker_observes_every_field_each_invariant_reads() {
    let project = profile::project();
    let sample = observe(&treasury(4, 1, 2, 1, PendingBuy, 2, 2));
    let last_seen = ProjectionPath::try_new(
        ProjectionRoot::Pre,
        vec![
            StableId::new(100).unwrap(),
            StableId::new(LAST_SEEN).unwrap(),
        ],
    )
    .unwrap();
    for id in [TREASURY_STAYS_WITHIN_LIMITS, BUDGET_NEVER_EXCEEDED] {
        for path in claim_paths(claim(&project, id)) {
            assert!(
                sample
                    .observations()
                    .iter()
                    .any(|observation| observation.path() == path),
                "claim {id}: the observer omits {path:?}"
            );
            assert_ne!(path, &last_seen, "claim {id} reads the tick");
        }
    }
    // On every admitted treasury each invariant has a definite value within
    // the limits the law checker uses, and it is the value the words give it.
    let generated = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = EvalLimits::default();
    let (mut within, mut treasuries) = (0, 0);
    for state in admitted_treasuries() {
        assert!(
            generated
                .admit_root::<RustCryptoSha256>(&state, ValidationLimits::default())
                .is_ok(),
            "{state:?}"
        );
        let step = observe(&state);
        assert_eq!(
            evaluate_invariant(claim(&project, TREASURY_STAYS_WITHIN_LIMITS), &step, limits),
            Some(outcome(within_limits(&state))),
            "{state:?}"
        );
        assert_eq!(
            evaluate_invariant(claim(&project, BUDGET_NEVER_EXCEEDED), &step, limits),
            Some(outcome(budget_kept(&state))),
            "{state:?}"
        );
        within += usize::from(within_limits(&state));
        treasuries += 1;
    }
    assert_eq!(treasuries, ADMITTED_TREASURIES);
    // The schema admits treasuries that claim 600 excludes, so the invariant
    // is not a consequence of admission. Claim 601's is: the schema bounds
    // `spent_today` at the budget, and the claim shows the laws do too.
    assert!(within > 0 && within < treasuries);
}

#[test]
fn each_invariant_holds_on_the_exact_genesis_treasury() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = create(
        &temp.0.join("treasury.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let genesis = Treasury::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    let project = profile::project();
    let limits = EvalLimits::default();
    for id in [TREASURY_STAYS_WITHIN_LIMITS, BUDGET_NEVER_EXCEEDED] {
        assert_eq!(
            evaluate_invariant(claim(&project, id), &observe(&genesis), limits),
            Some(EvalOutcome::True),
            "claim {id}"
        );
    }
    // The same check refuses treasuries the invariants exclude: a quote
    // balance below the reserve, a day over its budget, and a pending swap
    // holding a negative minimum, the start of the last counterexample the
    // solver found before the invariant said so.
    let evaluate =
        |id, state: &Treasury| evaluate_invariant(claim(&project, id), &observe(state), limits);
    let below_reserve = treasury(1, 1, 0, 0, NoSwap, 0, 0);
    assert_eq!(
        evaluate(TREASURY_STAYS_WITHIN_LIMITS, &below_reserve),
        Some(EvalOutcome::False)
    );
    assert_eq!(
        evaluate(BUDGET_NEVER_EXCEEDED, &below_reserve),
        Some(EvalOutcome::True)
    );
    let over_budget = treasury(6, 1, BUDGET + 1, 0, NoSwap, 0, 0);
    for id in [TREASURY_STAYS_WITHIN_LIMITS, BUDGET_NEVER_EXCEEDED] {
        assert_eq!(
            evaluate(id, &over_budget),
            Some(EvalOutcome::False),
            "claim {id}"
        );
    }
    let negative_minimum = treasury(2, 0, 4, 0, PendingBuy, 1, -1);
    assert_eq!(
        evaluate(TREASURY_STAYS_WITHIN_LIMITS, &negative_minimum),
        Some(EvalOutcome::False)
    );
}

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest();
    let law = profile::id(COMMITS_ADVANCE_THE_CLOCK);
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
    // Law 501 is declared `on commit`, without `, genesis`: a manifest that
    // enforced it on accepts only, or applied it at genesis, is reported.
    let law = profile::id(COMMITS_ADVANCE_THE_CLOCK);
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
