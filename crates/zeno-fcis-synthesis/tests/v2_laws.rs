//! Public V2 law/genesis boundary: real initial state and opaque reports.
use zeno_fcis_synthesis::finite::v2_laws::{
    self as laws, Atom, Failure, Field, Frame, Kind, Law, Observation, Op, Program, RootView, Scope,
};
use zeno_fcis_synthesis::finite::{V2Resource as Resource, v2_zero_limits as zero_limits};
#[test]
fn public_initial_condition_checks_actual_state() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let zero = [
        Op::Observe(Observation::Initial(7)),
        Op::Literal(Atom::I128(0)),
        Op::Eq(0, 1),
    ];
    let make = |id, kind, scope, genesis| Law {
        id,
        kind,
        scope,
        genesis,
        program: Program {
            nodes: &truth,
            root: 0,
        },
    };
    let laws = [
        make(10, Kind::StateInvariant, Scope::Committing, true),
        make(20, Kind::RejectNoAuthority, Scope::Reject, false),
        make(
            30,
            Kind::CommittedFailureEffects,
            Scope::CommittedFailure,
            false,
        ),
        make(40, Kind::DecisionConformance, Scope::Always, false),
        Law {
            id: 50,
            kind: Kind::InitialCondition,
            scope: Scope::Always,
            genesis: true,
            program: Program {
                nodes: &zero,
                root: 2,
            },
        },
    ];
    let limits = zero_limits()
        .with_limit(Resource::Step, 4)
        .with_limit(Resource::Read, 1);
    for value in [0, 1] {
        let fields = [Field {
            id: 7,
            value: Atom::I128(value),
        }];
        let (result, usage, diagnostics, reads) = laws::evaluate(
            &laws,
            &[10, 20, 30, 40, 50],
            &Frame::Genesis {
                initial: RootView::Record(&fields),
            },
            limits,
        )
        .into_parts();
        assert_eq!(
            result,
            if value == 0 {
                Ok(())
            } else {
                Err(Failure::Violated)
            }
        );
        assert_eq!(usage.used(Resource::Step), 4);
        assert_eq!(usage.used(Resource::Read), 1);
        assert_eq!(diagnostics.len(), 5);
        assert_eq!(reads.len(), 1);
    }
}
