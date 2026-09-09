//! Negative and generic-domain evidence for finite synthesis.
#![allow(clippy::unwrap_used)]

use zeno_fcis_synthesis::finite::emit::{
    JavaScriptEmitter, PythonEmitter, RustEmitter, TargetEmitter,
};
use zeno_fcis_synthesis::finite::{
    Budget, Case, Contract, Domain, Error, Op, Outcome, Program, Sketch, Slot, Witness, synthesize,
};

/// Straightforward reference product in the same input-major lexicographic
/// order the space iterator promises.
fn product(domains: &[Domain]) -> Vec<Vec<i64>> {
    let mut rows = vec![Vec::new()];
    for domain in domains {
        let (min, max) = domain.bounds();
        let mut next = Vec::new();
        for row in &rows {
            let mut value = min;
            loop {
                let mut extended = row.clone();
                extended.push(value);
                next.push(extended);
                if value == max {
                    break;
                }
                value += 1;
            }
        }
        rows = next;
    }
    rows
}

/// The straightforward evaluator this profile shipped with, kept as an
/// independent reference for the buffer-reusing interpreter.
fn reference_evaluate(program: &Program, input: &[i64]) -> Result<Vec<i64>, Error> {
    fn admitted(domains: &[Domain], values: &[i64]) -> bool {
        domains.len() == values.len()
            && domains
                .iter()
                .zip(values)
                .all(|(domain, value)| domain.contains(*value))
    }
    if !admitted(program.inputs(), input) {
        return Err(Error::Invalid("input-domain"));
    }
    let mut values: Vec<i64> = Vec::new();
    for op in program.nodes() {
        let at = |id: u16| values[usize::from(id)];
        let value = match *op {
            Op::Input(id) => input[usize::from(id)],
            Op::Int(v) => v,
            Op::Bool(v) => i64::from(v),
            Op::Add(a, b) => at(a).checked_add(at(b)).ok_or(Error::Arithmetic)?,
            Op::Sub(a, b) => at(a).checked_sub(at(b)).ok_or(Error::Arithmetic)?,
            Op::Eq(a, b) => i64::from(at(a) == at(b)),
            Op::Lt(a, b) => i64::from(at(a) < at(b)),
            Op::And(a, b) => i64::from(at(a) == 1 && at(b) == 1),
            Op::Not(a) => i64::from(at(a) == 0),
            Op::Select(c, a, b) => {
                if at(c) == 1 {
                    at(a)
                } else {
                    at(b)
                }
            }
        };
        values.push(value);
    }
    let output: Vec<i64> = program
        .roots()
        .iter()
        .map(|id| values[usize::from(*id)])
        .collect();
    if !admitted(program.outputs(), &output) {
        return Err(Error::Invalid("output-domain"));
    }
    Ok(output)
}

fn trivial_contract(inputs: Vec<Domain>) -> Contract {
    let outputs = vec![Domain::Bool];
    let relation = Program::try_new(
        [inputs.clone(), outputs.clone()].concat(),
        vec![Domain::Bool],
        vec![Op::Bool(true)],
        vec![0],
    )
    .unwrap();
    Contract::try_new(inputs, outputs, relation).unwrap()
}

/// A relation over one input and one output field that holds exactly at
/// `satisfied` and traps exactly at `trap`.
///
/// Every node is evaluated, so the trapping addition fires regardless of which
/// nodes the root reads.
fn indicator_relation(
    environment: Vec<Domain>,
    trap: (i64, i64),
    satisfied: (i64, i64),
) -> Program {
    Program::try_new(
        environment,
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(trap.0),
            Op::Int(trap.1),
            Op::Eq(0, 2),
            Op::Eq(1, 3),
            Op::And(4, 5),
            Op::Int(1),
            Op::Int(0),
            Op::Select(6, 7, 8),
            Op::Int(i64::MAX),
            Op::Add(10, 9),
            Op::Int(satisfied.0),
            Op::Int(satisfied.1),
            Op::Eq(0, 12),
            Op::Eq(1, 13),
            Op::And(14, 15),
        ],
        vec![16],
    )
    .unwrap()
}

