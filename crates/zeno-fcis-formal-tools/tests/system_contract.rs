//! Contract regressions from the independent review of commit 521b768.
//!
//! Each test failed on that commit. They are kept in the reviewer's form,
//! adapted only to the model shape that now carries proposed outputs.

use zeno_fcis_formal_tools::{SystemAnswer, SystemObligationKind, system_verdict};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
use zeno_fcis_synthesis::system::{Property, SystemLimits, check_system_property};

const D: Domain = Domain::Int { min: 0, max: 1 };

fn transition(nodes: Vec<Op>, root: u16) -> Program {
    Program::try_new(vec![D], vec![D], nodes, vec![root])
        .unwrap_or_else(|error| panic!("transition: {error:?}"))
}

fn property(nodes: Vec<Op>, root: u16) -> Property {
    let relation = Program::try_new(vec![D, D], vec![Domain::Bool], nodes, vec![root])
        .unwrap_or_else(|error| panic!("relation: {error:?}"));
    Property::try_new(vec![D], vec![D], relation)
        .unwrap_or_else(|error| panic!("property: {error:?}"))
}

fn sat(input: Vec<i64>, output: Vec<i64>) -> SystemAnswer {
    SystemAnswer::Sat { input, output }
}

#[test]
fn totality_counterexample_must_be_an_admitted_input() {
    let transition = transition(vec![Op::Input(0)], 0);
    let property = property(vec![Op::Bool(true)], 0);
    let exhaustive =
        check_system_property(&transition, property.contract(), SystemLimits::default())
            .unwrap_or_else(|error| panic!("exhaustive: {error:?}"));
    assert_eq!(exhaustive.code(), "domain-implied");
    let verdict = system_verdict(&transition, &property, |_, _| Ok(sat(vec![2], vec![])));
    assert!(
        verdict.is_err(),
        "out-of-domain model must be refused, got {verdict:?}"
    );
}

#[test]
fn totality_precedes_an_earlier_property_violation() {
    let transition = transition(vec![Op::Input(0), Op::Add(0, 0)], 1);
    let property = property(vec![Op::Input(1), Op::Int(1), Op::Eq(0, 1)], 2);
    assert_eq!(transition.evaluate(&[0]), Ok(vec![0]));
    assert!(transition.evaluate(&[1]).is_err());
    let exhaustive =
        check_system_property(&transition, property.contract(), SystemLimits::default())
            .unwrap_or_else(|error| panic!("exhaustive: {error:?}"));
    let replayed = system_verdict(&transition, &property, |kind, _| {
        assert_eq!(kind, SystemObligationKind::Totality);
        Ok(sat(vec![1], vec![]))
    })
    .unwrap_or_else(|error| panic!("replayed: {error:?}"));
    assert_eq!(
        exhaustive.code(),
        replayed.code(),
        "exhaustive={exhaustive:?}, replayed={replayed:?}"
    );
}

#[test]
fn promised_sat_replay_includes_domain_only_control() {
    let transition = transition(vec![Op::Input(0)], 0);
    let property = property(vec![Op::Bool(true)], 0);
    let verdict = system_verdict(&transition, &property, |kind, _| {
        Ok(match kind {
            SystemObligationKind::DomainOnly => sat(vec![0], vec![0]),
            _ => SystemAnswer::Unsat,
        })
    });
    assert!(
        verdict.is_err(),
        "a constant-true property has no replayable violating pair: {verdict:?}"
    );
}
