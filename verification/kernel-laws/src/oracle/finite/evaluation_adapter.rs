//! Private relocation adapter; execution still uses the actual finite evaluator.
//!
//! `PreparedFold` observes only the result tuple and error. Its old node buffer
//! is write-only scratch. Delegating through `Program::evaluate` preserves those
//! observations but gives up reuse of the evaluator's internal buffer capacity.
//! Neither this trait nor a native authoring operation is exported to consumers.

use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Error, Program};

pub(super) trait PreparationEvaluation {
    fn evaluate_into(
        &self,
        input: &[i64],
        nodes: &mut Vec<i64>,
        output: &mut Vec<i64>,
    ) -> Result<(), Error>;
}

impl PreparationEvaluation for Program {
    fn evaluate_into(
        &self,
        input: &[i64],
        nodes: &mut Vec<i64>,
        output: &mut Vec<i64>,
    ) -> Result<(), Error> {
        nodes.clear();
        output.clear();
        output.extend(self.evaluate(input)?);
        Ok(())
    }
}

#[test]
fn relocation_adapter_preserves_results_refusals_and_recovery() {
    use alloc::vec;
    use zeno_fcis_synthesis::finite::{Domain, Op};

    let identity = Program::try_new(
        vec![Domain::Int { min: -1, max: 1 }],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![Op::Input(0)],
        vec![0],
    )
    .unwrap_or_else(|error| panic!("reference identity fixture: {error}"));
    let overflow = Program::try_new(
        vec![Domain::Bool],
        vec![Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        }],
        vec![
            Op::Input(0),
            Op::Int(1),
            Op::Int(0),
            Op::Select(0, 1, 2),
            Op::Int(i64::MAX),
            Op::Add(4, 3),
        ],
        vec![5],
    )
    .unwrap_or_else(|error| panic!("reference arithmetic fixture: {error}"));
    let mut nodes = vec![123];
    let mut output = vec![456];
    for (program, input, expected) in [
        (&identity, vec![1], Ok(vec![1])),
        (&identity, vec![], Err(Error::Invalid("input-domain"))),
        (&identity, vec![0], Ok(vec![0])),
        (&identity, vec![-1], Err(Error::Invalid("output-domain"))),
        (&overflow, vec![0], Ok(vec![i64::MAX])),
        (&overflow, vec![1], Err(Error::Arithmetic)),
        (&overflow, vec![0], Ok(vec![i64::MAX])),
    ] {
        assert_eq!(program.evaluate(&input), expected);
        let result = PreparationEvaluation::evaluate_into(program, &input, &mut nodes, &mut output);
        assert_eq!(result.clone().map(|()| output.clone()), expected);
        assert!(nodes.is_empty());
        if result.is_err() {
            assert!(output.is_empty());
        }
    }
}