fn one_field_sketch(inputs: Vec<Domain>, outputs: Vec<Domain>) -> Sketch {
    Sketch::try_new(
        inputs,
        outputs,
        vec![Slot::Choice {
            id: 1,
            alternatives: vec![Op::Input(0)],
        }],
        vec![0],
    )
    .unwrap()
}

fn tiny() -> (Contract, Sketch) {
    // The relational contract requires output = input + 1, independently of
    // the implementation choice. An identity candidate must be refuted.
    let inputs = vec![Domain::Int { min: 0, max: 2 }];
    let outputs = vec![Domain::Int { min: 1, max: 3 }];
    let relation = Program::try_new(
        [inputs.clone(), outputs.clone()].concat(),
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(1),
            Op::Add(0, 2),
            Op::Eq(1, 3),
        ],
        vec![4],
    )
    .unwrap();
    let contract = Contract::try_new(inputs.clone(), outputs.clone(), relation).unwrap();
    let sketch = Sketch::try_new(
        inputs,
        outputs,
        vec![
            Slot::Fixed(Op::Input(0)),
            Slot::Fixed(Op::Int(1)),
            Slot::Choice {
                id: 1,
                alternatives: vec![Op::Input(0), Op::Add(0, 1)],
            },
        ],
        vec![2],
    )
    .unwrap();
    (contract, sketch)
}

#[test]
fn the_evaluator_agrees_with_the_straightforward_reference_on_every_input() {
    let full = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let programs = [
        // Every operation, including a Boolean result and an eager Select.
        Program::try_new(
            vec![Domain::Int { min: -2, max: 2 }, Domain::Bool],
            vec![Domain::Int { min: -4, max: 4 }, Domain::Bool],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Int(2),
                Op::Bool(true),
                Op::Add(0, 2),
                Op::Sub(0, 2),
                Op::Eq(0, 2),
                Op::Lt(0, 2),
                Op::And(1, 3),
                Op::Not(8),
                Op::Select(9, 4, 5),
            ],
            vec![10, 9],
        )
        .unwrap(),
        // Narrow output domain: some inputs leave the declared range.
        Program::try_new(
            vec![Domain::Int { min: -2, max: 2 }],
            vec![Domain::Int { min: 0, max: 1 }],
            vec![Op::Input(0), Op::Int(2), Op::Add(0, 1)],
            vec![2],
        )
        .unwrap(),
        // Checked arithmetic traps for one of the two admitted inputs.
        Program::try_new(
            vec![Domain::Int { min: 0, max: 1 }],
            vec![full],
            vec![Op::Input(0), Op::Int(i64::MAX), Op::Add(0, 1)],
            vec![2],
        )
        .unwrap(),
        // A dead trapping node is still evaluated.
        Program::try_new(
            vec![],
            vec![Domain::Bool],
            vec![
                Op::Int(i64::MAX),
                Op::Int(1),
                Op::Add(0, 1),
                Op::Bool(false),
            ],
            vec![3],
        )
        .unwrap(),
    ];
    for program in &programs {
        let mut inputs = product(program.inputs());
        inputs.push(Vec::new());
        inputs.push(vec![0, 0, 0]);
        inputs.push(vec![i64::MAX]);
        for input in inputs {
            assert_eq!(
                program.evaluate(&input),
                reference_evaluate(program, &input),
                "program {program:?} input {input:?}"
            );
        }
    }
}

