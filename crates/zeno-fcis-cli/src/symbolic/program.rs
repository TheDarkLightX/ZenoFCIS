//! A `finite-i128/1` program as SMT definitions, following the library
//! evaluator (`finite::execute_v2`) exactly: every node is evaluated,
//! including unused nodes and unselected arms; addition and subtraction are
//! checked in `i128`; Booleans are the integers 0 and 1 on the wire; and every
//! output must lie in its declared domain.
//!
//! The evaluator stops at the first trapping node and reports `Arithmetic`,
//! whichever node it is, so the program traps exactly when some checked
//! node leaves the `i128` range. It reports `OutputDomain` only for a program
//! that does not trap.

#[cfg(test)]
use zeno_fcis_synthesis::finite::{Domain, Op};

#[cfg(test)]
use super::smt::{Script, Sort, Term};

#[cfg(verus_keep_ghost)]
#[path = "program/bridge_spec.rs"]
pub(crate) mod bridge;
#[path = "program/node.rs"]
mod node;
#[path = "program/nodes.rs"]
mod nodes;
#[path = "program/roots.rs"]
mod roots;

#[path = "program/encoding.rs"]
mod encoding;
pub(crate) use encoding::{Encoded, Program, encode, input_values};

#[path = "program/library.rs"]
mod library;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use zeno_fcis_synthesis::finite::{Program as LibraryProgram, V2ExecutionFailure};

    use super::*;

    fn program(
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        nodes: Vec<Op>,
        roots: Vec<u16>,
    ) -> LibraryProgram {
        LibraryProgram::try_new(inputs, outputs, nodes, roots)
            .unwrap_or_else(|error| panic!("test program must be admitted: {error}"))
    }

    /// Encodes `program` with observed outputs and trap flags, so an
    /// evaluation at one input reads them back.
    fn script(program: &LibraryProgram) -> (Script, usize) {
        let mut script = Script::default();
        for (index, domain) in program.inputs().iter().enumerate() {
            let values = input_values(*domain).unwrap_or_else(|error| panic!("{error:?}"));
            script.constant(&format!("x{index}"), values);
        }
        let input: Vec<Term> = (0..program.inputs().len())
            .map(|index| Term::name(&format!("x{index}")))
            .collect();
        let encoded = encode(&mut script, "p", Program::of(program), &input)
            .unwrap_or_else(|error| panic!("encode: {error:?}"));
        for (index, output) in encoded.outputs.iter().enumerate() {
            script.define(&format!("out{index}"), Sort::Int, output.clone());
        }
        script.define("trap", Sort::Bool, encoded.trap.clone());
        script.define("ood", Sort::Bool, encoded.out_of_domain());
        (script, encoded.outputs.len())
    }

    #[test]
    fn missing_input_terms_are_refused_and_supplied_names_are_used() {
        let integer = Domain::Int { min: 0, max: 9 };
        let library = program(vec![integer], vec![integer], vec![Op::Input(0)], vec![0]);
        let mut refused = Script::default();
        assert!(
            encode(&mut refused, "p", Program::of(&library), &[]).is_err(),
            "an unsupplied input must be refused, not panic"
        );
        let mut script = Script::default();
        encode(
            &mut script,
            "p",
            Program::of(&library),
            &[Term::name("custom_wire")],
        )
        .unwrap_or_else(|error| panic!("encode: {error:?}"));
        assert!(
            script
                .source()
                .contains("(define-fun p0 () Int custom_wire)")
        );
    }

    /// The encoding agrees with the library evaluator at every listed input:
    /// the same outputs, the same trap, the same output-domain failure.
    fn agrees(program: &LibraryProgram, inputs: &[Vec<i128>]) {
        let (script, outputs) = script(program);
        for input in inputs {
            let assignment: BTreeMap<String, i128> = input
                .iter()
                .enumerate()
                .map(|(index, value)| (format!("x{index}"), i128::from(*value)))
                .collect();
            let evaluation = script
                .evaluate(&assignment)
                .unwrap_or_else(|| panic!("{input:?} is outside the declared inputs"));
            let library = program.execute_v2(
                input,
                zeno_fcis_synthesis::finite::v2_zero_limits().with_limit(
                    zeno_fcis_synthesis::finite::V2Resource::Step,
                    crate::transform::FULL_BUDGET,
                ),
            );
            let (result, _) = library.into_parts();
            let trap = evaluation.boolean("trap");
            let ood = evaluation.boolean("ood");
            match result {
                Ok(values) => {
                    assert_eq!((trap, ood), (Some(false), Some(false)), "{input:?}");
                    let encoded: Vec<Option<i128>> = (0..outputs)
                        .map(|index| evaluation.int(&format!("out{index}")))
                        .collect();
                    let expected: Vec<Option<i128>> = values
                        .iter()
                        .map(|value| Some(i128::from(*value)))
                        .collect();
                    assert_eq!(encoded, expected, "{input:?}");
                }
                Err(V2ExecutionFailure::Arithmetic) => assert_eq!(trap, Some(true), "{input:?}"),
                Err(V2ExecutionFailure::OutputDomain) => {
                    assert_eq!((trap, ood), (Some(false), Some(true)), "{input:?}");
                }
                Err(other) => panic!("{input:?}: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn checked_arithmetic_traps_where_the_evaluator_traps() {
        let wide = Domain::Int {
            min: i128::MIN,
            max: i128::MAX,
        };
        // An unused, unselected sum still traps: evaluation is eager.
        let program = program(
            vec![wide, wide, Domain::Bool],
            vec![wide, Domain::Bool],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Input(2),
                Op::Add(0, 1),
                Op::Sub(0, 1),
                Op::Select(2, 3, 4),
                Op::Lt(0, 1),
                Op::Eq(2, 6),
                Op::Not(7),
                Op::And(8, 2),
                Op::Int(7),
                Op::Add(0, 10),
            ],
            vec![5, 9],
        );
        let mut inputs = Vec::new();
        for a in [
            i128::MIN,
            i128::MIN + 1,
            -7,
            -1,
            0,
            1,
            7,
            i128::MAX - 7,
            i128::MAX,
        ] {
            for b in [i128::MIN, -1, 0, 1, i128::MAX] {
                for flag in [0, 1] {
                    inputs.push(vec![a, b, flag]);
                }
            }
        }
        agrees(&program, &inputs);
    }

    #[test]
    fn outputs_outside_their_domains_fail_after_every_node_is_defined() {
        let small = Domain::Int { min: 0, max: 3 };
        let program = program(
            vec![small, small],
            vec![small, Domain::Bool],
            vec![Op::Input(0), Op::Input(1), Op::Add(0, 1), Op::Eq(0, 1)],
            vec![2, 3],
        );
        let inputs: Vec<Vec<i128>> = (0..=3)
            .flat_map(|a| (0..=3).map(move |b| vec![a, b]))
            .collect();
        agrees(&program, &inputs);
    }

    #[test]
    fn eager_overflow_precedes_an_already_out_of_domain_root() {
        let small = Domain::Int { min: 0, max: 1 };
        let program = program(
            vec![small],
            vec![small],
            vec![Op::Int(2), Op::Int(i128::MAX), Op::Input(0), Op::Add(1, 2)],
            vec![0],
        );
        agrees(&program, &[vec![0], vec![1]]);
        let (script, _) = script(&program);
        let zero = script
            .evaluate(&BTreeMap::from([("x0".to_owned(), 0)]))
            .unwrap_or_else(|| panic!("admitted input"));
        assert_eq!(
            (zero.boolean("trap"), zero.boolean("ood")),
            (Some(false), Some(true))
        );
        let one = script
            .evaluate(&BTreeMap::from([("x0".to_owned(), 1)]))
            .unwrap_or_else(|| panic!("admitted input"));
        assert_eq!(
            (one.boolean("trap"), one.boolean("ood")),
            (Some(true), Some(false))
        );
    }

    #[test]
    fn boolean_wires_and_zero_one_integers_keep_distinct_sorts() {
        let integer = Domain::Int { min: 0, max: 1 };
        let program = program(
            vec![Domain::Bool, integer],
            vec![Domain::Bool, integer, integer],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Not(0),
                Op::Int(0),
                Op::Select(0, 1, 3),
            ],
            vec![2, 4, 1],
        );
        agrees(&program, &[vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]);
        let (script, _) = script(&program);
        let text = script.source();
        assert!(text.contains("(define-fun p0 () Bool (= x0 1))"));
        assert!(text.contains("(define-fun p1 () Int x1)"));
        assert!(text.contains("(define-fun out0 () Int (ite p2 1 0))"));
    }

    #[test]
    fn input_domains_bound_the_declared_constants() {
        let program = program(
            vec![Domain::Bool, Domain::Int { min: -2, max: 5 }],
            vec![Domain::Bool],
            vec![Op::Input(0)],
            vec![0],
        );
        let (script, _) = script(&program);
        assert!(
            script
                .source()
                .contains("(assert (and (<= 0 x0) (<= x0 1)))")
        );
        assert!(
            script
                .source()
                .contains("(assert (and (<= (- 2) x1) (<= x1 5)))")
        );
        let outside = BTreeMap::from([("x0".to_owned(), 2), ("x1".to_owned(), 0)]);
        assert!(script.evaluate(&outside).is_none());
    }
}
