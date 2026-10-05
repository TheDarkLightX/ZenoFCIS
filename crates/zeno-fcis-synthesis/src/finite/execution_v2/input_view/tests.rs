//! Independent native wire/counter oracle; it does not call the checked readers.
use super::*;
use std::collections::BTreeSet;

fn variants_valid(variants: &[Variant], min: i64, max: i64) -> bool {
    let ids: BTreeSet<_> = variants.iter().map(|v| v.id).collect();
    let codes: BTreeSet<_> = variants.iter().map(|v| v.code).collect();
    min <= max
        && max as i128 - min as i128 + 1 == variants.len() as i128
        && ids.len() == variants.len()
        && codes.len() == variants.len()
        && codes.iter().all(|code| min <= *code && *code <= max)
}
pub(in super::super) fn schema_valid(fields: &[Field]) -> bool {
    fields.windows(2).all(|pair| pair[0].id < pair[1].id)
        && fields.iter().all(|field| match &field.leaf {
            Leaf::I128 { min, max } => min <= max,
            Leaf::U128 { min, max } => min <= max && *max <= i64::MAX as u128,
            Leaf::Bool => true,
            Leaf::Enum {
                min, max, variants, ..
            }
            | Leaf::Sum {
                min, max, variants, ..
            } => variants_valid(variants, *min, *max),
        })
}
fn take<const N: usize>(bytes: &[u8], position: &mut usize) -> Option<[u8; N]> {
    let end = position.checked_add(N)?;
    let result = bytes.get(*position..end)?.try_into().ok()?;
    *position = end;
    Some(result)
}
fn field(bytes: &[u8], position: &mut usize, descriptor: &Field) -> Option<i64> {
    if u16::from_be_bytes(take(bytes, position)?) != descriptor.id {
        return None;
    }
    let [tag] = take(bytes, position)?;
    match &descriptor.leaf {
        Leaf::U128 { min, max } => {
            if tag != 3 {
                return None;
            }
            let value = u128::from_be_bytes(take(bytes, position)?);
            (*min <= value && value <= *max && value <= i64::MAX as u128).then_some(value as i64)
        }
        Leaf::I128 { min, max } => {
            if tag != 4 {
                return None;
            }
            let value = i128::from_be_bytes(take(bytes, position)?);
            (*min as i128 <= value && value <= *max as i128).then_some(value as i64)
        }
        Leaf::Bool => match tag {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        },
        Leaf::Enum {
            type_id, variants, ..
        }
        | Leaf::Sum {
            type_id, variants, ..
        } => {
            let sum = matches!(&descriptor.leaf, Leaf::Sum { .. });
            if tag != if sum { 10 } else { 7 } {
                return None;
            }
            if u32::from_be_bytes(take(bytes, position)?) != *type_id {
                return None;
            }
            let id = u16::from_be_bytes(take(bytes, position)?);
            if sum && take::<1>(bytes, position)? != [0] {
                return None;
            }
            variants.iter().find(|v| v.id == id).map(|v| v.code)
        }
    }
}
fn charge(
    used: &mut [u64; 8],
    limits: &[u64; 8],
    index: usize,
    resource: super::super::Resource,
    amount: u64,
) -> Result<(), Failure> {
    let total = used[index] as u128 + amount as u128;
    if total > u64::MAX as u128 || total > limits[index] as u128 {
        return Err(Failure::Budget(super::super::MeterFailure {
            resource,
            limit: limits[index],
            attempted: total.min(u64::MAX as u128) as u64,
            overflow: total > u64::MAX as u128,
        }));
    }
    used[index] = total as u64;
    Ok(())
}
pub(in super::super) fn oracle(
    bytes: &[u8],
    fields: &[Field],
    limits: [u64; 8],
    initial: [u64; 8],
    initial_attempts: Vec<AccessAttempt>,
) -> (Result<Vec<i64>, Failure>, [u64; 8], Vec<AccessAttempt>) {
    let mut used = initial;
    let mut attempts = initial_attempts;
    let result = (|| {
        if !schema_valid(fields) {
            return Err(Failure::Schema);
        }
        charge(
            &mut used,
            &limits,
            4,
            super::super::Resource::Byte,
            bytes.len() as u64,
        )?;
        let mut position = 0;
        if take::<1>(bytes, &mut position) != Some([9]) {
            return Err(Failure::Header);
        }
        let count = take::<4>(bytes, &mut position).ok_or(Failure::Header)?;
        if u32::from_be_bytes(count) as usize != fields.len() {
            return Err(Failure::Header);
        }
        let mut values = Vec::new();
        for descriptor in fields {
            let charged = charge(&mut used, &limits, 0, super::super::Resource::Read, 1);
            attempts.push(AccessAttempt {
                field_id: descriptor.id,
                permitted: charged.is_ok(),
            });
            charged?;
            values.push(
                field(bytes, &mut position, descriptor).ok_or(Failure::Field(descriptor.id))?,
            );
        }
        if position != bytes.len() {
            return Err(Failure::Trailing);
        }
        Ok(values)
    })();
    (result, used, attempts)
}
fn mixed() -> (Vec<Field>, Vec<u8>) {
    let fields = std::vec![
        Field {
            id: 0,
            leaf: Leaf::I128 {
                min: i64::MIN,
                max: i64::MIN
            }
        },
        Field {
            id: 1,
            leaf: Leaf::Bool
        },
        Field {
            id: 7,
            leaf: Leaf::Enum {
                type_id: 0,
                min: i64::MIN,
                max: i64::MIN + 1,
                variants: std::vec![
                    Variant {
                        id: u16::MAX,
                        code: i64::MIN
                    },
                    Variant {
                        id: 0,
                        code: i64::MIN + 1
                    }
                ]
            }
        },
        Field {
            id: 256,
            leaf: Leaf::Sum {
                type_id: u32::MAX,
                min: i64::MAX - 1,
                max: i64::MAX,
                variants: std::vec![
                    Variant {
                        id: u16::MAX,
                        code: i64::MAX
                    },
                    Variant {
                        id: 7,
                        code: i64::MAX - 1
                    }
                ]
            }
        },
        Field {
            id: 300,
            leaf: Leaf::I128 {
                min: i64::MAX,
                max: i64::MAX
            }
        },
        Field {
            id: u16::MAX,
            leaf: Leaf::Bool
        },
    ];
    let mut bytes = std::vec![9, 0, 0, 0, 6];
    for (id, payload) in [
        (0u16, [&[4][..], &(i64::MIN as i128).to_be_bytes()].concat()),
        (1, std::vec![2]),
        (7, std::vec![7, 0, 0, 0, 0, 0, 0]),
        (256, std::vec![10, 255, 255, 255, 255, 255, 255, 0]),
        (300, [&[4][..], &(i64::MAX as i128).to_be_bytes()].concat()),
        (u16::MAX, std::vec![1]),
    ] {
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(&payload);
    }
    (fields, bytes)
}
fn assert_public(bytes: &[u8], fields: &[Field], limits: [u64; 8]) {
    let expected = oracle(bytes, fields, limits, [0; 8], Vec::new());
    let outcome = project(bytes, fields, super::super::Limits { counters: limits });
    let (result, usage, attempts) = outcome.into_parts();
    assert_eq!((result, usage.counters, attempts), expected);
}

