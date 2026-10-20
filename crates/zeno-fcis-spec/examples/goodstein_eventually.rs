//! Small concrete witnesses for eventually; this is not the general Goodstein proof.
use zeno_fcis_spec::{ClaimMode, ProjectLimits, SourceLimits, elaborate_project, parse_project};

const SOURCE: &str = include_str!("../../../examples/goodstein/project.zeno");

#[derive(Debug, Eq, PartialEq)]
enum Stop {
    Zero,
    Horizon,
    Arithmetic,
}

#[derive(Debug, Eq, PartialEq)]
struct Run {
    values: Vec<u128>,
    stop: Stop,
}

fn power(mut base: u128, mut exponent: u128) -> Option<u128> {
    let mut result = 1_u128;
    while exponent != 0 {
        if exponent % 2 == 1 {
            result = result.checked_mul(base)?;
        }
        exponent /= 2;
        if exponent != 0 {
            base = base.checked_mul(base)?;
        }
    }
    Some(result)
}

// Change the base recursively inside exponents as well as in the outer numeral.
// The recursive argument is an exponent smaller than the positive input value.
fn change_base(mut value: u128, base: u128, next_base: u128) -> Option<u128> {
    let mut result = 0_u128;
    let mut exponent = 0_u128;
    while value != 0 {
        let digit = value % base;
        if digit != 0 {
            let changed_exponent = change_base(exponent, base, next_base)?;
            let term = digit.checked_mul(power(next_base, changed_exponent)?)?;
            result = result.checked_add(term)?;
        }
        value /= base;
        exponent += 1;
    }
    Some(result)
}

// The horizon counts observed values, including the initial value at position 0.
// No overflow is interpreted as zero, a mathematical step, or nontermination.
fn observe(initial: u128, horizon: usize) -> Result<Run, &'static str> {
    if !(1..=256).contains(&horizon) {
        return Err("the observation horizon must be between 1 and 256");
    }
    let mut values = vec![initial];
    let mut value = initial;
    let mut base = 2_u128;
    while value != 0 && values.len() < horizon {
        let Some(next) = change_base(value, base, base + 1).and_then(|n| n.checked_sub(1)) else {
            return Ok(Run {
                values,
                stop: Stop::Arithmetic,
            });
        };
        value = next;
        values.push(value);
        base += 1;
    }
    Ok(Run {
        values,
        stop: if value == 0 {
            Stop::Zero
        } else {
            Stop::Horizon
        },
    })
}

fn main() -> Result<(), String> {
    let parsed = parse_project(SOURCE, SourceLimits::default()).map_err(|set| set.to_string())?;
    let project =
        elaborate_project(parsed, ProjectLimits::default()).map_err(|set| set.to_string())?;
    assert_eq!(project.claims()[0].mode(), ClaimMode::Finite { horizon: 6 });
    assert_eq!(project.claims()[1].mode(), ClaimMode::UnboundedProof);

    let complete = observe(3, 6)?;
    let witness = complete.values.iter().position(|value| *value == 0);
    assert_eq!(witness, Some(5));
    println!(
        "start 3: {:?}; zero witnessed at position 5",
        complete.values
    );

    let prefix = observe(3, 4)?;
    println!(
        "short prefix: {:?}; unbounded eventuality still pending",
        prefix.values
    );
    println!("if declared a completed finite trace, that prefix fails eventually zero");

    let growing = observe(4, 6)?;
    println!(
        "start 4: {:?}; horizon exhausted, no nontermination claim",
        growing.values
    );
    println!("the .zeno claims were parsed, not proved or bound to these runs");
    println!("the general Goodstein theorem requires a separate ordinal proof");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_small_sequence_reaches_zero_after_five_transitions() {
        assert_eq!(
            observe(3, 6),
            Ok(Run {
                values: vec![3, 3, 3, 2, 1, 0],
                stop: Stop::Zero
            })
        );
        assert_eq!(
            observe(0, 1),
            Ok(Run {
                values: vec![0],
                stop: Stop::Zero
            })
        );
    }

    #[test]
    fn horizon_exhaustion_keeps_the_unfinished_prefix() {
        assert_eq!(
            observe(3, 4),
            Ok(Run {
                values: vec![3, 3, 3, 2],
                stop: Stop::Horizon
            })
        );
        assert_eq!(
            observe(4, 6),
            Ok(Run {
                values: vec![4, 26, 41, 60, 83, 109],
                stop: Stop::Horizon
            })
        );
        assert!(observe(3, 0).is_err());
        assert!(observe(3, 257).is_err());
    }

    #[test]
    fn hereditary_change_rewrites_exponents() {
        // 4 = 2^2 becomes 3^3, rather than 3^2 (ordinary base change).
        assert_eq!(change_base(4, 2, 3), Some(27));
        assert_eq!(change_base(16, 2, 3), Some(7_625_597_484_987));
    }

    #[test]
    fn checked_arithmetic_never_manufactures_a_terminal_value() {
        assert_eq!(power(u128::MAX, 2), None);
        assert_eq!(power(u128::MAX, 0), Some(1));
        assert_eq!(
            observe(u128::MAX, 2),
            Ok(Run {
                values: vec![u128::MAX],
                stop: Stop::Arithmetic
            })
        );
    }

    #[test]
    fn authoring_keeps_finite_and_unbounded_claims_distinct() {
        let parsed =
            parse_project(SOURCE, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
        let project = elaborate_project(parsed, ProjectLimits::default())
            .unwrap_or_else(|set| panic!("{set}"));
        assert_eq!(project.claims()[0].formula(), project.claims()[1].formula());
        assert_eq!(project.claims()[0].mode(), ClaimMode::Finite { horizon: 6 });
        assert_eq!(project.claims()[1].mode(), ClaimMode::UnboundedProof);
    }
}
