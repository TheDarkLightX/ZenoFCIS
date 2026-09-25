//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use inventory_reservation::{generated::*, laws::StockLaws, profile, request};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::Value;

const UNITS_CONSERVED: u32 = 501;

fn stock(available: i128, reserved: i128) -> Stock {
    Stock {
        available: Units(available),
        reserved: Units(reserved),
    }
}

fn shipment(destination: &str, units: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        300,
        WarehouseDestination(destination.into()).to_value().unwrap(),
        ShipmentRequest {
            shipped_units: Quantity(units),
        }
        .to_value()
        .unwrap(),
    )
}

/// Whether the checker accepts an accepted decision.
fn holds(
    pre: Stock,
    post: Stock,
    command: StockCommand,
    authorized: bool,
    shipments: Vec<OutboxEntry>,
    effect: bool,
) -> bool {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = pre.to_value().unwrap();
    let post = post.to_value().unwrap();
    let command = command.to_value().unwrap();
    let context = StockContext {
        authorized: OperatorFlag(authorized),
    }
    .to_value()
    .unwrap();
    let outbox = OutboxPlan::try_new(shipments).unwrap();
    let commit = CommitPlan::try_new(if effect {
        vec![Effect::new(0, 999, hash, hash, Value::I128(0))]
    } else {
        vec![]
    })
    .unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let decision = LawDecisionView::Accept {
        post_state: &post,
        patch: &patch,
        commit_plan: &commit,
        outbox_plan: &outbox,
    };
    let input = LawCheckInput::try_new(hash, hash, &pre, &command, &context, decision).unwrap();
    StockLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

#[test]
fn movements_must_be_exact_and_conserve_units() {
    let reserve = |post| {
        holds(
            stock(2, 2),
            post,
            request(StockAction::Reserve, 1),
            true,
            vec![],
            false,
        )
    };
    assert!(reserve(stock(1, 3)));
    // Conserved, but the unit moved the wrong way.
    assert!(!reserve(stock(3, 1)));
    // A unit created, or a unit lost.
    assert!(!reserve(stock(1, 4)));
    assert!(!reserve(stock(1, 2)));
    let restock = |post| {
        holds(
            stock(1, 1),
            post,
            request(StockAction::Restock, 3),
            true,
            vec![],
            false,
        )
    };
    assert!(restock(stock(4, 1)));
    assert!(!restock(stock(1, 4)));
    let release = |post| {
        holds(
            stock(1, 3),
            post,
            request(StockAction::Release, 2),
            true,
            vec![],
            false,
        )
    };
    assert!(release(stock(3, 1)));
    assert!(!release(stock(1, 1)));
}

#[test]
fn shipping_must_request_exactly_the_shipped_units() {
    let ship = |post, shipments| {
        holds(
            stock(2, 3),
            post,
            request(StockAction::Ship, 2),
            true,
            shipments,
            false,
        )
    };
    assert!(ship(stock(2, 1), vec![shipment("warehouse", 2)]));
    assert!(!ship(stock(2, 1), vec![]));
    assert!(!ship(stock(2, 1), vec![shipment("warehouse", 1)]));
    assert!(!ship(stock(2, 1), vec![shipment("elsewhere", 2)]));
    // Shipping from the available units instead of the reserved ones.
    assert!(!ship(stock(0, 3), vec![shipment("warehouse", 2)]));
    // A shipment request on any other movement is refused, as is an effect.
    assert!(!holds(
        stock(1, 1),
        stock(4, 1),
        request(StockAction::Restock, 3),
        true,
        vec![shipment("warehouse", 3)],
        false
    ));
    assert!(!holds(
        stock(1, 1),
        stock(4, 1),
        request(StockAction::Restock, 3),
        true,
        vec![],
        true
    ));
}

#[test]
fn unauthorized_or_impossible_movements_must_not_commit() {
    assert!(!holds(
        stock(1, 1),
        stock(4, 1),
        request(StockAction::Restock, 3),
        false,
        vec![],
        false
    ));
    // Restocking two units onto four is over capacity, so no accepted
    // result is right, even one that stays within the bounds.
    assert!(!holds(
        stock(4, 1),
        stock(5, 1),
        request(StockAction::Restock, 2),
        true,
        vec![],
        false
    ));
}

#[test]
fn rejections_must_carry_the_rule_that_applies_and_failures_never_commit() {
    let hash = profile::digest("example/test/reject", b"independent decision view");
    let cases = [
        (stock(3, 3), StockAction::Reserve, 2, false, 200, true),
        (stock(1, 3), StockAction::Reserve, 2, true, 201, true),
        (stock(1, 3), StockAction::Reserve, 2, true, 203, false),
        (stock(3, 4), StockAction::Reserve, 2, true, 203, true),
        (stock(3, 1), StockAction::Ship, 2, true, 202, true),
        (stock(4, 2), StockAction::Release, 2, true, 203, true),
        (stock(4, 2), StockAction::Release, 2, true, 202, false),
    ];
    for (pre, action, quantity, authorized, reason_id, right) in cases {
        let pre = pre.to_value().unwrap();
        let command = request(action, quantity).to_value().unwrap();
        let context = StockContext {
            authorized: OperatorFlag(authorized),
        }
        .to_value()
        .unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = StockLaws::default().evaluate(&input, LawLimits::default());
        assert_eq!(result.is_ok(), right, "reason {reason_id}: {result:?}");
    }
    let pre = stock(3, 1).to_value().unwrap();
    let post = stock(3, 1).to_value().unwrap();
    let command = request(StockAction::Ship, 1).to_value().unwrap();
    let context = StockContext {
        authorized: OperatorFlag(true),
    }
    .to_value()
    .unwrap();
    let outbox = OutboxPlan::try_new(vec![]).unwrap();
    let commit = CommitPlan::try_new(vec![]).unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let input = LawCheckInput::try_new(
        hash,
        hash,
        &pre,
        &command,
        &context,
        LawDecisionView::CommittedFailure {
            reason_id: 202,
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
    )
    .unwrap();
    assert_eq!(
        StockLaws::default().evaluate(&input, LawLimits::default()),
        Err(LawEngineFailure::InvalidOutput)
    );
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    for (state, expected) in [(stock(0, 0), true), (stock(0, 1), false)] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = StockLaws::default()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected
        );
    }
    let value = stock(0, 0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        StockLaws::default().evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest();
    let law = profile::id(UNITS_CONSERVED);
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
    let law = profile::id(UNITS_CONSERVED);
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
