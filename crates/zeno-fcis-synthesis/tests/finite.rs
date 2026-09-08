//! Negative and generic-domain evidence for finite synthesis.
#![allow(clippy::unwrap_used)]

use zeno_fcis_synthesis::finite::{
    Budget, Contract, Domain, Error, Op, Outcome, Program, Sketch, Slot, synthesize,
};

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
