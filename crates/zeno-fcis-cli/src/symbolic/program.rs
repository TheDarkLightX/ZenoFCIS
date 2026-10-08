//! A `finite-i64/1` program as SMT definitions, following the library
//! evaluator (`finite::execute_v2`) exactly: every node is evaluated,
//! including unused nodes and unselected arms; addition and subtraction are
//! checked in `i64`; Booleans are the integers 0 and 1 on the wire; and every
//! output must lie in its declared domain.
//!
//! The evaluator stops at the first trapping node and reports `Arithmetic`,
//! whichever node it is, so the program traps exactly when some checked
//! node leaves the `i64` range. It reports `OutputDomain` only for a program
//! that does not trap.

use zeno_fcis_synthesis::finite::{Domain, Op, Program as LibraryProgram};

use super::Unsupported;
use super::smt::{Script, Sort, Term};

/// An admitted program's parts, as the library's `Program` and a bound
/// descriptor's `ScalarProgram` both hold them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Program<'a> {
    pub(crate) inputs: &'a [Domain],
    pub(crate) outputs: &'a [Domain],
    pub(crate) nodes: &'a [Op],
    pub(crate) roots: &'a [u16],
}

impl<'a> Program<'a> {
    pub(crate) fn of(program: &'a LibraryProgram) -> Self {
        Self {
            inputs: program.inputs(),
            outputs: program.outputs(),
            nodes: program.nodes(),
            roots: program.roots(),
        }
    }
}

/// A program's terms over given input terms.
#[derive(Clone, Debug)]
pub(crate) struct Encoded {
    /// Each output's wire value, Booleans as 0 and 1.
    pub(crate) outputs: Vec<Term>,
    /// True when some checked addition or subtraction leaves the `i64` range:
    /// the evaluator's `Arithmetic` failure.
    pub(crate) trap: Term,
    /// True when every output lies in its declared domain. The evaluator
    /// checks this only when nothing trapped.
    pub(crate) admitted: Term,
}

impl Encoded {
    /// The evaluator returns outputs: nothing traps and every output is in
    /// its domain.
    pub(crate) fn ok(&self) -> Term {
        Term::and([Term::not(self.trap.clone()), self.admitted.clone()])
    }

    /// The evaluator fails with `OutputDomain`.
    pub(crate) fn out_of_domain(&self) -> Term {
        Term::and([
            Term::not(self.trap.clone()),
            Term::not(self.admitted.clone()),
        ])
    }
}

/// Whether a node is Boolean, as program admission types it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Bool,
    Int,
}

