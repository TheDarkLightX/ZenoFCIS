//! Public raw-record composition against actual canonical Value bytes.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{
    Domain, Op, V2InputBinding, V2InputField, V2InputLeaf, V2InputVariant, V2RawRecord,
    V2RecordExecutionFailure, V2RecordFailure, V2RecordInvocation, V2RecordSource, V2Resource,
    V2ScalarProgram, execute_records_v2, v2_zero_limits,
};
use zeno_fcis_value::{Field, Value};

#[test]
fn canonical_original_records_preserve_binding_order_and_shared_budget() {
    let values = [
        Value::record_canonical(vec![
            Field::new(7, Value::I128(-9)),
            Field::new(32, Value::Bool(true)),
        ]),
        Value::record_canonical(vec![Field::new(
            7,
            Value::Enum {
                type_id: u32::MAX,
                variant: u16::MAX,
            },
        )]),
        Value::record_canonical(vec![Field::new(
            u16::MAX,
            Value::Sum {
                type_id: 0,
                variant: 7,
                payload: None,
            },
        )]),
    ];
    let bytes: [Vec<u8>; 3] = values.map(|value| {
        value
            .unwrap_or_else(|e| panic!("canonical record: {e}"))
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("canonical bytes: {e}"))
    });
    let state = [
        V2InputField {
            id: 7,
            leaf: V2InputLeaf::I128 { min: -9, max: 9 },
        },
        V2InputField {
            id: 32,
            leaf: V2InputLeaf::Bool,
        },
    ];
    let command = [V2InputField {
        id: 7,
        leaf: V2InputLeaf::Enum {
            type_id: u32::MAX,
            min: -5,
            max: -4,
            variants: vec![
                V2InputVariant {
                    id: u16::MAX,
                    code: -4,
                },
                V2InputVariant { id: 0, code: -5 },
            ],
        },
    }];
    let context = [V2InputField {
        id: u16::MAX,
        leaf: V2InputLeaf::Sum {
            type_id: 0,
            min: 10,
            max: 11,
            variants: vec![
                V2InputVariant { id: 7, code: 11 },
                V2InputVariant {
                    id: u16::MAX,
                    code: 10,
                },
            ],
        },
    }];
    let inv = V2RecordInvocation {
        state: V2RawRecord {
            bytes: &bytes[0],
            fields: &state,
        },
        command: V2RawRecord {
            bytes: &bytes[1],
            fields: &command,
        },
        context: V2RawRecord {
            bytes: &bytes[2],
            fields: &context,
        },
    };
    let bindings = [
        V2InputBinding {
            source: V2RecordSource::Context,
            field: u16::MAX,
        },
        V2InputBinding {
            source: V2RecordSource::State,
            field: 32,
        },
        V2InputBinding {
            source: V2RecordSource::Command,
            field: 7,
        },
        V2InputBinding {
            source: V2RecordSource::State,
            field: 7,
        },
    ];
    let domains = [
        Domain::Int { min: 10, max: 11 },
        Domain::Bool,
        Domain::Int { min: -5, max: -4 },
        Domain::Int { min: -9, max: 9 },
    ];
    let program = V2ScalarProgram {
        inputs: &domains,
        outputs: &domains,
        nodes: &[Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)],
        roots: &[0, 1, 2, 3],
    };
    let total = bytes.iter().map(|b| b.len() as u64).sum();
    let limits = v2_zero_limits()
        .with_limit(V2Resource::Byte, total)
        .with_limit(V2Resource::Read, 4)
        .with_limit(V2Resource::Step, 4);
    let complete = execute_records_v2(&inv, &program, &bindings, limits);
    assert_eq!(complete.usage().used(V2Resource::Byte), total);
    assert_eq!(complete.usage().used(V2Resource::Read), 4);
    assert_eq!(complete.usage().used(V2Resource::Step), 4);
    assert_eq!(
        complete
            .attempts(V2RecordSource::Command)
            .iter()
            .map(|a| (a.field_id(), a.permitted()))
            .collect::<Vec<_>>(),
        [(7, true)]
    );
    assert_eq!(complete.into_parts().0, Ok(vec![11, 1, -4, -9]));
    let refused = execute_records_v2(
        &inv,
        &program,
        &bindings,
        limits.with_limit(V2Resource::Read, 3),
    );
    assert_eq!(refused.usage().used(V2Resource::Byte), total);
    assert_eq!(refused.usage().used(V2Resource::Read), 3);
    assert_eq!(refused.usage().used(V2Resource::Step), 0);
    assert_eq!(refused.attempts(V2RecordSource::State).len(), 2);
    assert_eq!(
        refused
            .attempts(V2RecordSource::Context)
            .iter()
            .map(|a| (a.field_id(), a.permitted()))
            .collect::<Vec<_>>(),
        [(u16::MAX, false)]
    );
    assert!(
        matches!(refused.into_parts().0,Err(V2RecordExecutionFailure::Record { source:V2RecordSource::Context,refusal:V2RecordFailure::Budget(error) })
        if error.resource==V2Resource::Read && error.limit==3 && error.attempted==4)
    );
    let step = execute_records_v2(
        &inv,
        &program,
        &bindings,
        limits.with_limit(V2Resource::Step, 3),
    );
    assert_eq!(step.usage().used(V2Resource::Byte), total);
    assert_eq!(step.usage().used(V2Resource::Read), 4);
    assert_eq!(step.usage().used(V2Resource::Step), 3);
    assert!(matches!(
        step.into_parts().0,
        Err(V2RecordExecutionFailure::Execution(_))
    ));
    for resource in [
        V2Resource::Candidate,
        V2Resource::Write,
        V2Resource::Effect,
        V2Resource::WitnessByte,
        V2Resource::Depth,
    ] {
        assert_eq!(
            execute_records_v2(&inv, &program, &bindings, limits)
                .usage()
                .used(resource),
            0
        );
    }
}