#[test]
fn the_input_space_matches_a_straightforward_product_and_clones_independently() {
    for domains in [
        vec![],
        vec![Domain::Bool],
        vec![Domain::Int { min: -1, max: 1 }, Domain::Bool],
        vec![Domain::Int {
            min: i64::MAX - 1,
            max: i64::MAX,
        }],
        vec![
            Domain::Int {
                min: i64::MIN,
                max: i64::MIN + 1,
            },
            Domain::Int {
                min: i64::MAX - 1,
                max: i64::MAX,
            },
        ],
    ] {
        let expected = product(&domains);
        let contract = trivial_contract(domains);
        let space = contract.input_space().unwrap();
        assert_eq!(space.cardinality(), u64::try_from(expected.len()).unwrap());
        assert_eq!(space.collect::<Vec<_>>(), expected);

        let mut original = contract.input_space().unwrap();
        assert_eq!(original.next(), expected.first().cloned());
        let mut forked = original.clone();
        let remaining: Vec<_> = original.by_ref().collect();
        assert_eq!(remaining, expected[1..].to_vec());
        assert_eq!(forked.by_ref().collect::<Vec<_>>(), remaining);
        // An exhausted space stays exhausted, and each clone exhausts alone.
        assert_eq!(original.next(), None);
        assert_eq!(forked.next(), None);
    }
}

#[test]
fn realizability_reports_a_trap_after_an_earlier_satisfying_output() {
    let field = Domain::Int { min: 0, max: 2 };
    let contract = Contract::try_new(
        vec![field],
        vec![field],
        indicator_relation(vec![field, field], (0, 1), (0, 0)),
    )
    .unwrap();
    let sketch = one_field_sketch(vec![field], vec![field]);

    assert_eq!(
        synthesize(&contract, &sketch, Budget::default()),
        Err(Error::ContractTrap {
            input: vec![0],
            output: vec![1],
        })
    );
}

#[test]
fn a_trap_inside_a_row_precedes_that_rows_unrealizability() {
    let field = Domain::Int { min: 0, max: 2 };
    let contract = Contract::try_new(
        vec![field],
        vec![field],
        indicator_relation(vec![field, field], (1, 2), (0, 0)),
    )
    .unwrap();
    let sketch = one_field_sketch(vec![field], vec![field]);

    // Input one satisfies nothing and traps at output two; the trap is the
    // reported failure and input two is never reached.
    assert_eq!(
        synthesize(&contract, &sketch, Budget::default()),
        Err(Error::ContractTrap {
            input: vec![1],
            output: vec![2],
        })
    );
}

#[test]
fn an_earlier_unrealizable_input_precedes_a_later_trap() {
    let field = Domain::Int { min: 0, max: 2 };
    let contract = Contract::try_new(
        vec![field],
        vec![field],
        indicator_relation(vec![field, field], (2, 0), (1, 0)),
    )
    .unwrap();
    let sketch = one_field_sketch(vec![field], vec![field]);

    assert_eq!(
        synthesize(&contract, &sketch, Budget::default()).unwrap(),
        Outcome::Unrealizable { input: vec![0] }
    );
}

#[test]
fn a_trapped_candidate_does_not_disturb_the_next_candidate() {
    let (contract, _) = tiny();
    // The first canonical alternative leaves the declared output domain at
    // every input; the second is the correct implementation.
    let sketch = Sketch::try_new(
        contract.inputs().to_vec(),
        contract.outputs().to_vec(),
        vec![
            Slot::Fixed(Op::Input(0)),
            Slot::Fixed(Op::Int(i64::MAX)),
            Slot::Fixed(Op::Int(1)),
            Slot::Choice {
                id: 1,
                alternatives: vec![Op::Add(0, 1), Op::Add(0, 2)],
            },
        ],
        vec![3],
    )
    .unwrap();

    let Outcome::Selected {
        cases,
        first_counterexample,
        certificate,
        ..
    } = synthesize(&contract, &sketch, Budget::default()).unwrap()
    else {
        panic!("expected selection")
    };
    assert_eq!(certificate.evaluated(), 2);
    assert_eq!(
        first_counterexample,
        Some(Witness {
            input: vec![0],
            output: None,
        })
    );
    assert_eq!(
        cases,
        vec![
            Case {
                input: vec![0],
                output: vec![1],
            },
            Case {
                input: vec![1],
                output: vec![2],
            },
            Case {
                input: vec![2],
                output: vec![3],
            },
        ]
    );

    // A rejected pair leaves nothing behind for the next admitted pair.
    assert!(contract.holds(&[9], &[1]).is_err());
    assert!(contract.holds(&[0], &[9]).is_err());
    assert_eq!(contract.holds(&[0], &[1]), Ok(true));
    assert_eq!(contract.holds(&[0], &[2]), Ok(false));
}

