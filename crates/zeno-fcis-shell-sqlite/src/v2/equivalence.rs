//! Exhaustive comparison of two decision programs on their full declared
//! input domain, through the library's metered evaluator.
//!
//! This is the predicate `zeno-fcis transform check` (F3) decides, without
//! its declared Step limit: both programs must have the same ordered input
//! and output domains; then every tuple of the ordered product of the input
//! domains, last input fastest, runs through `execute_v2` at the full budget
//! of `MAX_NODES` Steps. An admitted program has at most `MAX_NODES` nodes and
//! each instruction attempt costs one Step, so no run is refused for Steps and
//! a Step refusal cannot mask a difference; unused nodes and unselected arms
//! still run. The two programs are equal when every tuple gives identical
//! output tuples or the identical evaluator failure. The product's size is
//! computed with checked arithmetic and bounded by a caller's cap; only a
//! complete enumeration of exactly that many tuples constructs an [`Equal`],
//! with one exception: two programs with identical instructions and roots,
//! after the same checks, are equal on every tuple without enumeration,
//! because the evaluator is deterministic. No other shortcut is taken: no
//! sampling, no skipped inputs, no threads.
//!
//! Pure: no SQLite, I/O, clock, threads or ambient state, and no allocation
//! that grows with the domain. One tuple is stepped in place like an odometer;
//! no list of tuples or of a range's values is ever built. The shared checker
//! source is owned by this library; the CLI calls its public data API. Their
//! compatibility tests retain frozen expected results.

use zeno_fcis_synthesis::finite::V2ScalarProgram;
#[path = "equivalence/checker.rs"]
mod checker;
#[path = "equivalence/checker_api.rs"]
pub mod finite_checker;
#[cfg(test)]
use checker::{advance, domain_size};
#[cfg(test)]
use zeno_fcis_synthesis::finite::Domain;

/// The default cap on the input tuples one comparison may enumerate: F3's default.
pub const DEFAULT_MAX_INPUT_TUPLES: u64 = 100_000_000;

/// A completed comparison that found the two programs equal on every input
/// tuple. Only [`compare`] constructs one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Equal {
    tuples: u64,
    enumerated: bool,
}

impl Equal {
    /// The number of input tuples on which the programs are equal: the
    /// whole declared domain.
    pub fn tuples(self) -> u64 {
        self.tuples
    }

    /// Whether every tuple was evaluated; false only for two programs with
    /// identical instructions and roots.
    pub fn enumerated(self) -> bool {
        self.enumerated
    }
}

/// Why a comparison did not establish that the programs are equal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Unestablished {
    /// Input count, order or domains differ.
    InputAbi,
    /// Output count, order or domains differ.
    OutputAbi,
    /// The input domain at `position` has no values.
    EmptyInputDomain {
        /// Zero-based input position.
        position: usize,
    },
    /// The domain has more tuples than the cap; `size` is `None` when the
    /// product exceeds `u128::MAX`. Nothing was evaluated.
    DomainTooLarge {
        /// Exact number of tuples, when representable.
        size: Option<u128>,
        /// The cap the comparison ran under.
        cap: u64,
    },
    /// The first tuple, in enumeration order from zero, whose results differ.
    Counterexample {
        /// Position of the tuple in enumeration order.
        ordinal: u64,
    },
    /// Defensive: the odometer did not stop exactly after the computed number
    /// of tuples.
    CoverageMismatch {
        /// The computed number of tuples.
        expected: u64,
        /// The tuples the odometer produced.
        visited: u64,
    },
}

