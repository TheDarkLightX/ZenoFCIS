//! Actual borrowed scalar program and its eager SMT encoding.
#[cfg(verus_keep_ghost)]
use super::bridge;
use super::{node, nodes, roots};
use crate::symbolic::Unsupported;
use crate::symbolic::smt::{Script, Term, Values};
#[cfg(verus_keep_ghost)]
use crate::term::denotation;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::{Domain, Op};

/// An admitted program's parts, as the library's `Program` and a bound
/// descriptor's `ScalarProgram` both hold them.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
#[cfg_attr(not(verus_keep_ghost), derive(Debug))]
pub(crate) struct Program<'a> {
    pub(crate) inputs: &'a [Domain],
    pub(crate) outputs: &'a [Domain],
    pub(crate) nodes: &'a [Op],
    pub(crate) roots: &'a [u16],
}

/// A program's terms over given input terms.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug))]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        forall|environment: Map<Seq<char>, denotation::Value>|
            match (#[trigger] denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                (Some(denotation::Value::Boolean(trap)), Some(denotation::Value::Boolean(admitted))) =>
                    denotation::term(result, environment) == Some(denotation::Value::Boolean(!trap && admitted)),
                _ => true,
            },
    ))]
    pub(crate) fn ok(&self) -> Term {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        let items = vec![Term::not(self.trap.clone()), self.admitted.clone()];
        #[cfg(verus_keep_ghost)]
        let arguments = verus_exec_expr!(Ghost(items@));
        let result = Term::and(items);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, denotation::Value>|
                match (#[trigger] denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                    (Some(denotation::Value::Boolean(trap)), Some(denotation::Value::Boolean(admitted))) =>
                        denotation::term(result, environment) == Some(denotation::Value::Boolean(!trap && admitted)),
                    _ => true,
                } by {
                if let (Some(denotation::Value::Boolean(_)), Some(denotation::Value::Boolean(_))) =
                    (denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                    denotation::conjunction_pair(arguments@, result, environment);
                }
            }
        }
        result
    }

    /// The evaluator fails with `OutputDomain`.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        forall|environment: Map<Seq<char>, denotation::Value>|
            match (#[trigger] denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                (Some(denotation::Value::Boolean(trap)), Some(denotation::Value::Boolean(admitted))) =>
                    denotation::term(result, environment) == Some(denotation::Value::Boolean(!trap && !admitted)),
                _ => true,
            },
    ))]
    pub(crate) fn out_of_domain(&self) -> Term {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        let items = vec![
            Term::not(self.trap.clone()),
            Term::not(self.admitted.clone()),
        ];
        #[cfg(verus_keep_ghost)]
        let arguments = verus_exec_expr!(Ghost(items@));
        let result = Term::and(items);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, denotation::Value>|
                match (#[trigger] denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                    (Some(denotation::Value::Boolean(trap)), Some(denotation::Value::Boolean(admitted))) =>
                        denotation::term(result, environment) == Some(denotation::Value::Boolean(!trap && !admitted)),
                    _ => true,
                } by {
                if let (Some(denotation::Value::Boolean(_)), Some(denotation::Value::Boolean(_))) =
                    (denotation::term(self.trap, environment), denotation::term(self.admitted, environment)) {
                    denotation::conjunction_pair(arguments@, result, environment);
                }
            }
        }
        result
    }
}

