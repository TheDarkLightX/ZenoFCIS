//! Random-bit declarations, canonical identities and exact rational arithmetic.

use zeno_fcis_spec::{
    DistributionOutcome, Probability, ProjectLimits, SourceLimits, elaborate_project, parse_project,
};

const PROJECT: &str =
    include_str!("../../../examples/distribution-laws/reporter-choice/project.zeno");

fn spec(source: &str) -> Result<zeno_fcis_spec::ProjectSpec, Box<dyn std::error::Error>> {
    let parsed =
        parse_project(source, SourceLimits::default()).map_err(|error| error.to_string())?;
    elaborate_project(parsed, ProjectLimits::default()).map_err(|error| error.to_string().into())
}

#[test]
fn distribution_syntax_and_canonical_identity_retain_the_premise_and_target()
-> Result<(), Box<dyn std::error::Error>> {
    let original = spec(PROJECT)?;
    assert_eq!(original.random_inputs()[0].bits(), 2);
    assert_eq!(original.distributions()[0].target().len(), 4);
    let spelling = PROJECT.replace(
        "uniform over {0, 1, 2, 3}",
        "{3: 2/8, 1: 1/4, 0: 1/4, 2: 1/4}",
    );
    assert_eq!(
        original.canonical_bytes()?,
        spec(&spelling)?.canonical_bytes()?
    );
    for changed in [
        PROJECT.replace("bits 2", "bits 1"),
        PROJECT.replace("{0, 1, 2, 3}", "{0, 1}"),
    ] {
        assert_ne!(
            original.canonical_bytes()?,
            spec(&changed)?.canonical_bytes()?
        );
    }
    Ok(())
}