/// Compares `original` and `candidate` on every tuple of their declared input
/// domain, if it has at most `max_input_tuples` tuples.
///
/// Refusal order: input domains, output domains, an empty input domain, then
/// a domain above the cap; otherwise the first tuple whose results differ.
/// Programs with identical instructions and roots pass those same checks and
/// are then equal without enumeration.
///
/// # Errors
/// Returns why equality was not established.
pub fn compare(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    max_input_tuples: u64,
) -> Result<Equal, Unestablished> {
    checker::compare_equal(original, candidate, max_input_tuples)
        .map(|equal| Equal {
            tuples: equal.tuples(),
            enumerated: equal.enumerated(),
        })
        .map_err(|error| match error {
            checker::Failure::InputAbi => Unestablished::InputAbi,
            checker::Failure::OutputAbi => Unestablished::OutputAbi,
            checker::Failure::EmptyInputDomain { position } => {
                Unestablished::EmptyInputDomain { position }
            }
            checker::Failure::DomainTooLarge { size, limit } => {
                Unestablished::DomainTooLarge { size, cap: limit }
            }
            checker::Failure::Counterexample { ordinal, .. } => {
                Unestablished::Counterexample { ordinal }
            }
            checker::Failure::CoverageMismatch { expected, visited } => {
                Unestablished::CoverageMismatch { expected, visited }
            }
            // The equality-only route has no Step-boundary branch. Keep mapping
            // total and fail closed if that internal invariant ever changes.
            checker::Failure::BudgetBoundary { .. } => Unestablished::CoverageMismatch {
                expected: 0,
                visited: 0,
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_synthesis::finite::Op;

    const BOOL: Domain = Domain::Bool;
    const SMALL: Domain = Domain::Int { min: -1, max: 2 };

    fn program<'a>(
        inputs: &'a [Domain],
        outputs: &'a [Domain],
        nodes: &'a [Op],
        roots: &'a [u16],
    ) -> V2ScalarProgram<'a> {
        V2ScalarProgram {
            inputs,
            outputs,
            nodes,
            roots,
        }
    }

    #[test]
    fn equal_programs_are_compared_on_every_tuple() {
        let inputs = [BOOL, SMALL];
        let outputs = [BOOL];
        // `a` and `not not a`.
        let plain = [Op::Input(0)];
        let doubled = [Op::Input(0), Op::Not(0), Op::Not(1)];
        let equal = compare(
            &program(&inputs, &outputs, &plain, &[0]),
            &program(&inputs, &outputs, &doubled, &[2]),
            8,
        );
        assert_eq!(equal.map(Equal::tuples), Ok(8));
        assert_eq!(equal.map(Equal::enumerated), Ok(true));
        // Zero inputs: one empty tuple.
        let constant = [Op::Bool(true)];
        let equal = compare(
            &program(&[], &outputs, &constant, &[0]),
            &program(&[], &outputs, &constant, &[0]),
            1,
        );
        assert_eq!(equal.map(Equal::tuples), Ok(1));
    }

    #[test]
    fn identical_programs_are_equal_without_enumeration_after_the_same_checks() {
        let inputs = [BOOL, SMALL];
        let outputs = [SMALL];
        // A program that traps on every tuple is still equal to itself.
        let nodes = [Op::Input(1), Op::Int(i64::MAX), Op::Add(1, 1)];
        let same = compare(
            &program(&inputs, &outputs, &nodes, &[0]),
            &program(&inputs, &outputs, &nodes, &[0]),
            8,
        );
        assert_eq!(
            same,
            Ok(Equal {
                tuples: 8,
                enumerated: false
            })
        );
        // The refusals still come first.
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &nodes, &[0]),
                &program(&inputs, &outputs, &nodes, &[0]),
                7,
            ),
            Err(Unestablished::DomainTooLarge {
                size: Some(8),
                cap: 7
            })
        );
        let wide = [Domain::Int { min: -1, max: 3 }];
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &nodes, &[0]),
                &program(&inputs, &wide, &nodes, &[0]),
                8,
            ),
            Err(Unestablished::OutputAbi)
        );
        // The same nodes with other roots are enumerated.
        let copies = [Op::Input(1), Op::Input(1)];
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &copies, &[0]),
                &program(&inputs, &outputs, &copies, &[1]),
                8,
            ),
            Ok(Equal {
                tuples: 8,
                enumerated: true
            })
        );
    }

    #[test]
    fn the_first_differing_tuple_in_enumeration_order_is_reported() {
        let inputs = [BOOL, SMALL];
        let outputs = [SMALL];
        // `b`, and `b` except 0 where `a` holds and `b` is 2.
        let plain = [Op::Input(1)];
        let changed = [
            Op::Input(0),
            Op::Input(1),
            Op::Int(2),
            Op::Eq(1, 2),
            Op::And(0, 3),
            Op::Int(0),
            Op::Select(4, 5, 1),
        ];
        // Tuples run (0,-1) (0,0) (0,1) (0,2) (1,-1) (1,0) (1,1) (1,2).
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &plain, &[0]),
                &program(&inputs, &outputs, &changed, &[6]),
                8,
            ),
            Err(Unestablished::Counterexample { ordinal: 7 })
        );
    }

    #[test]
    fn a_trap_in_an_unselected_arm_is_a_difference_and_equal_traps_are_not() {
        let inputs = [SMALL];
        let outputs = [SMALL];
        let plain = [Op::Input(0)];
        // An unused overflowing node still runs and traps on every tuple.
        let trapping = [
            Op::Input(0),
            Op::Int(i64::MAX),
            Op::Add(1, 1),
            Op::Bool(true),
            Op::Select(3, 0, 0),
        ];
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &plain, &[0]),
                &program(&inputs, &outputs, &trapping, &[4]),
                4,
            ),
            Err(Unestablished::Counterexample { ordinal: 0 })
        );
        assert_eq!(
            compare(
                &program(&inputs, &outputs, &trapping, &[4]),
                &program(&inputs, &outputs, &trapping, &[0]),
                4,
            )
            .map(Equal::tuples),
            Ok(4)
        );
    }

    #[test]
    fn refusals_come_in_order_before_any_evaluation() {
        let nodes = [Op::Input(0)];
        let small = [SMALL];
        let wide = [Domain::Int { min: -1, max: 3 }];
        assert_eq!(
            compare(
                &program(&small, &small, &nodes, &[0]),
                &program(&wide, &small, &nodes, &[0]),
                100,
            ),
            Err(Unestablished::InputAbi)
        );
        assert_eq!(
            compare(
                &program(&small, &small, &nodes, &[0]),
                &program(&small, &wide, &nodes, &[0]),
                100,
            ),
            Err(Unestablished::OutputAbi)
        );
        let empty = [BOOL, Domain::Int { min: 1, max: 0 }];
        assert_eq!(
            compare(
                &program(&empty, &small, &nodes, &[0]),
                &program(&empty, &small, &nodes, &[0]),
                100,
            ),
            Err(Unestablished::EmptyInputDomain { position: 1 })
        );
        // One tuple above the cap, and a product beyond u128.
        assert_eq!(
            compare(
                &program(&small, &small, &nodes, &[0]),
                &program(&small, &small, &nodes, &[0]),
                3,
            ),
            Err(Unestablished::DomainTooLarge {
                size: Some(4),
                cap: 3
            })
        );
        let full = Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        };
        let huge = [full, full, full];
        assert_eq!(
            compare(
                &program(&huge, &small, &nodes, &[0]),
                &program(&huge, &small, &nodes, &[0]),
                u64::MAX,
            ),
            Err(Unestablished::DomainTooLarge {
                size: None,
                cap: u64::MAX
            })
        );
        // Two full i64 domains: 2^128 tuples overflow u128 by one.
        assert_eq!(domain_size(&[full, full]), Ok(None));
        assert_eq!(domain_size(&[full]), Ok(Some(1 << 64)));
    }

    #[test]
    fn the_odometer_visits_each_tuple_once_up_to_the_i64_endpoints() {
        let domains = [
            Domain::Int {
                min: i64::MAX - 1,
                max: i64::MAX,
            },
            BOOL,
        ];
        let mut tuple = [i64::MAX - 1, 0];
        let mut seen = vec![tuple];
        while advance(&domains, &mut tuple) {
            seen.push(tuple);
        }
        assert_eq!(
            seen,
            [
                [i64::MAX - 1, 0],
                [i64::MAX - 1, 1],
                [i64::MAX, 0],
                [i64::MAX, 1]
            ]
        );
        assert_eq!(tuple, [i64::MAX - 1, 0]);
    }
}