#[test]
fn raw_projection_matches_independent_wire_and_wider_counter_oracle() {
    let (fields, original) = mixed();
    let mut corpus = std::vec![original.clone()];
    corpus.extend((0..original.len()).map(|end| original[..end].to_vec()));
    for index in 0..original.len() {
        for mask in [1, 128, 255] {
            let mut bytes = original.clone();
            bytes[index] ^= mask;
            corpus.push(bytes);
        }
    }
    let mut trailing = original.clone();
    trailing.push(0);
    corpus.push(trailing);
    for bytes in corpus {
        for byte_limit in [
            0,
            (bytes.len() as u64).saturating_sub(1),
            bytes.len() as u64,
            u64::MAX,
        ] {
            for read_limit in [0, 1, 2, 5, 6, u64::MAX] {
                let mut limits = [u64::MAX; 8];
                limits[4] = byte_limit;
                limits[0] = read_limit;
                assert_public(&bytes, &fields, limits);
            }
        }
    }
    let (actual, _, _) = project(
        &original,
        &fields,
        super::super::Limits {
            counters: [u64::MAX; 8],
        },
    )
    .into_parts();
    assert_eq!(
        actual,
        Ok(std::vec![i64::MIN, 1, i64::MIN + 1, i64::MAX, i64::MAX, 0])
    );
}