#[test]
fn distribution_targets_may_declare_committed_failure_mass()
-> Result<(), Box<dyn std::error::Error>> {
    let declared = |target: &str| spec(&PROJECT.replace("uniform over {0, 1, 2, 3}", target));
    let explicit = declared("{committed-failure: 1/4, 0: 1/4, 1: 1/4, 2: 1/4}")?;
    assert_eq!(
        explicit.distributions()[0].target()[3],
        (
            DistributionOutcome::CommittedFailure,
            Probability::try_new(1, 4).ok_or("invalid 1/4 probability")?
        )
    );
    assert_eq!(
        explicit.canonical_bytes()?,
        declared("uniform over {0, 1, 2, committed-failure}")?.canonical_bytes()?
    );
    // The failure outcome is distinct from any value it might write.
    assert_ne!(
        explicit.canonical_bytes()?,
        spec(PROJECT)?.canonical_bytes()?
    );
    for target in [
        "{committed-failure: 1/2, committed-failure: 1/2}",
        "{0: 1/2, committed-failure: 1/4}",
        "uniform over {committed-failure, committed-failure}",
    ] {
        let source = PROJECT.replace("uniform over {0, 1, 2, 3}", target);
        assert!(
            parse_project(&source, SourceLimits::default()).is_err(),
            "{target}"
        );
    }
    Ok(())
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
fn distribution_rational_arithmetic_is_exact_and_bounded() -> Result<(), Box<dyn std::error::Error>>
{
    assert_eq!(Probability::try_new(2, 4), Probability::try_new(1, 2));
    assert!(Probability::try_new(1, 0).is_none());
    assert!(Probability::try_new(2, 1).is_none());
    assert!(Probability::try_new(u128::MAX - 1, u128::MAX).is_none());
    let large = Probability::try_new(u128::from(u64::MAX) - 1, u128::from(u64::MAX))
        .ok_or("invalid large probability")?;
    assert!(large.checked_add(large).is_none());
    for n in 1..=64 {
        let w = Probability::try_new(1, n).ok_or("invalid unit fraction")?;
        let sum = (0..n)
            .try_fold(Probability::ZERO, |sum, _| sum.checked_add(w))
            .ok_or("probability sum overflowed")?;
        assert_eq!(sum, Probability::ONE);
        assert!(sum.checked_add(w).is_none());
    }
    Ok(())
}

#[test]
fn reward_syntax_is_canonical_and_binds_every_parameter() -> Result<(), Box<dyn std::error::Error>>
{
    let reward = "reward 610 payout random {600} on post.100.110 failure -1 { expectation >= -3/2; variance <= 5/4; threshold 2 = 1/2; excess 2 <= 1/4; }";
    let source = format!("{PROJECT}\n{reward}");
    let original = spec(&source)?;
    assert_eq!(original.rewards()[0].random()[0].get(), 600);
    assert_eq!(original.rewards()[0].failure(), Some(-1));
    assert_eq!(original.rewards()[0].measures().len(), 4);
    let spelling = source.replace(
        "expectation >= -3/2; variance <= 5/4; threshold 2 = 1/2; excess 2 <= 1/4;",
        "excess 2 <= 2/8; threshold 2 = 2/4; variance <= 10/8; expectation >= -6/4;",
    );
    assert_eq!(
        original.canonical_bytes()?,
        spec(&spelling)?.canonical_bytes()?
    );
    let bytes = original.canonical_bytes()?;
    assert_eq!(
        &bytes[b"ZFCIS-PROJECT-SPEC\0".len()..][..2],
        &4u16.to_be_bytes()
    );
    for changed in [
        source.replace("610 payout", "611 payout"),
        source.replace("failure -1", "failure 0"),
        source.replace("failure -1 ", ""),
        source.replace("threshold 2", "threshold 1"),
        source.replace("excess 2", "excess 1"),
        source.replace("5/4", "6/4"),
        source.replace("expectation >=", "expectation <="),
    ] {
        assert_ne!(
            original.canonical_bytes()?,
            spec(&changed)?.canonical_bytes()?
        );
    }
    // Format 4 now appends the reward count even when it is zero.
    let without = spec(PROJECT)?.canonical_bytes()?;
    assert_eq!(&without[without.len() - 4..], &0u32.to_be_bytes());
    Ok(())
}
#[test]
fn reward_invalid_declarations_and_premises_fail_closed() {
    for declaration in [
        "reward 610 r random {} on post.100.110 { expectation = 0; }",
        "reward 610 r random {600,600} on post.100.110 { expectation = 0; }",
        "reward 610 r random {999} on post.100.110 { expectation = 0; }",
        "reward 610 r random {600} on post.100.999 { expectation = 0; }",
        "reward 610 r random {600} on post.100.110 {}",
        "reward 610 r random {600} on post.100.110 { expectation = 0; expectation = 0; }",
        "reward 610 r random {600} on post.100.110 { expectation = 1/0; }",
        "reward 610 r random {600} on post.100.110 { expectation = 9223372036854775808; }",
        "reward 610 r random {600} on post.100.110 { expectation > 0; }",
        "reward 610 r random {600} on post.100.110 { median = 0; }",
    ] {
        assert!(
            spec(&format!("{PROJECT}\n{declaration}")).is_err(),
            "{declaration}"
        );
    }
    for declarations in [
        "reward 610 r random {600} on post.100.110 { expectation = 0; } reward 610 s random {600} on post.100.110 { expectation = 0; }",
        "reward 610 r random {600} on post.100.110 { expectation = 0; } reward 611 r random {600} on post.100.110 { expectation = 0; }",
        "random 602 other on context.102.130 bits 2 independent uniform; reward 610 r random {600} on post.100.110 { expectation = 0; }",
    ] {
        assert!(spec(&format!("{PROJECT}\n{declarations}")).is_err());
    }
}
#[test]
fn reward_signed_rationals_are_exact_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
    use zeno_fcis_spec::Rational;
    let r = |n, d| Rational::try_new(n, d).ok_or("rational bounds");
    assert_eq!(r(-2, 4)?, r(-1, 2)?);
    assert!(Rational::try_new(1, 0).is_none());
    assert_eq!(r(-1, 2)?.checked_add(r(3, 2)?), Some(r(1, 1)?));
    assert_eq!(r(-1, 2)?.checked_mul(r(-3, 2)?), Some(r(3, 4)?));
    assert_eq!(r(-1, 2)?.checked_sub(r(3, 2)?), Some(r(-2, 1)?));
    assert!(r(i128::from(i64::MAX), 1)?.checked_add(r(1, 1)?).is_none());
    assert_eq!(r(i128::MIN, 1u128 << 127)?, r(-1, 1)?);
    assert_eq!(r(0, u128::MAX)?, Rational::ZERO);
    assert!(r(i128::from(i64::MIN), u128::from(u64::MAX))? < Rational::ZERO);
    Ok(())
}
