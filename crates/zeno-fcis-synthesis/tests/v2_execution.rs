//! Compare the public V2 convenience adapter with the checked entry point.
use zeno_fcis_synthesis::finite::{
    Domain, Op, Program, V2ExecutionFailure, V2Resource, execute_v2, v2_zero_limits,
};

#[test]
fn public_program_adapter_matches_checked_entry_and_restarts_usage() {
    let domain = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let program = Program::try_new(
        vec![domain],
        vec![domain],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap_or_else(|error| panic!("program: {error}"));
    for quota in [0, 1, 2, 3, 4, u64::MAX] {
        for value in [i64::MIN, -1, 0, i64::MAX] {
            let limits = v2_zero_limits().with_limit(V2Resource::Step, quota);
            let actual = program.execute_v2(&[value], limits);
            assert_eq!(
                actual,
                execute_v2(
                    program.inputs(),
                    program.outputs(),
                    program.nodes(),
                    program.roots(),
                    &[value],
                    limits
                )
            );
            let (result, usage) = actual.into_parts();
            assert_eq!(usage.used(V2Resource::Step), quota.min(3));
            if quota >= 3 && value == i64::MAX {
                assert_eq!(result, Err(V2ExecutionFailure::Arithmetic));
            }
            assert_eq!(program.execute_v2(&[value], limits).into_parts().1, usage);
        }
    }
}

#[test]
fn public_byte_readers_preserve_existing_canonical_integer_payloads() {
    use zeno_fcis_codec::CanonicalEncode;
    use zeno_fcis_synthesis::finite::canonical_v2::{read_big_endian, read_signed_128};
    use zeno_fcis_value::Value;

    for value in [i128::MIN, i128::MIN + 1, -1, 0, 1, i128::MAX - 1, i128::MAX] {
        let encoded = Value::signed(value)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("canonical integer: {error}"));
        assert_eq!(encoded.len(), 17);
        assert_eq!(encoded[0], zeno_fcis_value::zcve::TAG_I128);
        assert_eq!(read_signed_128(&encoded, 1), Some((value, 17)));
        let payload: [u8; 16] = encoded[1..]
            .try_into()
            .unwrap_or_else(|error| panic!("integer payload: {error}"));
        assert_eq!(
            read_big_endian(&encoded, 1, 16),
            Some((u128::from_be_bytes(payload), 17))
        );
        for truncated in 0..encoded.len() {
            assert_eq!(read_signed_128(&encoded[..truncated], 1), None);
        }
    }
    assert_eq!(read_big_endian(&[], 0, 0), Some((0, 0)));
    assert_eq!(read_big_endian(&[], usize::MAX, 0), None);
}