#[test]
fn synthesizes_a_generic_relation_and_refutes_identity() {
    let (contract, sketch) = tiny();
    let Outcome::Selected {
        program,
        certificate,
        ..
    } = synthesize(&contract, &sketch, Budget::default()).unwrap()
    else {
        panic!("expected selection")
    };
    assert_eq!(program.evaluate(&[2]).unwrap(), vec![3]);
    assert_eq!(certificate.evaluated(), 2);
    assert_eq!(certificate.counterexamples().len(), 1);
    assert!(program.evaluate(&[3]).is_err());
    assert_eq!(
        synthesize(&contract, &sketch, Budget::default()).unwrap(),
        synthesize(&contract, &sketch, Budget::default()).unwrap()
    );
}

#[test]
fn distinguishes_unrealizable_contract_from_insufficient_grammar() {
    let (contract, _) = tiny();
    let sketch = Sketch::try_new(
        contract.inputs().to_vec(),
        contract.outputs().to_vec(),
        vec![Slot::Choice {
            id: 1,
            alternatives: vec![Op::Input(0)],
        }],
        vec![0],
    )
    .unwrap();
    assert!(matches!(
        synthesize(&contract, &sketch, Budget::default()).unwrap(),
        Outcome::NoSolution { .. }
    ));
    let impossible = Program::try_new(
        vec![
            Domain::Int { min: 0, max: 2 },
            Domain::Int { min: 1, max: 3 },
        ],
        vec![Domain::Bool],
        vec![Op::Bool(false)],
        vec![0],
    )
    .unwrap();
    let impossible = Contract::try_new(
        contract.inputs().to_vec(),
        contract.outputs().to_vec(),
        impossible,
    )
    .unwrap();
    assert!(
        matches!(synthesize(&impossible, &sketch, Budget::default()).unwrap(), Outcome::Unrealizable { input } if input == vec![0])
    );
}

#[test]
fn checks_budgets_and_closed_types_before_search() {
    let (contract, sketch) = tiny();
    assert!(matches!(
        synthesize(
            &contract,
            &sketch,
            Budget {
                max_assignments: 1,
                ..Budget::default()
            }
        ),
        Err(Error::Budget { .. })
    ));
    assert!(matches!(
        synthesize(
            &contract,
            &sketch,
            Budget {
                max_steps: 1,
                ..Budget::default()
            }
        ),
        Err(Error::Budget { .. })
    ));
    assert!(Program::try_new(vec![], vec![Domain::Bool], vec![Op::Not(0)], vec![0]).is_err());
    assert!(
        Program::try_new(
            vec![],
            vec![Domain::Bool],
            vec![Op::Int(1), Op::Not(0)],
            vec![1]
        )
        .is_err()
    );
    assert!(
        Sketch::try_new(
            vec![],
            vec![Domain::Bool],
            vec![Slot::Choice {
                id: 1,
                alternatives: vec![Op::Bool(true), Op::Int(1)]
            }],
            vec![0]
        )
        .is_err()
    );
}

#[test]
fn arithmetic_and_contract_traps_never_become_evidence() {
    let program = Program::try_new(
        vec![],
        vec![Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        }],
        vec![Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap();
    assert_eq!(program.evaluate(&[]), Err(Error::Arithmetic));
    let (contract, sketch) = tiny();
    let relation = Program::try_new(
        [contract.inputs(), contract.outputs()].concat(),
        vec![Domain::Bool],
        vec![Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1), Op::Eq(2, 2)],
        vec![3],
    )
    .unwrap();
    let broken = Contract::try_new(
        contract.inputs().to_vec(),
        contract.outputs().to_vec(),
        relation,
    )
    .unwrap();
    assert!(matches!(
        synthesize(&broken, &sketch, Budget::default()),
        Err(Error::ContractTrap { .. })
    ));
}