#[test]
fn arbitrary_closed_intervals_maps_and_invalid_metadata_have_exact_admission() {
    for base in [i64::MIN, -3, 10, i64::MAX - 2] {
        let vars = std::vec![
            Variant {
                id: 0,
                code: base + 2
            },
            Variant {
                id: u16::MAX,
                code: base
            },
            Variant {
                id: 7,
                code: base + 1
            }
        ];
        let fields = [Field {
            id: 0,
            leaf: Leaf::Enum {
                type_id: u32::MAX,
                min: base,
                max: base + 2,
                variants: vars,
            },
        }];
        let bytes = [9, 0, 0, 0, 1, 0, 0, 7, 255, 255, 255, 255, 0, 7];
        assert_public(&bytes, &fields, [u64::MAX; 8]);
        let (value, _, _) = project(
            &bytes,
            &fields,
            super::super::Limits {
                counters: [u64::MAX; 8],
            },
        )
        .into_parts();
        assert_eq!(value, Ok(std::vec![base + 1]));
    }
    for (min, max, vars) in [
        (
            10,
            11,
            std::vec![
                Variant { id: 7, code: 11 },
                Variant {
                    id: u16::MAX,
                    code: 10
                }
            ],
        ),
        (
            i64::MIN,
            i64::MIN,
            std::vec![Variant {
                id: 0,
                code: i64::MIN
            }],
        ),
        (
            i64::MAX,
            i64::MAX,
            std::vec![Variant {
                id: u16::MAX,
                code: i64::MAX
            }],
        ),
        (
            i64::MIN,
            i64::MAX,
            std::vec![
                Variant {
                    id: 0,
                    code: i64::MIN
                },
                Variant {
                    id: 7,
                    code: i64::MAX
                }
            ],
        ),
        (10, 9, std::vec![]),
        (
            10,
            11,
            std::vec![Variant { id: 0, code: 10 }, Variant { id: 0, code: 11 }],
        ),
        (
            10,
            11,
            std::vec![Variant { id: 0, code: 10 }, Variant { id: 7, code: 10 }],
        ),
        (
            10,
            11,
            std::vec![Variant { id: 0, code: 9 }, Variant { id: 7, code: 11 }],
        ),
    ] {
        let fields = [Field {
            id: 0,
            leaf: Leaf::Sum {
                type_id: 0,
                min,
                max,
                variants: vars,
            },
        }];
        assert_public(
            &[9, 0, 0, 0, 1, 0, 0, 10, 0, 0, 0, 0, 0, 7, 0],
            &fields,
            [u64::MAX; 8],
        );
        assert_public(&[0], &fields, [0; 8]);
    }
    for fields in [
        std::vec![Field {
            id: 0,
            leaf: Leaf::I128 { min: 2, max: 1 }
        }],
        std::vec![
            Field {
                id: 7,
                leaf: Leaf::Bool
            },
            Field {
                id: 7,
                leaf: Leaf::Bool
            }
        ],
        std::vec![
            Field {
                id: 7,
                leaf: Leaf::Bool
            },
            Field {
                id: 0,
                leaf: Leaf::Bool
            }
        ],
    ] {
        assert_public(&[9, 0, 0, 0, 0], &fields, [0; 8]);
    }
}

#[test]
fn private_projection_retains_arbitrary_usage_attempt_prefix_and_refusal_cleanup() {
    let (fields, original) = mixed();
    let prefix = std::vec![
        AccessAttempt {
            field_id: 123,
            permitted: true
        },
        AccessAttempt {
            field_id: 4,
            permitted: false
        }
    ];
    for initial_read in [0, 3, u64::MAX - 1, u64::MAX] {
        for initial_byte in [0, u64::MAX - original.len() as u64, u64::MAX] {
            for read_limit in [0, 3, u64::MAX] {
                for byte_limit in [0, original.len() as u64, u64::MAX] {
                    for bytes in [&original[..], &original[..original.len() - 1], &[][..]] {
                        let initial = [initial_read, u64::MAX, 12, 17, initial_byte, 3, 9, 5];
                        let mut limits = [u64::MAX; 8];
                        limits[0] = read_limit;
                        limits[4] = byte_limit;
                        let expected = oracle(bytes, &fields, limits, initial, prefix.clone());
                        let mut meter = super::super::meter::Meter {
                            limits: super::super::Limits { counters: limits },
                            used: super::super::Usage { counters: initial },
                        };
                        let mut output = std::vec![-8, 16];
                        let mut attempts = prefix.clone();
                        let result =
                            project_into(bytes, &fields, &mut meter, &mut output, &mut attempts);
                        let actual = result.map(|()| output.clone());
                        if actual.is_err() {
                            assert!(output.is_empty());
                        }
                        assert_eq!((actual, meter.used.counters, attempts), expected);
                    }
                }
            }
        }
    }
}

