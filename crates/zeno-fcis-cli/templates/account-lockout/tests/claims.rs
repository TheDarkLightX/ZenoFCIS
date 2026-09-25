//! Connects the induction step of claim 600 in `project.zeno` to this
//! application.
//!
//! Claim 600 restates law 500 as an invariant over the account before a
//! decision: a lock never outlasts 900 seconds past the last decision, and a
//! locked account carries no failure count. It assumes laws 501 and 502 on
//! accepts and law 503 on committed failures, which is where the law
//! manifest enforces them, and the range `project.zeno` declares for each
//! integer type. `zeno-fcis prove` checks the induction step with an SMT
//! solver, whose `unsat` is attested, not independently checked: every
//! transition that satisfies the assumed laws, over values in the declared
//! ranges, and starts in an account that satisfies the invariant ends in
//! one. That result says something about this application only together
//! with the checks here:
//! - the law checker's own observer, `laws::state_observations`, reads every
//!   field the invariant reads, and the invariant has a definite value on
//!   every admitted account of a grid across the boundaries it compares, the
//!   value the README's words give it;
//! - the invariant holds on the exact genesis account, as that observer sees
//!   it;
//! - the law manifest enforces each assumed law on the decisions the claim
//!   assumes it on;
//! - the law manifest enforces each law exactly on the decisions
//!   `project.zeno` declares for it, the scopes elaboration checked the
//!   claim's groups against;
//! - the declared ranges are the bounds the generated schema enforces, every
//!   field the invariant reads has one, and they match the program's
//!   constants.
//!
//! A rejection leaves the account unchanged, so it needs no law.

use account_lockout::{
    authority, bindings::GeneratedProject, create, delivery::Destination, generated::*,
    laws::state_observations, profile, program,
};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawManifest, ScopeMismatch,
};
use zeno_fcis_schema::{TypeId, TypeKind, ValidationLimits};
use zeno_fcis_spec::{
    ClaimDecl, ClaimFormula, ClaimMode, DeclaredDomain, EvalLimits, EvalOutcome, ProjectSpec,
    ProjectionRoot, StableId, TraceStep, TypeDecl, claim_paths, declared_domain,
    evaluate_invariant, invariant_at,
};

const LOCK_STATE_STAYS_CONSISTENT: u32 = 600;
const LOCK_STATE_CONSISTENT: u32 = 500;
const LOGIN_CLEARS_FAILURES: u32 = 501;
const ADMIN_UNLOCK_CLEARS_LOCK: u32 = 502;
const FAILED_LOGIN_IS_RECORDED: u32 = 503;
/// The types whose ranges `project.zeno` declares: the failure count, a
/// time, and a lock deadline.
const ATTEMPTS: u32 = 105;
const UNIX_TIME: u32 = 106;
const LOCK_DEADLINE: u32 = 107;
/// The failure count at which the next failed login locks the account.
const COUNT_BEFORE_LOCK: i128 = 2;
/// The latest time a request may carry: 2100-01-01T00:00:00Z in Unix seconds.
const LAST_TIME: i128 = 4_102_444_800;
/// The latest deadline: one lock past the latest time.
const LAST_DEADLINE: i128 = LAST_TIME + program::LOCK_SECONDS;
/// The accounts the grid below holds: for each of seven times, the distinct
/// deadlines in range among zero, the time and its neighbours, 900 and 901
/// seconds past it, and the range's end, with every failure count.
const ADMITTED_ACCOUNTS: usize = 129;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "account-lockout-claims-{}-{}",
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