#[test]
fn budget_includes_selected_replay_and_contract_identity_includes_partition() {
    let relation = Program::try_new(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Bool(true)],
        vec![0],
    )
    .unwrap();
    let contract = Contract::try_new(vec![], vec![Domain::Bool], relation).unwrap();
    let sketch = Sketch::try_new(
        vec![],
        vec![Domain::Bool],
        vec![Slot::Choice {
            id: 1,
            alternatives: vec![Op::Bool(true)],
        }],
        vec![0],
    )
    .unwrap();
    assert!(matches!(
        synthesize(
            &contract,
            &sketch,
            Budget {
                max_assignments: 1,
                max_steps: 4
            }
        ),
        Err(Error::Budget { required: 5, .. })
    ));
    let relation = Program::try_new(
        vec![Domain::Bool; 3],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Input(1), Op::Eq(0, 1)],
        vec![2],
    )
    .unwrap();
    let one =
        Contract::try_new(vec![Domain::Bool], vec![Domain::Bool; 2], relation.clone()).unwrap();
    let two = Contract::try_new(vec![Domain::Bool; 2], vec![Domain::Bool], relation).unwrap();
    assert_ne!(one.commitment().unwrap(), two.commitment().unwrap());
}

#[test]
fn language_adapters_keep_distinct_identities_extensions_and_stated_abis() {
    let javascript = JavaScriptEmitter.target();
    assert_eq!(javascript.language, "javascript");
    assert_eq!(javascript.extension, "mjs");
    assert_eq!(javascript.revision, 1);
    assert!(JavaScriptEmitter.abi().contains("primitive string"));
    // A copied adapter that kept another emitter's identity, extension, or ABI
    // would let one target's conformance evidence stand in for another's.
    let adapters: [&dyn TargetEmitter; 3] = [&RustEmitter, &PythonEmitter, &JavaScriptEmitter];
    for (position, left) in adapters.iter().enumerate() {
        let identity = left.identity();
        assert_eq!(identity, left.identity(), "identity must be stable");
        assert!(!left.abi().is_empty());
        for right in &adapters[position + 1..] {
            assert_ne!(left.identity(), right.identity());
            assert_ne!(left.target().language, right.target().language);
            assert_ne!(left.target().extension, right.target().extension);
            assert_ne!(left.abi(), right.abi());
        }
    }
}

#[test]
fn unselected_arithmetic_still_traps_and_hole_order_is_canonical() {
    let program = Program::try_new(
        vec![],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Bool(false),
            Op::Select(3, 2, 1),
        ],
        vec![4],
    )
    .unwrap();
    assert_eq!(program.evaluate(&[]), Err(Error::Arithmetic));
    let (contract, sketch) = tiny();
    let reversed = Sketch::try_new(
        contract.inputs().to_vec(),
        contract.outputs().to_vec(),
        vec![
            Slot::Fixed(Op::Input(0)),
            Slot::Fixed(Op::Int(1)),
            Slot::Choice {
                id: 1,
                alternatives: vec![Op::Add(0, 1), Op::Input(0)],
            },
        ],
        vec![2],
    )
    .unwrap();
    assert_eq!(
        synthesize(&contract, &sketch, Budget::default()).unwrap(),
        synthesize(&contract, &reversed, Budget::default()).unwrap()
    );
    assert!(
        Sketch::try_new(
            vec![],
            vec![Domain::Bool],
            vec![Slot::Choice {
                id: 1,
                alternatives: vec![Op::Bool(true); 1025]
            }],
            vec![0]
        )
        .is_err()
    );
}
