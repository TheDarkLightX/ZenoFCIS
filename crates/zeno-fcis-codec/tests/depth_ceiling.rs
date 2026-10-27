//! Regressions for the decoder's depth ceiling: nesting deeper than
//! `ValueLimits::MAX_SUPPORTED_DEPTH` is refused, even under limits that admit
//! more, before recursion can exhaust the stack.

use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, EncodeError, canonical_value_bytes_with_limits,
    decode_value,
};
use zeno_fcis_value::{Value, ValueError, ValueLimits};

/// The encoding of `depth` nested singleton tuples around a unit, built
/// without constructing the deep value.
fn nested_tuples(depth: usize) -> Vec<u8> {
    let unit = Value::unit()
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("{error:?}"));
    let one = Value::tuple(vec![Value::unit()])
        .unwrap_or_else(|error| panic!("{error:?}"))
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("{error:?}"));
    let mut bytes = one[..one.len() - unit.len()].repeat(depth);
    bytes.extend_from_slice(&unit);
    bytes
}

/// Limits that admit any depth, node count, payload and collection length.
fn permissive() -> DecodeLimits {
    DecodeLimits {
        max_input_bytes: u64::MAX,
        value: ValueLimits {
            max_depth: u32::MAX,
            max_nodes: u64::MAX,
            max_payload_bytes: u64::MAX,
            max_collection_len: u32::MAX,
        },
    }
}

/// Runs `f` on a 16 MiB stack: enough for the ceiling in an unoptimized
/// build, far too little for 60,000 levels.
fn on_ceiling_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(f)
        .unwrap_or_else(|error| panic!("{error:?}"))
        .join()
        .unwrap_or_else(|error| panic!("{error:?}"))
}

#[test]
fn nesting_beyond_the_supported_depth_is_refused_under_permissive_limits() {
    // 60,000 levels aborted the process on an 8 MiB stack before the ceiling.
    for depth in [1_025, 2_000, 60_000] {
        let bytes = nested_tuples(depth);
        let result = on_ceiling_stack(move || decode_value(&bytes, permissive()));
        assert_eq!(
            result,
            Err(DecodeError::DepthLimit {
                limit: ValueLimits::MAX_SUPPORTED_DEPTH,
                attempted: ValueLimits::MAX_SUPPORTED_DEPTH + 1,
            }),
            "depth {depth}"
        );
    }
}

#[test]
fn validation_and_encoding_refuse_beyond_the_supported_depth() {
    let (validated, encoded) = on_ceiling_stack(|| {
        let mut value = Value::unit();
        for _ in 0..=ValueLimits::MAX_SUPPORTED_DEPTH {
            value = Value::tuple(vec![value]).unwrap_or_else(|error| panic!("{error:?}"));
        }
        (
            value.validate_limits(permissive().value).err(),
            canonical_value_bytes_with_limits(&value, permissive().value).err(),
        )
    });
    let refusal = ValueError::DepthLimit {
        limit: ValueLimits::MAX_SUPPORTED_DEPTH,
        attempted: ValueLimits::MAX_SUPPORTED_DEPTH + 1,
    };
    assert_eq!(validated, Some(refusal.clone()));
    assert_eq!(encoded, Some(EncodeError::InvalidValue(refusal)));
}

#[test]
fn the_supported_depth_decodes_and_round_trips() {
    let bytes = nested_tuples(ValueLimits::MAX_SUPPORTED_DEPTH as usize);
    let same = on_ceiling_stack(move || {
        let value = decode_value(&bytes, permissive()).unwrap_or_else(|error| panic!("{error:?}"));
        canonical_value_bytes_with_limits(&value, permissive().value)
            .unwrap_or_else(|error| panic!("{error:?}"))
            == bytes
    });
    assert!(same);
}

#[test]
fn default_limits_still_refuse_at_depth_65() {
    assert_eq!(
        decode_value(&nested_tuples(65), DecodeLimits::default()),
        Err(DecodeError::DepthLimit {
            limit: 64,
            attempted: 65,
        })
    );
    assert!(decode_value(&nested_tuples(64), DecodeLimits::default()).is_ok());
}