fn account(failed: i128, until: i128, seen: i128) -> Account {
    Account {
        failed_attempts: Attempts(failed),
        locked_until: LockDeadline(until),
        last_seen: UnixTime(seen),
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

/// Observes an account under `pre.` paths through the law checker's own observer.
fn observe(state: &Account) -> TraceStep {
    TraceStep::try_new(state_observations(ProjectionRoot::Pre, state).unwrap()).unwrap()
}

/// The definite value an invariant has on an account where the words say it
/// `holds`, or not.
fn outcome(holds: bool) -> EvalOutcome {
    if holds {
        EvalOutcome::True
    } else {
        EvalOutcome::False
    }
}

/// Law 500 and claim 600 in the README's words: the lock never outlasts 900
/// seconds past the last decision, and a locked account carries no failure
/// count.
fn consistent(state: &Account) -> bool {
    let locked = state.locked_until.0 > state.last_seen.0;
    state.locked_until.0 <= state.last_seen.0 + program::LOCK_SECONDS
        && (!locked || state.failed_attempts.0 == 0)
}

/// Accounts across every boundary the invariant compares, each within the
/// declared ranges: the deadline against the time, and against 900 seconds
/// past it, at times from zero to the latest one.
fn admitted_accounts() -> Vec<Account> {
    let mut accounts = Vec::with_capacity(ADMITTED_ACCOUNTS);
    for seen in [
        0,
        1,
        1000,
        LAST_TIME - program::LOCK_SECONDS - 1,
        LAST_TIME - program::LOCK_SECONDS,
        LAST_TIME - 1,
        LAST_TIME,
    ] {
        let deadlines: BTreeSet<i128> = [
            0,
            seen - 1,
            seen,
            seen + 1,
            seen + program::LOCK_SECONDS,
            seen + program::LOCK_SECONDS + 1,
            LAST_DEADLINE,
        ]
        .into_iter()
        .filter(|until| (0..=LAST_DEADLINE).contains(until))
        .collect();
        for until in deadlines {
            for failed in 0..=COUNT_BEFORE_LOCK {
                accounts.push(account(failed, until, seen));
            }
        }
    }
    accounts
}

#[test]
fn claim_600_restates_law_500_over_the_account_before_a_decision() {
    let project = profile::project();
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    assert_eq!(claim.mode(), ClaimMode::Inductive);
    let ClaimFormula::Relational(invariant) = claim.formula() else {
        panic!("an inductive claim states a relational invariant");
    };
    let law = project
        .laws()
        .iter()
        .find(|law| law.id().get() == LOCK_STATE_CONSISTENT)
        .expect("law 500 is declared");
    assert_eq!(
        invariant_at(invariant, ProjectionRoot::Post).as_ref(),
        Some(law.formula())
    );
}

#[test]
fn the_claim_assumes_its_laws_on_the_decisions_the_manifest_checks_them_on() {
    let project = profile::project();
    let manifest = profile::manifest();
    assert_eq!(project.claims().len(), 1);
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    let (every_commit, accepts, failures) = groups(claim);
    assert_eq!(
        (&every_commit, &accepts, &failures),
        (
            &vec![],
            &vec![LOGIN_CLEARS_FAILURES, ADMIN_UNLOCK_CLEARS_LOCK],
            &vec![FAILED_LOGIN_IS_RECORDED]
        )
    );
    let ids = |group: &[u32]| {
        group
            .iter()
            .map(|law| profile::id(*law))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        manifest.check_step_assumptions(&ids(&every_commit), &ids(&accepts), &ids(&failures)),
        Ok(())
    );
    // A law is assumed only on the decisions its scope covers: the login and
    // unlock laws are checked on accepts, not on every commit or on committed
    // failures; the failed-login law on committed failures, not on accepts;
    // the rejection law on no commit; and there is no law 504.
    let id = |law| profile::id(law);
    for (every_commit, accepts, failures) in [
        (vec![id(LOGIN_CLEARS_FAILURES)], vec![], vec![]),
        (vec![], vec![], vec![id(ADMIN_UNLOCK_CLEARS_LOCK)]),
        (vec![], vec![id(FAILED_LOGIN_IS_RECORDED)], vec![]),
        (vec![id(profile::REJECT_PUBLISHES_NOTHING)], vec![], vec![]),
        (vec![id(504)], vec![], vec![]),
    ] {
        assert!(
            manifest
                .check_step_assumptions(&every_commit, &accepts, &failures)
                .is_err(),
            "{every_commit:?} {accepts:?} {failures:?}"
        );
    }
}

#[test]
fn the_law_checker_observes_every_field_the_invariant_reads() {
    let project = profile::project();
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    let sample = observe(&account(1, 1000, 500));
    for path in claim_paths(claim) {
        assert!(
            sample
                .observations()
                .iter()
                .any(|observation| observation.path() == path),
            "the observer omits {path:?}"
        );
    }
    // On every account of the grid the invariant has a definite value within
    // the limits the law checker uses, and it is the value the words give it.
    let generated = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = EvalLimits::default();
    let (mut consistent_accounts, mut accounts) = (0, 0);
    for state in admitted_accounts() {
        assert!(
            generated
                .admit_root::<RustCryptoSha256>(&state, ValidationLimits::default())
                .is_ok(),
            "{state:?}"
        );
        assert_eq!(
            evaluate_invariant(claim, &observe(&state), limits),
            Some(outcome(consistent(&state))),
            "{state:?}"
        );
        consistent_accounts += usize::from(consistent(&state));
        accounts += 1;
    }
    assert_eq!(accounts, ADMITTED_ACCOUNTS);
    // The schema admits accounts that the invariant excludes, so the
    // invariant is not a consequence of admission.
    assert!(consistent_accounts > 0 && consistent_accounts < accounts);
}

#[test]
fn the_invariant_holds_on_the_exact_genesis_account() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = create(
        &temp.0.join("account.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let genesis = Account::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    let project = profile::project();
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    let limits = EvalLimits::default();
    assert_eq!(
        evaluate_invariant(claim, &observe(&genesis), limits),
        Some(EvalOutcome::True)
    );
    // The same check refuses accounts the invariant excludes: a lock longer
    // than 900 seconds, and a locked account that still counts failures.
    let overlong = account(0, program::LOCK_SECONDS + 1, 0);
    let counting = account(1, program::LOCK_SECONDS, 0);
    for state in [overlong, counting] {
        assert_eq!(
            evaluate_invariant(claim, &observe(&state), limits),
            Some(EvalOutcome::False),
            "{state:?}"
        );
    }
}

#[test]
fn the_declared_ranges_are_the_schemas_bounds() {
    let project = profile::project();
    let schema = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let declared = |id: u32| {
        let range = project
            .types()
            .iter()
            .find(|declared| declared.id().get() == id)
            .and_then(TypeDecl::range)
            .unwrap_or_else(|| panic!("type {id} declares a range"));
        (range.min(), range.max())
    };
    let bound = |id: u32| {
        let kind = schema
            .catalog()
            .schema()
            .type_by_id(TypeId::new(id))
            .unwrap_or_else(|| panic!("type {id} is in the schema"))
            .kind()
            .clone();
        let TypeKind::I128 { min, max } = kind else {
            panic!("type {id} is a bounded integer, not {kind:?}");
        };
        (min, max)
    };
    // The generated schema enforces exactly the declared ranges, which are
    // the README's numbers.
    for (id, range) in [
        (ATTEMPTS, (0, COUNT_BEFORE_LOCK)),
        (UNIX_TIME, (0, LAST_TIME)),
        (LOCK_DEADLINE, (0, LAST_DEADLINE)),
    ] {
        assert_eq!(declared(id), range, "type {id}");
        assert_eq!(bound(id), range, "type {id}");
    }
    // The ranges agree with the program: the count locks at the range's end
    // plus one, and a deadline is at most one lock past the latest time.
    assert_eq!(declared(ATTEMPTS).1 + 1, program::LOCK_AFTER_FAILURES);
    assert_eq!(
        declared(LOCK_DEADLINE).1 - declared(UNIX_TIME).1,
        program::LOCK_SECONDS
    );
    // Every field the invariant reads has a declared range, so the induction
    // step assumes the schema's bounds for each of them.
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    for path in claim_paths(claim) {
        assert!(
            matches!(
                declared_domain(&project, path),
                Some(DeclaredDomain::Range(_))
            ),
            "{path:?} has no declared range"
        );
    }
}

#[test]
fn outside_the_declared_ranges_the_invariant_can_have_no_value() {
    // At a time near the top of the 128-bit range `last_seen + 900`
    // overflows, so the invariant has no value there. The schema refuses such
    // a time, and the induction step assumes the declared range instead.
    let project = profile::project();
    let claim = claim(&project, LOCK_STATE_STAYS_CONSISTENT);
    let generated = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let late = account(0, 0, i128::MAX);
    assert!(
        generated
            .admit_root::<RustCryptoSha256>(&late, ValidationLimits::default())
            .is_err()
    );
    assert!(matches!(
        evaluate_invariant(claim, &observe(&late), EvalLimits::default()),
        Some(EvalOutcome::Indeterminate(_))
    ));
}

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest();
    let law = profile::id(LOGIN_CLEARS_FAILURES);
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
    let law = profile::id(LOGIN_CLEARS_FAILURES);
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
