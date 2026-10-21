//! Random-bit declarations, canonical identities and exact rational arithmetic.
#![allow(clippy::unwrap_used)]

use zeno_fcis_spec::{Probability, ProjectLimits, SourceLimits, elaborate_project, parse_project};

const PROJECT: &str =
    include_str!("../../../examples/distribution-laws/reporter-choice/project.zeno");

fn spec(source: &str) -> zeno_fcis_spec::ProjectSpec {
    elaborate_project(
        parse_project(source, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap()
}

#[test]
fn distribution_syntax_and_canonical_identity_retain_the_premise_and_target() {
    let original = spec(PROJECT);
    assert_eq!(original.random_inputs()[0].bits(), 2);
    assert_eq!(original.distributions()[0].target().len(), 4);
    let spelling = PROJECT.replace(
        "uniform over {0, 1, 2, 3}",
        "{3: 2/8, 1: 1/4, 0: 1/4, 2: 1/4}",
    );
    assert_eq!(
        original.canonical_bytes().unwrap(),
        spec(&spelling).canonical_bytes().unwrap()
    );
    for changed in [
        PROJECT.replace("bits 2", "bits 1"),
        PROJECT.replace("{0, 1, 2, 3}", "{0, 1}"),
    ] {
        assert_ne!(
            original.canonical_bytes().unwrap(),
            spec(&changed).canonical_bytes().unwrap()
        );
    }
}

#[test]
fn distribution_rejects_invalid_targets_and_missing_premises() {
    for target in [
        "{}",
        "{0: 1/0}",
        "{0: -1/2, 1: 3/2}",
        "{0: 1/2}",
        "uniform over {}",
        "uniform over {0, 0}",
    ] {
        let source = PROJECT.replace("uniform over {0, 1, 2, 3}", target);
        assert!(
            parse_project(&source, SourceLimits::default()).is_err(),
            "{target}"
        );
    }
    for premise in ["", "uniform", "independent"] {
        assert!(
            parse_project(
                &PROJECT.replace("independent uniform", premise),
                SourceLimits::default()
            )
            .is_err()
        );
    }
}

#[test]
fn distribution_rational_arithmetic_is_exact_and_bounded() {
    assert_eq!(Probability::try_new(2, 4), Probability::try_new(1, 2));
    assert!(Probability::try_new(1, 0).is_none());
    assert!(Probability::try_new(2, 1).is_none());
    assert!(Probability::try_new(u128::MAX - 1, u128::MAX).is_none());
    let large = Probability::try_new(u128::from(u64::MAX) - 1, u128::from(u64::MAX)).unwrap();
    assert!(large.checked_add(large).is_none());
    for n in 1..=64 {
        let w = Probability::try_new(1, n).unwrap();
        let sum = (0..n)
            .try_fold(Probability::ZERO, |sum, _| sum.checked_add(w))
            .unwrap();
        assert_eq!(sum, Probability::ONE);
        assert!(sum.checked_add(w).is_none());
    }
}
