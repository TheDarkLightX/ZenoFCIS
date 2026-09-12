use super::*;
use crate::{assignment_at, increment_indexes};

fn checked<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("{error:?}"))
}

fn mixed_choices(order: u8) -> Sketch {
    let mut integer = vec![
        Op::Input(0),
        Op::Int(i64::MIN),
        Op::Int(-1),
        Op::Int(0),
        Op::Int(i64::MAX),
        Op::Add(0, 2),
        Op::Sub(0, 2),
        Op::Select(1, 0, 2),
        Op::Select(3, 2, 0),
    ];
    let mut boolean = vec![
        Op::Input(1),
        Op::Bool(false),
        Op::Bool(true),
        Op::Eq(0, 2),
        Op::Eq(1, 3),
        Op::Lt(0, 2),
        Op::And(1, 3),
        Op::Not(1),
        Op::Select(1, 1, 3),
    ];
    if order != 0 {
        integer.reverse();
        boolean.reverse();
    }
    let a = Slot::Choice {
        id: 91,
        alternatives: integer,
    };
    let b = Slot::Choice {
        id: 7,
        alternatives: boolean,
    };
    let mut nodes = vec![
        Slot::Fixed(Op::Input(0)),
        Slot::Fixed(Op::Input(1)),
        Slot::Fixed(Op::Int(2)),
        Slot::Fixed(Op::Bool(true)),
    ];
    let fixed = Slot::Fixed(Op::Int(42));
    let roots = if order == 2 {
        nodes.extend([b, fixed, a]);
        vec![6, 4]
    } else {
        nodes.extend([a, fixed, b]);
        vec![4, 6]
    };
    let integer = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    checked(Sketch::try_new(
        vec![integer, Domain::Bool],
        vec![integer, Domain::Bool],
        nodes,
        roots,
    ))
}

fn problems() -> Vec<Sketch> {
    let mut sketches: Vec<_> = (0..3).map(mixed_choices).collect();
    sketches.push(checked(Sketch::try_new(
        vec![],
        vec![Domain::Int {
            min: -512,
            max: 511,
        }],
        vec![Slot::Choice {
            id: u32::MAX,
            alternatives: (-512..512).rev().map(Op::Int).collect(),
        }],
        vec![0],
    )));
    sketches.push(checked(Sketch::try_new(
        vec![],
        vec![Domain::Int { min: 0, max: 0 }],
        (1..=64)
            .rev()
            .map(|id| Slot::Choice {
                id,
                alternatives: vec![Op::Int(0)],
            })
            .collect(),
        vec![63],
    )));
    sketches
}

fn problem(sketch: &Sketch) -> SynthesisProblem {
    let hash = Hash32::new([1; 32]);
    checked(SynthesisProblem::try_new(
        SynthesisBindings {
            schema_hash: hash,
            contract_hash: hash,
            grammar_hash: hash,
            algorithm_hash: hash,
        },
        sketch.holes.clone(),
        SearchBudget {
            max_assignments: crate::MAX_ASSIGNMENTS,
        },
    ))
}