#[test]
fn empty_and_full_sized_records_need_no_artificial_field_limit() {
    for count in [0u16, 32, 100] {
        let fields: Vec<_> = (0..count)
            .map(|id| Field {
                id,
                leaf: Leaf::Bool,
            })
            .collect();
        let mut bytes = std::vec![9];
        bytes.extend_from_slice(&(count as u32).to_be_bytes());
        for id in 0..count {
            bytes.extend_from_slice(&id.to_be_bytes());
            bytes.push(1 + (id % 2) as u8);
        }
        assert_public(&bytes, &fields, [u64::MAX; 8]);
        let outcome = project(
            &bytes,
            &fields,
            super::super::Limits {
                counters: [u64::MAX; 8],
            },
        );
        assert_eq!(
            outcome.usage().used(super::super::Resource::Read),
            count as u64
        );
        assert_eq!(outcome.attempts().len(), count as usize);
    }
}

#[test]
fn unsigned_scalar_projection_checks_original_wire_and_is_injective() {
    let leaf = Leaf::U128 {
        min: 0,
        max: 1_000_000,
    };
    assert!(validate_leaf(&leaf));
    for value in [0u128, 1, 9, 1_000_000] {
        let mut bytes = alloc::vec![3];
        bytes.extend_from_slice(&value.to_be_bytes());
        assert_eq!(decode_scalar(&bytes, 0, &leaf), Some((value as i64, 17)));
        let actual = super::super::decision::Atom::U128(
            decode_scalar(&bytes, 0, &leaf)
                .unwrap_or_else(|| panic!("strict83 expected checked unsigned scalar projection"))
                .0 as u128,
        );
        assert!(matches!(actual,super::super::decision::Atom::U128(v) if v==value));
        assert_eq!(
            crate::finite::canonical_v2::output::encode_atom(actual, 17).unwrap_or_else(
                |error| panic!("strict83 unexpected typed test refusal: {error:?}")
            ),
            bytes
        );
        for end in 0..17 {
            assert!(decode_scalar(&bytes[..end], 0, &leaf).is_none());
        }
        bytes[0] = 4;
        assert!(decode_scalar(&bytes, 0, &leaf).is_none());
    }
    for value in [
        1_000_001u128,
        i64::MAX as u128,
        i64::MAX as u128 + 1,
        u128::MAX,
    ] {
        let mut bytes = alloc::vec![3];
        bytes.extend_from_slice(&value.to_be_bytes());
        assert!(decode_scalar(&bytes, 0, &leaf).is_none());
    }
    let largest = Leaf::U128 {
        min: i64::MAX as u128,
        max: i64::MAX as u128,
    };
    assert!(validate_leaf(&largest));
    let mut bytes = alloc::vec![3];
    bytes.extend_from_slice(&(i64::MAX as u128).to_be_bytes());
    assert_eq!(decode_scalar(&bytes, 0, &largest), Some((i64::MAX, 17)));
    assert!(!validate_leaf(&Leaf::U128 {
        min: 0,
        max: i64::MAX as u128 + 1
    }));
    assert!(!validate_leaf(&Leaf::U128 { min: 1, max: 0 }));
}

// Migrates the removed account reader's exact signed-wire/truncation grid to
// the surviving decoder. Its admitted scalar range is explicitly i64; full
// i128 byte values outside that profile refuse instead of being narrowed.
#[test]
fn migrated_account_signed_wire_reader_extremes() {
    let values = [
        i128::MIN,
        i128::MIN + 1,
        -901,
        -900,
        -1,
        0,
        1,
        2,
        3,
        899,
        900,
        901,
        i64::MAX as i128 + 1,
        i128::MAX - 900,
        i128::MAX - 1,
        i128::MAX,
    ];
    for x in values {
        for id in [0, 1, 110, u16::MAX] {
            let field = Field {
                id,
                leaf: Leaf::I128 {
                    min: i64::MIN,
                    max: i64::MAX,
                },
            };
            let mut b = id.to_be_bytes().to_vec();
            b.push(4);
            b.extend(x.to_be_bytes());
            assert_eq!(
                decode_field(&b, 0, &field),
                i64::try_from(x).ok().map(|n| (n, 19))
            );
            for end in 0..19 {
                assert_eq!(decode_field(&b[..end], 0, &field), None);
            }
            assert_eq!(decode_field(&b, usize::MAX, &field), None);
        }
    }
}
