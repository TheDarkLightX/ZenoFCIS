//! System relation bounds are independent of the narrower synthesis grammar.
#![allow(clippy::unwrap_used)]

use zeno_fcis_synthesis::finite::{
    Budget, Contract, Domain, Error, Op, Program, Sketch, Slot, synthesize,
};
use zeno_fcis_synthesis::system::{Property, SystemCheck, SystemLimits, check_system_property};

const ZERO: Domain = Domain::Int { min: 0, max: 0 };
const BIT: Domain = Domain::Int { min: 0, max: 1 };

fn constant_relation(inputs: Vec<Domain>) -> Program {
    Program::try_new(inputs, vec![Domain::Bool], vec![Op::Bool(true)], vec![0]).unwrap()
}

/// Only the last input varies, and the last output must preserve it. All
/// preceding fields are singleton integers, keeping exhaustive checks small.
fn identity(inputs: usize, outputs: usize) -> (Program, Property) {
    let mut input_domains = vec![ZERO; inputs];
    input_domains[inputs - 1] = BIT;
    let mut output_domains = vec![ZERO; outputs];
    output_domains[outputs - 1] = BIT;
    let mut roots = vec![0; outputs];
    roots[outputs - 1] = 1;
    let last_input = u16::try_from(inputs - 1).unwrap();
    let last_output = u16::try_from(inputs + outputs - 1).unwrap();
    let transition = Program::try_new(
        input_domains.clone(),
        output_domains.clone(),
        vec![Op::Int(0), Op::Input(last_input)],
        roots,
    )
    .unwrap();
    let relation = Program::try_new(
        [input_domains.as_slice(), output_domains.as_slice()].concat(),
        vec![Domain::Bool],
        vec![Op::Input(last_input), Op::Input(last_output), Op::Eq(0, 1)],
        vec![2],
    )
    .unwrap();
    let property = Property::try_new(input_domains, output_domains, relation).unwrap();
    (transition, property)
}

#[test]
fn system_properties_cover_wide_transitions_and_the_combined_boundary() {
    for (inputs, outputs) in [(17, 9), (31, 1), (16, 16)] {
        let (transition, property) = identity(inputs, outputs);
        assert_eq!(
            check_system_property(&transition, property.contract(), SystemLimits::default()),
            Ok(SystemCheck::SystemProperty { inputs: 2 })
        );
        let input = vec![0; inputs];
        let mut output = vec![0; outputs];
        output[outputs - 1] = 1;
        assert_eq!(property.contract().holds(&input, &output), Ok(false));
    }
}

#[test]
fn system_property_constructor_rejects_bad_shapes() {
    let shape_error = Err(Error::Invalid("contract-shape"));
    for (inputs, outputs, relation_inputs) in [
        (32, 1, 32),  // Combined bound exceeded.
        (16, 17, 32), // Transition output bound exceeded.
        (17, 0, 17),  // Empty output tuple.
        (17, 9, 25),  // Missing relation field.
    ] {
        assert_eq!(
            Property::try_new(
                vec![ZERO; inputs],
                vec![ZERO; outputs],
                constant_relation(vec![ZERO; relation_inputs]),
            ),
            shape_error
        );
    }
    let mut mismatched = vec![ZERO; 26];
    mismatched[17] = BIT;
    assert_eq!(
        Property::try_new(vec![ZERO; 17], vec![ZERO; 9], constant_relation(mismatched)),
        shape_error
    );
    let non_boolean =
        Program::try_new(vec![ZERO; 26], vec![ZERO], vec![Op::Int(0)], vec![0]).unwrap();
    assert_eq!(
        Property::try_new(vec![ZERO; 17], vec![ZERO; 9], non_boolean),
        shape_error
    );
    assert_eq!(
        Program::try_new(
            vec![ZERO; 33],
            vec![Domain::Bool],
            vec![Op::Bool(true)],
            vec![0]
        ),
        Err(Error::Invalid("program-shape"))
    );
}