/// Defines every node of `program` in `script` as `{prefix}{index}`, with
/// `input(i)` the wire value of input `i`.
///
/// # Errors
/// An instruction or domain this encoding does not know, or a node whose
/// operand kinds admission would have refused.
pub(crate) fn encode(
    script: &mut Script,
    prefix: &str,
    program: Program<'_>,
    input: &dyn Fn(usize) -> Term,
) -> Result<Encoded, Unsupported> {
    let mut kinds = Vec::with_capacity(program.nodes.len());
    let mut terms: Vec<Term> = Vec::with_capacity(program.nodes.len());
    let mut checked = Vec::new();
    for (index, op) in program.nodes.iter().enumerate() {
        let operand = |id: u16, wanted: Option<Kind>| -> Result<(Term, Kind), Unsupported> {
            let id = usize::from(id);
            match (terms.get(id), kinds.get(id)) {
                (Some(term), Some(kind)) if wanted.is_none_or(|wanted| wanted == *kind) => {
                    Ok((term.clone(), *kind))
                }
                _ => Err(Unsupported::new(format!(
                    "node {index} reads node {id}, which is later or of another kind"
                ))),
            }
        };
        let (kind, term) = match *op {
            Op::Input(id) => {
                let id = usize::from(id);
                let wire = input(id);
                match program.inputs.get(id) {
                    Some(Domain::Bool) => (Kind::Bool, Term::eq(wire, Term::Int(1))),
                    Some(Domain::Int { .. }) => (Kind::Int, wire),
                    _ => {
                        return Err(Unsupported::new(format!(
                            "node {index} reads input {id}, which has no supported domain"
                        )));
                    }
                }
            }
            Op::Int(value) => (Kind::Int, Term::Int(i128::from(value))),
            Op::Bool(value) => (Kind::Bool, Term::Bool(value)),
            Op::Add(a, b) | Op::Sub(a, b) => {
                let (a, _) = operand(a, Some(Kind::Int))?;
                let (b, _) = operand(b, Some(Kind::Int))?;
                let term = if matches!(op, Op::Add(..)) {
                    Term::add(a, b)
                } else {
                    Term::sub(a, b)
                };
                (Kind::Int, term)
            }
            Op::Eq(a, b) => {
                let (a, kind) = operand(a, None)?;
                let (b, _) = operand(b, Some(kind))?;
                (Kind::Bool, Term::eq(a, b))
            }
            Op::Lt(a, b) => {
                let (a, _) = operand(a, Some(Kind::Int))?;
                let (b, _) = operand(b, Some(Kind::Int))?;
                (Kind::Bool, Term::lt(a, b))
            }
            Op::And(a, b) => {
                let (a, _) = operand(a, Some(Kind::Bool))?;
                let (b, _) = operand(b, Some(Kind::Bool))?;
                (Kind::Bool, Term::and([a, b]))
            }
            Op::Not(a) => {
                let (a, _) = operand(a, Some(Kind::Bool))?;
                (Kind::Bool, Term::not(a))
            }
            Op::Select(condition, a, b) => {
                let (condition, _) = operand(condition, Some(Kind::Bool))?;
                let (a, kind) = operand(a, None)?;
                let (b, _) = operand(b, Some(kind))?;
                (kind, Term::ite(condition, a, b))
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "node {index} is an instruction this encoding does not know: {op:?}"
                )));
            }
        };
        let name = format!("{prefix}{index}");
        let sort = match kind {
            Kind::Bool => Sort::Bool,
            Kind::Int => Sort::Int,
        };
        let defined = script.define(&name, sort, term);
        if matches!(op, Op::Add(..) | Op::Sub(..)) {
            checked.push(Term::not(Term::within(
                &defined,
                i128::from(i64::MIN),
                i128::from(i64::MAX),
            )));
        }
        kinds.push(kind);
        terms.push(defined);
    }
    if program.roots.len() != program.outputs.len() {
        return Err(Unsupported::new(
            "the program's roots and outputs differ in number",
        ));
    }
    let mut outputs = Vec::with_capacity(program.roots.len());
    let mut admitted = Vec::with_capacity(program.roots.len());
    for (position, (root, domain)) in program.roots.iter().zip(program.outputs).enumerate() {
        let root = usize::from(*root);
        let (Some(term), Some(kind)) = (terms.get(root), kinds.get(root)) else {
            return Err(Unsupported::new(format!(
                "output {position} names node {root}, which does not exist"
            )));
        };
        match (domain, kind) {
            (Domain::Bool, Kind::Bool) => {
                outputs.push(Term::ite(term.clone(), Term::Int(1), Term::Int(0)));
            }
            (Domain::Int { min, max }, Kind::Int) => {
                admitted.push(Term::within(term, i128::from(*min), i128::from(*max)));
                outputs.push(term.clone());
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "output {position} has a domain this encoding does not know, or another kind"
                )));
            }
        }
    }
    Ok(Encoded {
        outputs,
        trap: Term::or(checked),
        admitted: Term::and(admitted),
    })
}

/// The values of an input domain.
pub(crate) fn input_values(domain: Domain) -> Result<super::smt::Values, Unsupported> {
    match domain {
        Domain::Bool => Ok(super::smt::Values::Range { min: 0, max: 1 }),
        Domain::Int { min, max } => Ok(super::smt::Values::Range {
            min: i128::from(min),
            max: i128::from(max),
        }),
        _ => Err(Unsupported::new(
            "an input domain this encoding does not know",
        )),
    }
}

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
        let encoded = encode(&mut script, "p", Program::of(program), &|index| {
            Term::name(&format!("x{index}"))
        })
        .unwrap_or_else(|error| panic!("encode: {error:?}"));
        for (index, output) in encoded.outputs.iter().enumerate() {
            script.define(&format!("out{index}"), Sort::Int, output.clone());
        }
        script.define("trap", Sort::Bool, encoded.trap.clone());
        script.define("ood", Sort::Bool, encoded.out_of_domain());
        (script, encoded.outputs.len())
    }

    /// The encoding agrees with the library evaluator at every listed input:
    /// the same outputs, the same trap, the same output-domain failure.
    fn agrees(program: &LibraryProgram, inputs: &[Vec<i64>]) {
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
            min: i64::MIN,
            max: i64::MAX,
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
            i64::MIN,
            i64::MIN + 1,
            -7,
            -1,
            0,
            1,
            7,
            i64::MAX - 7,
            i64::MAX,
        ] {
            for b in [i64::MIN, -1, 0, 1, i64::MAX] {
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
        let inputs: Vec<Vec<i64>> = (0..=3)
            .flat_map(|a| (0..=3).map(move |b| vec![a, b]))
            .collect();
        agrees(&program, &inputs);
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
