//! The actual ordered root/output loop, with the caller's refusals as data.

#[cfg(verus_keep_ghost)]
use super::node;
use super::node::Kind;
use crate::symbolic::smt::Term;
#[cfg(verus_keep_ghost)]
use crate::term::denotation;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::Domain;

#[cfg(verus_keep_ghost)]
#[path = "roots_spec.rs"]
pub(super) mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct EncodedRoots {
    pub(super) outputs: Vec<Term>,
    pub(super) admitted: Vec<Term>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Failure {
    Count,
    Missing { position: usize, root: usize },
    Domain { position: usize },
}

/// The arguments are unrestricted, including unequal term/kind lengths.
/// A canonical prefix supplies the separate conditional runtime relation.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result.is_ok() == spec::acceptable(domains@, roots@, terms@, kinds@),
    match result {
        Ok(encoded) => spec::facts(domains@, roots@, terms@, kinds@,
                roots@.len(), encoded.outputs@, encoded.admitted@)
            && (forall|environment: Map<Seq<char>, denotation::Value>|
                node::typed_sources(Seq::empty(), terms@, kinds@, environment) ==>
                    spec::typed(encoded.outputs@, encoded.admitted@, environment))
            && (forall|environment: Map<Seq<char>, denotation::Value>, values: Seq<i128>|
                node::prefix_values(kinds@, terms@, values, environment) ==>
                    spec::meaning(domains@, roots@, encoded.outputs@, encoded.admitted@,
                        values, environment)),
        Err(failure) => spec::refusal(domains@, roots@, terms@, kinds@, failure),
    },
))]
pub(super) fn encode_roots(
    domains: &[Domain],
    roots: &[u16],
    terms: &[Term],
    kinds: &[Kind],
) -> Result<EncodedRoots, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(<Term as View>::view);
        broadcast use vstd::seq::group_seq_lemmas;
        reveal_with_fuel(spec::guards, 1);
    }
    if roots.len() != domains.len() {
        return Err(Failure::Count);
    }
    let mut outputs = Vec::with_capacity(roots.len());
    let mut admitted = Vec::with_capacity(roots.len());
    let mut position = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant position <= roots@.len(), roots@.len() == domains@.len(),
            spec::facts(domains@, roots@, terms@, kinds@,
                position as nat, outputs@, admitted@),
        decreases roots@.len() - position,
    ))]
    while position < roots.len() {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        // The preceding count check makes this the original zip/enumerate order.
        let root = usize::from(roots[position]);
        let domain = &domains[position];
        let (Some(term), Some(kind)) = (terms.get(root), kinds.get(root)) else {
            #[cfg(verus_keep_ghost)]
            proof! {
                if spec::acceptable(domains@, roots@, terms@, kinds@) {
                    assert(spec::ready(domains@, roots@, terms@, kinds@, position as int));
                    assert(false);
                }
            }
            return Err(Failure::Missing { position, root });
        };
        #[cfg(verus_keep_ghost)]
        let previous = verus_exec_expr!(Ghost((outputs@, admitted@)));
        match (domain, kind) {
            (Domain::Bool, Kind::Bool) => {
                let output = Term::ite(term.clone(), Term::Int(1), Term::Int(0));
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert forall|environment: Map<Seq<char>, denotation::Value>|
                        denotation::term(output, environment) == match denotation::term(*term, environment) {
                            Some(denotation::Value::Boolean(value)) =>
                                Some(denotation::Value::Integer(if value { 1int } else { 0int })),
                            _ => None,
                        } by {
                        reveal_with_fuel(denotation::term, 1);
                    }
                    assert(spec::output_fact(*term, output, *kind));
                }
                outputs.push(output);
            }
            (Domain::Int { min, max }, Kind::Int) => {
                let guard = Term::within(term, *min, *max);
                #[cfg(verus_keep_ghost)]
                proof! { assert(spec::guard_fact(*domain, *term, guard)); }
                admitted.push(guard);
                let output = term.clone();
                #[cfg(verus_keep_ghost)]
                proof! { assert(spec::output_fact(*term, output, *kind)); }
                outputs.push(output);
            }
            _ => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    if spec::acceptable(domains@, roots@, terms@, kinds@) {
                        assert(spec::ready(domains@, roots@, terms@, kinds@, position as int));
                        assert(false);
                    }
                }
                return Err(Failure::Domain { position });
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
            spec::guards_step(domains@, position as nat);
            assert(spec::ready(domains@, roots@, terms@, kinds@, position as int));
            assert forall|j: int| 0 <= j < position + 1 implies
                spec::ready(domains@, roots@, terms@, kinds@, j)
                && spec::output_fact(terms@[roots@[j] as int], outputs@[j], kinds@[roots@[j] as int]) by {
                if j < position {
                    assert(outputs@[j] == previous@.0[j]);
                } else {
                    assert(j == position);
                }
            }
            assert forall|j: int| 0 <= j < position + 1 && spec::integer(domains@[j]) implies
                spec::guards(domains@, j as nat) < admitted@.len()
                && spec::guard_fact(domains@[j], terms@[roots@[j] as int],
                    admitted@[spec::guards(domains@, j as nat) as int]) by {
                if j < position {
                    assert(admitted@.subrange(0, previous@.1.len() as int) == previous@.1);
                } else {
                    assert(j == position);
                    assert(spec::guards(domains@, position as nat) == previous@.1.len());
                }
            }
            assert(spec::facts(domains@, roots@, terms@, kinds@,
                (position + 1) as nat, outputs@, admitted@));
        }
        position += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, denotation::Value>|
            node::typed_sources(Seq::empty(), terms@, kinds@, environment) implies
                spec::typed(outputs@, admitted@, environment) by {
            spec::typed_refines(domains@, roots@, terms@, kinds@,
                outputs@, admitted@, environment);
        }
        assert forall|environment: Map<Seq<char>, denotation::Value>, values: Seq<i128>|
            node::prefix_values(kinds@, terms@, values, environment) implies
                spec::meaning(domains@, roots@, outputs@, admitted@, values, environment) by {
            spec::canonical_prefix(domains@, roots@, terms@, kinds@,
                outputs@, admitted@, values, environment, roots@.len());
            assert(domains@.subrange(0, roots@.len() as int) =~= domains@);
            assert(outputs@.subrange(0, roots@.len() as int) =~= outputs@);
        }
    }
    Ok(EncodedRoots { outputs, admitted })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use zeno_fcis_synthesis::finite::{self, Op, V2ExecutionFailure, V2Resource};

    use super::super::{Program, encode, nodes};
    use super::*;
    use crate::symbolic::smt::{Script, Sort, Values};

    #[test]
    fn ordered_repeated_mixed_roots_match_actual_runtime_wires() {
        let integer = Domain::Int { min: 0, max: 1 };
        let seven = Domain::Int { min: 7, max: 7 };
        let input_domains = [Domain::Bool, integer];
        let output_domains = [
            Domain::Bool,
            integer,
            Domain::Bool,
            integer,
            seven,
            Domain::Bool,
        ];
        let operations = [Op::Input(0), Op::Input(1), Op::Not(0), Op::Int(7)];
        let roots = [2, 1, 0, 1, 3, 2];
        let mut script = Script::default();
        let input = [
            script.constant("x0", Values::Range { min: 0, max: 1 }),
            script.constant("x1", Values::Range { min: 0, max: 1 }),
        ];
        let prepared = nodes::encode_nodes(&mut script, "p", &input_domains, &operations, &input)
            .unwrap_or_else(|_| panic!("the mixed fixture's actual nodes must encode"));
        let encoded = encode_roots(&output_domains, &roots, &prepared.terms, &prepared.kinds)
            .unwrap_or_else(|error| panic!("the mixed roots must encode: {error:?}"));
        assert_eq!(encoded.outputs.len(), 6);
        assert_eq!(encoded.admitted.len(), 3);
        for (position, output) in encoded.outputs.iter().enumerate() {
            script.define(&format!("root_out{position}"), Sort::Int, output.clone());
        }
        script.define("root_admitted", Sort::Bool, Term::and(encoded.admitted));

        // Opposite Bool/Int wires distinguish kind conversion from root position.
        for (raw_input, expected) in [
            ([0i128, 1i128], [1i128, 1, 0, 1, 7, 1]),
            ([1i128, 0i128], [0i128, 0, 1, 0, 7, 0]),
        ] {
            let assignment = BTreeMap::from([
                ("x0".to_owned(), raw_input[0]),
                ("x1".to_owned(), raw_input[1]),
            ]);
            let evaluation = script
                .evaluate(&assignment)
                .unwrap_or_else(|| panic!("the concrete inputs are admitted"));
            let observed: Vec<Option<i128>> = (0..roots.len())
                .map(|position| evaluation.int(&format!("root_out{position}")))
                .collect();
            assert_eq!(observed, expected.map(Some).to_vec());
            assert_eq!(evaluation.boolean("root_admitted"), Some(true));
            let (actual, _) = finite::execute_v2(
                &input_domains,
                &output_domains,
                &operations,
                &roots,
                &raw_input,
                finite::v2_zero_limits().with_limit(V2Resource::Step, 4),
            )
            .into_parts();
            assert_eq!(actual, Ok(expected.to_vec()));
        }
    }

    #[test]
    fn count_and_first_root_refusals_keep_exact_data_and_caller_diagnostics() {
        let integer = Domain::Int { min: 0, max: 3 };
        let operations = [Op::Int(4), Op::Bool(true)];
        let mut prepared_script = Script::default();
        let prepared = nodes::encode_nodes(&mut prepared_script, "p", &[], &operations, &[])
            .unwrap_or_else(|_| panic!("the refusal fixture's actual nodes must encode"));
        let cases = [
            (
                vec![Domain::Bool],
                vec![0, u16::MAX],
                Failure::Count,
                "the program's roots and outputs differ in number",
            ),
            (
                vec![integer, Domain::Bool, integer],
                vec![0, 0, u16::MAX],
                Failure::Domain { position: 1 },
                "output 1 has a domain this encoding does not know, or another kind",
            ),
            (
                vec![integer, Domain::Bool, Domain::Bool],
                vec![0, u16::MAX, 0],
                Failure::Missing {
                    position: 1,
                    root: usize::from(u16::MAX),
                },
                "output 1 names node 65535, which does not exist",
            ),
        ];
        for (domains, roots, refusal, diagnostic) in cases {
            assert_eq!(
                encode_roots(&domains, &roots, &prepared.terms, &prepared.kinds).err(),
                Some(refusal),
            );
            let mut script = Script::default();
            let unsupported = encode(
                &mut script,
                "p",
                Program {
                    inputs: &[],
                    outputs: &domains,
                    nodes: &operations,
                    roots: &roots,
                },
                &[],
            )
            .err()
            .unwrap_or_else(|| panic!("the caller must retain the helper's refusal"));
            assert_eq!(unsupported.reason(), diagnostic);
            let source = script.source();
            assert_eq!(source, prepared_script.source());
            let definitions: Vec<&str> = source
                .lines()
                .filter(|line| line.starts_with("(define-fun "))
                .collect();
            assert_eq!(
                definitions,
                ["(define-fun p0 () Int 4)", "(define-fun p1 () Bool true)",]
            );
        }

        // Both get failures are Missing, even when the other vector has the root.
        assert_eq!(
            encode_roots(&[Domain::Bool], &[1], &prepared.terms, &prepared.kinds[..1]).err(),
            Some(Failure::Missing {
                position: 0,
                root: 1
            }),
        );
        assert_eq!(
            encode_roots(&[Domain::Bool], &[1], &prepared.terms[..1], &prepared.kinds).err(),
            Some(Failure::Missing {
                position: 0,
                root: 1
            }),
        );
    }

    #[test]
    fn empty_roots_and_reversed_bounds_keep_actual_runtime_outcomes() {
        let operations = [Op::Int(2)];
        let mut script = Script::default();
        let prepared = nodes::encode_nodes(&mut script, "p", &[], &operations, &[])
            .unwrap_or_else(|_| panic!("the literal node must encode"));
        // Empty roots need neither terms nor kinds; unequal supplied lengths are legal.
        let empty = encode_roots(&[], &[], &prepared.terms, &[])
            .unwrap_or_else(|error| panic!("empty roots must encode: {error:?}"));
        assert!(empty.outputs.is_empty());
        assert!(empty.admitted.is_empty());
        script.define("empty_admitted", Sort::Bool, Term::and(empty.admitted));
        let evaluation = script
            .evaluate(&BTreeMap::new())
            .unwrap_or_else(|| panic!("the literal script has no missing inputs"));
        assert_eq!(evaluation.boolean("empty_admitted"), Some(true));
        let (actual, _) = finite::execute_v2(
            &[],
            &[],
            &operations,
            &[],
            &[],
            finite::v2_zero_limits().with_limit(V2Resource::Step, 1),
        )
        .into_parts();
        assert_eq!(actual, Ok(Vec::new()));

        // Empty output projection must still retain an unused eager overflow.
        let overflow = [Op::Int(i128::MAX), Op::Int(1), Op::Add(0, 1)];
        let mut overflowing = Script::default();
        let encoded = encode(
            &mut overflowing,
            "p",
            Program {
                inputs: &[],
                outputs: &[],
                nodes: &overflow,
                roots: &[],
            },
            &[],
        )
        .unwrap_or_else(|error| panic!("an arithmetic trap is encoded, not refused: {error:?}"));
        assert!(encoded.outputs.is_empty());
        overflowing.define("empty_trap", Sort::Bool, encoded.trap);
        overflowing.define("empty_admitted", Sort::Bool, encoded.admitted);
        let evaluation = overflowing
            .evaluate(&BTreeMap::new())
            .unwrap_or_else(|| panic!("the literal script has no missing inputs"));
        assert_eq!(evaluation.boolean("empty_trap"), Some(true));
        assert_eq!(evaluation.boolean("empty_admitted"), Some(true));
        let (actual, _) = finite::execute_v2(
            &[],
            &[],
            &overflow,
            &[],
            &[],
            finite::v2_zero_limits().with_limit(V2Resource::Step, 3),
        )
        .into_parts();
        assert_eq!(actual, Err(V2ExecutionFailure::Arithmetic));

        // Raw execution accepts slices: static library admission is a separate gate.
        let reversed = Domain::Int { min: 3, max: 1 };
        let encoded = encode_roots(&[reversed], &[0], &prepared.terms, &prepared.kinds)
            .unwrap_or_else(|error| panic!("reversed bounds produce a guard: {error:?}"));
        assert_eq!(encoded.outputs.len(), 1);
        assert_eq!(encoded.admitted.len(), 1);
        script.define("reversed_out", Sort::Int, encoded.outputs[0].clone());
        script.define("reversed_admitted", Sort::Bool, Term::and(encoded.admitted));
        let evaluation = script
            .evaluate(&BTreeMap::new())
            .unwrap_or_else(|| panic!("the literal script has no missing inputs"));
        assert_eq!(evaluation.int("reversed_out"), Some(2));
        assert_eq!(evaluation.boolean("reversed_admitted"), Some(false));
        let (actual, _) = finite::execute_v2(
            &[],
            &[reversed],
            &operations,
            &[0],
            &[],
            finite::v2_zero_limits().with_limit(V2Resource::Step, 1),
        )
        .into_parts();
        assert_eq!(actual, Err(V2ExecutionFailure::OutputDomain));
        assert_eq!(
            encode_roots(&[reversed], &[], &prepared.terms, &prepared.kinds).err(),
            Some(Failure::Count),
        );
    }
}