#[test]
fn synthesis_cannot_accept_wide_system_contracts() {
    let (_, property) = identity(17, 9);
    assert_eq!(
        Contract::try_new(
            property.contract().inputs().to_vec(),
            property.contract().outputs().to_vec(),
            property.relation().clone(),
        ),
        Err(Error::Invalid("contract-shape"))
    );
    assert_eq!(
        Contract::try_new(
            vec![ZERO; 15],
            vec![ZERO; 17],
            constant_relation(vec![ZERO; 32]),
        ),
        Err(Error::Invalid("contract-shape"))
    );
    assert_eq!(
        Sketch::try_new(
            vec![ZERO; 17],
            vec![ZERO; 9],
            vec![Slot::Fixed(Op::Int(0))],
            vec![0; 9],
        ),
        Err(Error::Limit("input-fields"))
    );
    assert_eq!(
        Sketch::try_new(
            vec![ZERO; 15],
            vec![ZERO; 17],
            vec![Slot::Fixed(Op::Int(0))],
            vec![0; 17],
        ),
        Err(Error::Invalid("program-shape"))
    );
    let narrow = Sketch::try_new(
        vec![ZERO; 16],
        vec![ZERO; 9],
        vec![Slot::Choice {
            id: 1,
            alternatives: vec![Op::Int(0)],
        }],
        vec![0; 9],
    )
    .unwrap();
    assert_eq!(
        synthesize(property.contract(), &narrow, Budget::default()),
        Err(Error::Invalid("schema-mismatch"))
    );
}

#[test]
fn existing_contracts_keep_their_representation_and_commitment() {
    // Obtained by constructing these contracts with unchanged commit
    // 4a4092e9fd59dfd1ad5dba7c9c0a0006f6d26cdb, before the wider property API.
    for (inputs, outputs, expected) in [
        (
            0,
            1,
            "3629ab50804496c80e084fbc1016079cfae8e795b85e57b5ac93720dda34ab5d",
        ),
        (
            1,
            1,
            "3fd913f7b713103841a1d3697fac7f6902a2d50e972c1f52f3815bcae01dd36a",
        ),
        (
            16,
            16,
            "13a1cd4ec0d54c6654219c8d76bf045b8bd00060570b3bf587b895cfe66528d9",
        ),
    ] {
        let relation = constant_relation(vec![ZERO; inputs + outputs]);
        let contract =
            Contract::try_new(vec![ZERO; inputs], vec![ZERO; outputs], relation.clone()).unwrap();
        let property =
            Property::try_new(vec![ZERO; inputs], vec![ZERO; outputs], relation).unwrap();
        assert_eq!(property.contract(), &contract);
        assert_eq!(property.contract().commitment(), contract.commitment());
        let hex: String = contract
            .commitment()
            .unwrap()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hex, expected);
    }
}

#[test]
fn wider_shapes_do_not_widen_enumeration_limits() {
    let transition =
        Program::try_new(vec![BIT; 17], vec![ZERO], vec![Op::Int(0)], vec![0]).unwrap();
    let property = Property::try_new(
        vec![BIT; 17],
        vec![ZERO],
        constant_relation([vec![BIT; 17], vec![ZERO]].concat()),
    )
    .unwrap();
    assert_eq!(
        check_system_property(&transition, property.contract(), SystemLimits::default()),
        Err(Error::Limit("space"))
    );
    let (transition, property) = identity(17, 9);
    assert_eq!(
        check_system_property(
            &transition,
            property.contract(),
            SystemLimits {
                max_inputs: 1,
                max_pairs: 4,
            }
        ),
        Err(Error::Limit("system-inputs"))
    );
    assert_eq!(
        check_system_property(
            &transition,
            property.contract(),
            SystemLimits {
                max_inputs: 2,
                max_pairs: 3,
            }
        ),
        Err(Error::Limit("system-pairs"))
    );
}