/// Defines every node of `program` in `script` as `{prefix}{index}`, with
/// `input[i]` the wire value of input `i`.
///
/// # Errors
/// An instruction or domain this encoding does not know, a node whose
/// operand kinds admission would have refused, or a supported input with no
/// supplied term.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    final(script).comments@ == old(script).comments@,
    final(script).constants@ == old(script).constants@,
    final(script).observations@ == old(script).observations@,
    final(script).assertions@ == old(script).assertions@,
    final(script).definitions@.len() >= old(script).definitions@.len(),
    final(script).definitions@.subrange(0, old(script).definitions@.len() as int)
        == old(script).definitions@,
    match result {
        Ok(encoded) => encoded.outputs@.len() == program.outputs@.len()
            && bridge::block(final(script).definitions@, old(script).definitions@.len(),
            prefix@, program.inputs@, program.nodes@, input@)
            && (forall|environment: Map<Seq<char>, denotation::Value>, raw: Seq<i64>|
                node::input_values(program.inputs@, input@, raw, environment)
                    && crate::script::spec::definitions_hold(final(script).definitions@,
                        old(script).definitions@.len(), program.nodes@.len(), environment) ==>
                bridge::outcome(program.inputs@, program.outputs@, program.nodes@,
                    program.roots@, raw, encoded.outputs@, encoded.trap, encoded.admitted, environment)),
        Err(_) => true,
    },
))]
pub(crate) fn encode(
    script: &mut Script,
    prefix: &str,
    program: Program<'_>,
    input: &[Term],
) -> Result<Encoded, Unsupported> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(<Term as View>::view); }
    #[cfg(verus_keep_ghost)]
    let base = verus_exec_expr!(Ghost(script.definitions@.len()));
    let nodes::EncodedNodes {
        kinds,
        terms,
        checked,
    } = match nodes::encode_nodes(script, prefix, program.inputs, program.nodes, input) {
        Ok(encoded) => encoded,
        Err(nodes::Failure { index, cause }) => {
            return Err(Unsupported::new(match cause {
                node::Failure::InputDomain(id) => {
                    format!("node {index} reads input {id}, which has no supported domain")
                }
                node::Failure::MissingInput(id) => {
                    format!("node {index} has no supplied term for input {id}")
                }
                node::Failure::Operand(id) => {
                    format!("node {index} reads node {id}, which is later or of another kind")
                }
                node::Failure::Instruction => {
                    let op = &program.nodes[index];
                    format!("node {index} is an instruction this encoding does not know: {op:?}")
                }
            }));
        }
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        bridge::record_block(script.definitions@, base@, prefix@, program.inputs@,
            program.nodes@, input@, kinds@, terms@);
    }
    let roots::EncodedRoots { outputs, admitted } =
        roots::encode_roots(program.outputs, program.roots, &terms, &kinds).map_err(|failure| {
            match failure {
                roots::Failure::Count => {
                    Unsupported::new("the program's roots and outputs differ in number")
                }
                roots::Failure::Missing { position, root } => Unsupported::new(format!(
                    "output {position} names node {root}, which does not exist"
                )),
                roots::Failure::Domain { position } => Unsupported::new(format!(
                    "output {position} has a domain this encoding does not know, or another kind"
                )),
            }
        })?;
    #[cfg(verus_keep_ghost)]
    let guards = verus_exec_expr!(Ghost((checked@, admitted@)));
    let trap = Term::or(checked);
    let admitted = Term::and(admitted);
    #[cfg(verus_keep_ghost)]
    proof! {
        broadcast use vstd::seq::group_seq_lemmas;
        assert(kinds@.subrange(0, program.nodes@.len() as int) =~= kinds@);
        assert(terms@.subrange(0, program.nodes@.len() as int) =~= terms@);
        assert(guards@.0.subrange(0, nodes::spec::guards(program.nodes@, program.nodes@.len()) as int)
            =~= guards@.0);
        assert forall|environment: Map<Seq<char>, denotation::Value>, raw: Seq<i64>|
            node::input_values(program.inputs@, input@, raw, environment)
                && crate::script::spec::definitions_hold(script.definitions@,
                    base@, program.nodes@.len(), environment) implies
            bridge::outcome(program.inputs@, program.outputs@, program.nodes@,
                program.roots@, raw, outputs@, trap, admitted, environment) by {
            assert(nodes::spec::meaning(program.nodes@, raw, program.nodes@.len(),
                input@, kinds@, terms@, guards@.0, environment));
            assert(node::typed_sources(Seq::empty(), terms@, kinds@, environment));
            assert(roots::spec::typed(outputs@, guards@.1, environment));
            denotation::disjunction_meaning(guards@.0, trap, environment);
            denotation::conjunction_meaning(guards@.1, admitted, environment);
            match crate::evaluation::spec::prefix(program.nodes@, raw, program.nodes@.len()) {
                Ok(values) => {
                    roots::spec::scalar_execution_after_prefix(program.inputs@, program.outputs@,
                        program.nodes@, program.roots@, raw, values, terms@, kinds@,
                        outputs@, guards@.1, admitted, environment);
                },
                Err(crate::evaluation::Failure::Arithmetic) => {},
                _ => {},
            }
        }
    }
    Ok(Encoded {
        outputs,
        trap,
        admitted,
    })
}

/// The values of an input domain.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    match domain {
        Domain::Bool => match result {
            Ok(Values::Range { min, max }) => min == 0 && max == 1, _ => false,
        },
        Domain::Int { min, max } => match result {
            Ok(Values::Range { min: lo, max: hi }) => lo == min as int && hi == max as int,
            _ => false,
        },
    },
))]
pub(crate) fn input_values(domain: Domain) -> Result<Values, Unsupported> {
    match domain {
        Domain::Bool => Ok(Values::Range { min: 0, max: 1 }),
        Domain::Int { min, max } => Ok(Values::Range {
            min: i128::from(min),
            max: i128::from(max),
        }),
        _ => Err(Unsupported::new(
            "an input domain this encoding does not know",
        )),
    }
}