// The previous implementation reconstructs each Op value independently of the
// stored hole values. Keep it as an oracle for lookup and first-error behavior.
fn previous_close(sketch: &Sketch, assignment: &Assignment) -> Result<Program, Error> {
    let nodes = sketch
        .nodes
        .iter()
        .map(|slot| match slot {
            Slot::Fixed(op) => Ok(op.clone()),
            Slot::Choice { id, alternatives } => {
                let value = assignment
                    .get(HoleId::try_new(*id)?)
                    .ok_or(Error::Invalid("missing-assignment"))?;
                alternatives
                    .iter()
                    .find(|op| op.value() == *value)
                    .cloned()
                    .ok_or(Error::Invalid("unknown-assignment"))
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Program::try_new(
        sketch.inputs.clone(),
        sketch.outputs.clone(),
        nodes,
        sketch.roots.clone(),
    )
}

#[test]
fn stored_choice_values_match_instructions_in_canonical_order() {
    for sketch in problems() {
        for sketch in [&sketch, &sketch.clone()] {
            for slot in &sketch.nodes {
                let Slot::Choice { id, alternatives } = slot else {
                    continue;
                };
                let hole = sketch
                    .holes
                    .iter()
                    .find(|hole| hole.id().get() == *id)
                    .unwrap_or_else(|| panic!("missing hole {id}"));
                assert_eq!(hole.values().len(), alternatives.len());
                let mut previous = None;
                for (value, op) in hole.values().zip(alternatives) {
                    assert_eq!(*value, op.value());
                    let encoded = checked(value.canonical_bytes());
                    if let Some(previous) = previous {
                        assert!(previous < encoded);
                    }
                    previous = Some(encoded);
                }
            }
        }
    }
    assert_eq!(
        mixed_choices(0)
            .holes
            .iter()
            .map(|hole| hole.id().get())
            .collect::<Vec<_>>(),
        vec![91, 7]
    );
}

#[test]
fn closing_every_choice_matches_the_previous_value_comparison() {
    let mut compared = 0;
    for sketch in problems() {
        let problem = problem(&sketch);
        let mut indexes = vec![0; problem.holes().len()];
        for _ in 0..problem.cardinality() {
            let assignment = assignment_at(&problem, &indexes);
            assert_eq!(
                sketch.close(&assignment),
                previous_close(&sketch, &assignment)
            );
            compared += 1;
            increment_indexes(&problem, &mut indexes);
        }
    }
    assert_eq!(compared, 3 * 81 + 1024 + 1);
}

fn assignment(entries: Vec<(u32, Value)>) -> Assignment {
    let mut entries: Vec<_> = entries
        .into_iter()
        .map(|(id, value)| (checked(HoleId::try_new(id)), value))
        .collect();
    entries.sort_by_key(|(id, _)| *id);
    Assignment {
        entries: entries.into_boxed_slice(),
    }
}

#[test]
fn bad_assignments_keep_the_first_error_and_extra_ids_are_ignored() {
    let sketch = mixed_choices(0);
    let integer = Op::Int(-1).value();
    let boolean = Op::Bool(true).value();
    let missing = Error::Invalid("missing-assignment");
    let unknown = Error::Invalid("unknown-assignment");
    let mut cases = vec![
        (vec![], missing.clone()),
        (vec![(7, boolean.clone())], missing.clone()),
        (vec![(91, integer.clone())], missing.clone()),
        (vec![(91, Value::Unit)], unknown.clone()),
        (vec![(7, Value::Unit)], missing),
        (
            vec![(91, boolean.clone()), (7, integer.clone())],
            unknown.clone(),
        ),
        (
            vec![(91, integer.clone()), (7, Value::Unit)],
            unknown.clone(),
        ),
    ];
    for wrong in [
        Value::Text("\u{e9}".into()),
        tuple(vec![Value::U128(1), Value::I128(-1)]),
        tuple(vec![Value::I128(1), Value::I128(-1), Value::I128(0)]),
        Value::Vector(vec![Value::I128(1), Value::I128(-1)].into_boxed_slice()),
    ] {
        cases.push((vec![(91, wrong), (7, boolean.clone())], unknown.clone()));
    }
    for (entries, expected) in cases {
        let assignment = assignment(entries);
        assert_eq!(previous_close(&sketch, &assignment), Err(expected.clone()));
        assert_eq!(sketch.close(&assignment), Err(expected));
    }
    let good = assignment(vec![(91, integer.clone()), (7, boolean.clone())]);
    let extra = assignment(vec![
        (1, Value::Unit),
        (7, boolean),
        (91, integer),
        (u32::MAX, Value::Bool(false)),
    ]);
    assert_eq!(sketch.close(&extra), previous_close(&sketch, &good));
    assert_eq!(sketch.close(&good), previous_close(&sketch, &good));
}

#[test]
fn duplicate_choice_values_are_rejected_at_construction() {
    assert_eq!(
        Sketch::try_new(
            vec![],
            vec![Domain::Int { min: 0, max: 1 }],
            vec![Slot::Choice {
                id: 7,
                alternatives: vec![Op::Int(1), Op::Int(1)]
            }],
            vec![0],
        ),
        Err(Error::Search(SynthesisError::DuplicateCandidateValue(
            checked(HoleId::try_new(7))
        )))
    );
}
