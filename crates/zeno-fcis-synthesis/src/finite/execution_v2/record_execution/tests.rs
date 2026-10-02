//! Independent wire, metadata and wide-counter oracles composed without runtime helpers.
use super::super::{MeterFailure, Resource};
use super::*;
use input_view::{Leaf, Variant};
use std::collections::BTreeSet;

const SOURCES: [Source; 3] = [Source::State, Source::Command, Source::Context];
type OracleOutcome = (Result<Vec<i64>, Failure>, [u64; 8], [Vec<AccessAttempt>; 3]);
fn record<'a>(inv: &'a Invocation<'_>, source: Source) -> &'a RawRecord<'a> {
    match source {
        Source::State => &inv.state,
        Source::Command => &inv.command,
        Source::Context => &inv.context,
    }
}
fn domain(leaf: &Leaf) -> Domain {
    match *leaf {
        Leaf::Bool => Domain::Bool,
        Leaf::I128 { min, max } | Leaf::Enum { min, max, .. } | Leaf::Sum { min, max, .. } => {
            Domain::Int { min, max }
        }
    }
}
fn source_index(source: Source) -> usize {
    match source {
        Source::State => 0,
        Source::Command => 1,
        Source::Context => 2,
    }
}
fn oracle(
    inv: &Invocation<'_>,
    program: &ScalarProgram<'_>,
    bindings: &[Binding],
    limits: [u64; 8],
    initial: [u64; 8],
    prefix: [Vec<AccessAttempt>; 3],
) -> OracleOutcome {
    let mut used = initial;
    let mut attempts = prefix;
    let result = (|| {
        for source in SOURCES {
            if !input_view::tests::schema_valid(record(inv, source).fields) {
                return Err(Failure::Schema(source));
            }
        }
        let declared: BTreeSet<_> = SOURCES
            .iter()
            .flat_map(|source| {
                record(inv, *source)
                    .fields
                    .iter()
                    .map(|field| (source_index(*source), field.id))
            })
            .collect();
        let chosen: BTreeSet<_> = bindings
            .iter()
            .map(|b| (source_index(b.source), b.field))
            .collect();
        if bindings.len() != program.inputs.len()
            || chosen.len() != bindings.len()
            || chosen != declared
        {
            return Err(Failure::Binding);
        }
        for (binding, expected) in bindings.iter().zip(program.inputs) {
            let field = record(inv, binding.source)
                .fields
                .iter()
                .find(|f| f.id == binding.field)
                .ok_or(Failure::Binding)?;
            if domain(&field.leaf) != *expected {
                return Err(Failure::Binding);
            }
        }
        let mut decoded = [Vec::new(), Vec::new(), Vec::new()];
        for source in SOURCES {
            let i = source_index(source);
            let raw = record(inv, source);
            let (projected, next, report) =
                input_view::tests::oracle(raw.bytes, raw.fields, limits, used, attempts[i].clone());
            used = next;
            attempts[i] = report;
            decoded[i] = projected.map_err(|refusal| Failure::Record { source, refusal })?;
        }
        let input: Vec<_> = bindings
            .iter()
            .map(|binding| {
                let position = record(inv, binding.source)
                    .fields
                    .iter()
                    .position(|f| f.id == binding.field)
                    .unwrap_or_else(|| panic!("oracle metadata admitted unknown field"));
                decoded[source_index(binding.source)][position]
            })
            .collect();
        if !super::super::tests::domains_match(program.inputs, &input) {
            return Err(Failure::Execution(super::super::Failure::InputDomain));
        }
        let mut values = Vec::new();
        for node in program.nodes {
            let next = u128::from(used[7]) + 1;
            if next > u128::from(u64::MAX) || next > u128::from(limits[7]) {
                return Err(Failure::Execution(super::super::Failure::Budget(
                    MeterFailure {
                        resource: Resource::Step,
                        limit: limits[7],
                        attempted: next.min(u128::from(u64::MAX)) as u64,
                        overflow: next > u128::from(u64::MAX),
                    },
                )));
            }
            used[7] = next as u64;
            values.push(
                super::super::tests::reference_node(node, &input, &values)
                    .map_err(Failure::Execution)?,
            );
        }
        let output: Vec<_> = program
            .roots
            .iter()
            .map(|root| {
                values
                    .get(usize::from(*root))
                    .copied()
                    .ok_or(Failure::Execution(super::super::Failure::Reference))
            })
            .collect::<Result<_, _>>()?;
        if !super::super::tests::domains_match(program.outputs, &output) {
            return Err(Failure::Execution(super::super::Failure::OutputDomain));
        }
        Ok(output)
    })();
    (result, used, attempts)
}
fn compare(
    inv: &Invocation<'_>,
    program: &ScalarProgram<'_>,
    bindings: &[Binding],
    limits: [u64; 8],
    initial: [u64; 8],
    prefix: [Vec<AccessAttempt>; 3],
) {
    let expected = oracle(inv, program, bindings, limits, initial, prefix.clone());
    let mut meter = super::super::meter::Meter {
        limits: super::super::Limits { counters: limits },
        used: super::super::Usage { counters: initial },
    };
    let mut attempts = Attempts {
        state: prefix[0].clone(),
        command: prefix[1].clone(),
        context: prefix[2].clone(),
    };
    let mut output = std::vec![101, 202];
    let result = execute_into(
        inv,
        program,
        bindings,
        &mut meter,
        &mut output,
        &mut attempts,
    );
    let result = result.map(|()| output.clone());
    assert_eq!(result, expected.0);
    assert_eq!(meter.used.counters, expected.1);
    assert_eq!(meter.limits.counters, limits);
    assert_eq!(
        [attempts.state, attempts.command, attempts.context],
        expected.2
    );
    if result.is_err() {
        assert!(output.is_empty());
    }
    if initial == [0; 8] && prefix.iter().all(Vec::is_empty) {
        let (actual, usage, report) = execute(
            inv,
            program,
            bindings,
            super::super::Limits { counters: limits },
        )
        .into_parts();
        assert_eq!(actual, expected.0);
        assert_eq!(usage.counters, expected.1);
        assert_eq!([report.state, report.command, report.context], expected.2);
    }
}
fn empty_prefix() -> [Vec<AccessAttempt>; 3] {
    [Vec::new(), Vec::new(), Vec::new()]
}
fn encode(fields: &[Field], values: &[i64]) -> Vec<u8> {
    let mut bytes = std::vec![9];
    bytes.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    for (field, value) in fields.iter().zip(values) {
        bytes.extend_from_slice(&field.id.to_be_bytes());
        match &field.leaf {
            Leaf::I128 { .. } => {
                bytes.push(4);
                bytes.extend_from_slice(&i128::from(*value).to_be_bytes());
            }
            Leaf::Bool => bytes.push(if *value == 0 { 1 } else { 2 }),
            Leaf::Enum {
                type_id, variants, ..
            }
            | Leaf::Sum {
                type_id, variants, ..
            } => {
                let sum = matches!(field.leaf, Leaf::Sum { .. });
                bytes.push(if sum { 10 } else { 7 });
                bytes.extend_from_slice(&type_id.to_be_bytes());
                let variant = variants
                    .iter()
                    .find(|v| v.code == *value)
                    .unwrap_or_else(|| panic!("test mapping"));
                bytes.extend_from_slice(&variant.id.to_be_bytes());
                if sum {
                    bytes.push(0);
                }
            }
        }
    }
    bytes
}
fn invocation<'a>(bytes: &'a [Vec<u8>; 3], fields: &'a [Vec<Field>; 3]) -> Invocation<'a> {
    Invocation {
        state: RawRecord {
            bytes: &bytes[0],
            fields: &fields[0],
        },
        command: RawRecord {
            bytes: &bytes[1],
            fields: &fields[1],
        },
        context: RawRecord {
            bytes: &bytes[2],
            fields: &fields[2],
        },
    }
}
fn mixed() -> ([Vec<Field>; 3], [Vec<i64>; 3]) {
    (
        [
            std::vec![
                Field {
                    id: 7,
                    leaf: Leaf::I128 { min: -99, max: 99 }
                },
                Field {
                    id: 32,
                    leaf: Leaf::Bool
                }
            ],
            std::vec![Field {
                id: 7,
                leaf: Leaf::Enum {
                    type_id: u32::MAX,
                    min: -5,
                    max: -4,
                    variants: std::vec![
                        Variant {
                            id: u16::MAX,
                            code: -4
                        },
                        Variant { id: 0, code: -5 }
                    ]
                }
            }],
            std::vec![Field {
                id: u16::MAX,
                leaf: Leaf::Sum {
                    type_id: 0,
                    min: 55,
                    max: 56,
                    variants: std::vec![
                        Variant { id: 7, code: 56 },
                        Variant {
                            id: u16::MAX,
                            code: 55
                        }
                    ]
                }
            }],
        ],
        [std::vec![13, 1], std::vec![-4], std::vec![55]],
    )
}
fn copy_field(field: &Field) -> Field {
    let leaf = match &field.leaf {
        Leaf::Bool => Leaf::Bool,
        Leaf::I128 { min, max } => Leaf::I128 {
            min: *min,
            max: *max,
        },
        Leaf::Enum {
            type_id,
            min,
            max,
            variants,
        } => Leaf::Enum {
            type_id: *type_id,
            min: *min,
            max: *max,
            variants: variants.clone(),
        },
        Leaf::Sum {
            type_id,
            min,
            max,
            variants,
        } => Leaf::Sum {
            type_id: *type_id,
            min: *min,
            max: *max,
            variants: variants.clone(),
        },
    };
    Field { id: field.id, leaf }
}
fn permutations(size: usize) -> Vec<Vec<usize>> {
    fn visit(size: usize, prefix: &mut Vec<usize>, result: &mut Vec<Vec<usize>>) {
        if prefix.len() == size {
            result.push(prefix.clone());
            return;
        }
        for index in 0..size {
            if !prefix.contains(&index) {
                prefix.push(index);
                visit(size, prefix, result);
                prefix.pop();
            }
        }
    }
    let mut result = Vec::new();
    visit(size, &mut Vec::new(), &mut result);
    result
}
fn bindings(fields: &[Vec<Field>; 3]) -> Vec<Binding> {
    SOURCES
        .iter()
        .flat_map(|source| {
            fields[source_index(*source)].iter().map(|field| Binding {
                source: *source,
                field: field.id,
            })
        })
        .collect()
}
fn domains(fields: &[Vec<Field>; 3], bindings: &[Binding]) -> Vec<Domain> {
    bindings
        .iter()
        .map(|b| {
            domain(
                &fields[source_index(b.source)]
                    .iter()
                    .find(|f| f.id == b.field)
                    .unwrap_or_else(|| panic!("test field"))
                    .leaf,
            )
        })
        .collect()
}

#[test]
fn all_four_shapes_six_source_permutations_and_twenty_four_abi_orders() {
    let (groups, scalars) = mixed();
    let mut combinations = 0;
    for sources in permutations(3) {
        let fields = core::array::from_fn(|i| {
            groups[sources[i]]
                .iter()
                .map(copy_field)
                .collect::<Vec<_>>()
        });
        let values = [
            scalars[sources[0]].clone(),
            scalars[sources[1]].clone(),
            scalars[sources[2]].clone(),
        ];
        let bytes = core::array::from_fn(|i| encode(&fields[i], &values[i]));
        let inv = invocation(&bytes, &fields);
        let ordered = bindings(&fields);
        for order in permutations(4) {
            let chosen: Vec<_> = order.iter().map(|i| ordered[*i]).collect();
            let domains = domains(&fields, &chosen);
            let nodes = [Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)];
            let program = ScalarProgram {
                inputs: &domains,
                outputs: &domains,
                nodes: &nodes,
                roots: &[0, 1, 2, 3],
            };
            compare(
                &inv,
                &program,
                &chosen,
                [u64::MAX; 8],
                [0; 8],
                empty_prefix(),
            );
            let expected: Vec<_> = order
                .iter()
                .map(|i| values.iter().flatten().copied().collect::<Vec<_>>()[*i])
                .collect();
            assert_eq!(
                execute(
                    &inv,
                    &program,
                    &chosen,
                    super::super::Limits {
                        counters: [u64::MAX; 8]
                    }
                )
                .into_parts()
                .0,
                Ok(expected)
            );
            combinations += 1;
        }
    }
    assert_eq!(combinations, 144);
}

#[test]
fn equal_typed_equal_id_sources_are_distinct_and_every_declared_field_is_required() {
    let fields = [
        std::vec![Field {
            id: 7,
            leaf: Leaf::I128 { min: 0, max: 9 }
        }],
        std::vec![Field {
            id: 7,
            leaf: Leaf::I128 { min: 0, max: 9 }
        }],
        Vec::new(),
    ];
    let bytes = [
        encode(&fields[0], &[1]),
        encode(&fields[1], &[2]),
        encode(&fields[2], &[]),
    ];
    let inv = invocation(&bytes, &fields);
    let chosen = [
        Binding {
            source: Source::Command,
            field: 7,
        },
        Binding {
            source: Source::State,
            field: 7,
        },
    ];
    let types = [Domain::Int { min: 0, max: 9 }; 2];
    let program = ScalarProgram {
        inputs: &types,
        outputs: &types,
        nodes: &[Op::Input(0), Op::Input(1)],
        roots: &[0, 1],
    };
    assert_eq!(
        execute(
            &inv,
            &program,
            &chosen,
            super::super::Limits {
                counters: [u64::MAX; 8]
            }
        )
        .into_parts()
        .0,
        Ok(std::vec![2, 1])
    );
    compare(
        &inv,
        &program,
        &chosen,
        [u64::MAX; 8],
        [0; 8],
        empty_prefix(),
    );
    for bad in [
        std::vec![chosen[0]],
        std::vec![chosen[0], chosen[0]],
        std::vec![
            Binding {
                source: Source::Context,
                field: 7
            },
            chosen[1]
        ],
    ] {
        compare(&inv, &program, &bad, [0; 8], [0; 8], empty_prefix());
        assert_eq!(
            execute(
                &inv,
                &program,
                &bad,
                super::super::Limits { counters: [0; 8] }
            )
            .into_parts()
            .0,
            Err(Failure::Binding)
        );
    }
    let omitted = ScalarProgram {
        inputs: &types[..1],
        outputs: &[],
        nodes: &[],
        roots: &[],
    };
    assert_eq!(
        execute(
            &inv,
            &omitted,
            &chosen[..1],
            super::super::Limits {
                counters: [u64::MAX; 8]
            }
        )
        .into_parts()
        .0,
        Err(Failure::Binding)
    );
}

#[test]
fn malformed_records_quotas_and_eager_graphs_match_independent_oracles() {
    let (fields, values) = mixed();
    let bytes = core::array::from_fn(|i| encode(&fields[i], &values[i]));
    let chosen = bindings(&fields);
    let types = domains(&fields, &chosen);
    let program = ScalarProgram {
        inputs: &types,
        outputs: &types,
        nodes: &[Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)],
        roots: &[0, 1, 2, 3],
    };
    for resource in [0usize, 4, 7] {
        for quota in 0..=bytes.iter().map(Vec::len).sum::<usize>() as u64 + 1 {
            let mut limits = [u64::MAX; 8];
            limits[resource] = quota;
            compare(
                &invocation(&bytes, &fields),
                &program,
                &chosen,
                limits,
                [0; 8],
                empty_prefix(),
            );
        }
    }
    for source in 0..3 {
        let mut corrupt = bytes.clone();
        for count in 0..bytes[source].len() {
            corrupt[source] = bytes[source][..count].to_vec();
            compare(
                &invocation(&corrupt, &fields),
                &program,
                &chosen,
                [u64::MAX; 8],
                [0; 8],
                empty_prefix(),
            );
        }
        for position in 0..bytes[source].len() {
            corrupt[source] = bytes[source].clone();
            corrupt[source][position] ^= 0xff;
            compare(
                &invocation(&corrupt, &fields),
                &program,
                &chosen,
                [u64::MAX; 8],
                [0; 8],
                empty_prefix(),
            );
        }
        corrupt[source] = bytes[source].clone();
        corrupt[source].push(0);
        compare(
            &invocation(&corrupt, &fields),
            &program,
            &chosen,
            [u64::MAX; 8],
            [0; 8],
            empty_prefix(),
        );
    }
    for graph in [
        std::vec![Op::Input(u16::MAX)],
        std::vec![Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1)],
        std::vec![
            Op::Bool(false),
            Op::Int(9),
            Op::Int(i64::MIN),
            Op::Sub(2, 1),
            Op::Select(0, 3, 1)
        ],
        std::vec![Op::Not(u16::MAX)],
        std::vec![Op::Int(1)],
    ] {
        for root in [0, u16::MAX] {
            let bad = ScalarProgram {
                inputs: &types,
                outputs: &[Domain::Bool],
                nodes: &graph,
                roots: &[root],
            };
            for quota in 0..=graph.len() as u64 + 1 {
                let mut limits = [u64::MAX; 8];
                limits[7] = quota;
                compare(
                    &invocation(&bytes, &fields),
                    &bad,
                    &chosen,
                    limits,
                    [0; 8],
                    empty_prefix(),
                );
            }
        }
    }
}

#[test]
fn metadata_precedes_raw_access_and_domains_match_complete_intervals() {
    let (fields, values) = mixed();
    let bytes = core::array::from_fn(|i| encode(&fields[i], &values[i]));
    let chosen = bindings(&fields);
    let types = domains(&fields, &chosen);
    let program = ScalarProgram {
        inputs: &types,
        outputs: &[],
        nodes: &[],
        roots: &[],
    };
    for source in 0..3 {
        let mut invalid =
            core::array::from_fn(|i| fields[i].iter().map(copy_field).collect::<Vec<_>>());
        let duplicate = copy_field(&invalid[source][0]);
        invalid[source].push(duplicate);
        let malformed = [Vec::new(), Vec::new(), Vec::new()];
        let inv = invocation(&malformed, &invalid);
        compare(&inv, &program, &chosen, [0; 8], [0; 8], empty_prefix());
        let outcome = execute(
            &inv,
            &program,
            &chosen,
            super::super::Limits { counters: [0; 8] },
        );
        assert_eq!(
            outcome.into_parts().0,
            Err(Failure::Schema(SOURCES[source]))
        );
    }
    for (position, wrong) in [
        (0, Domain::Int { min: -99, max: 98 }),
        (1, Domain::Int { min: 0, max: 1 }),
        (2, Domain::Int { min: -4, max: -4 }),
        (3, Domain::Int { min: 0, max: 100 }),
    ] {
        let mut narrowed = types.clone();
        narrowed[position] = wrong;
        let bad = ScalarProgram {
            inputs: &narrowed,
            outputs: &[],
            nodes: &[],
            roots: &[],
        };
        compare(
            &invocation(&bytes, &fields),
            &bad,
            &chosen,
            [0; 8],
            [0; 8],
            empty_prefix(),
        );
        assert_eq!(
            execute(
                &invocation(&bytes, &fields),
                &bad,
                &chosen,
                super::super::Limits { counters: [0; 8] }
            )
            .into_parts()
            .0,
            Err(Failure::Binding)
        );
    }
    // A constant graph must still read an unused declared malformed field.
    let mut corrupt = bytes.clone();
    corrupt[2].clear();
    let constant = ScalarProgram {
        inputs: &types,
        outputs: &[Domain::Int { min: 1, max: 1 }],
        nodes: &[Op::Int(1)],
        roots: &[0],
    };
    assert_eq!(
        execute(
            &invocation(&corrupt, &fields),
            &constant,
            &chosen,
            super::super::Limits {
                counters: [u64::MAX; 8]
            }
        )
        .into_parts()
        .0,
        Err(Failure::Record {
            source: Source::Context,
            refusal: input_view::Failure::Header
        })
    );
}

#[test]
fn private_initial_counters_and_source_prefixes_survive_every_refusal() {
    let (fields, values) = mixed();
    let bytes = core::array::from_fn(|i| encode(&fields[i], &values[i]));
    let chosen = bindings(&fields);
    let types = domains(&fields, &chosen);
    let program = ScalarProgram {
        inputs: &types,
        outputs: &types,
        nodes: &[Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)],
        roots: &[0, 1, 2, 3],
    };
    let prefix_fields = [Field {
        id: 123,
        leaf: Leaf::Bool,
    }];
    let prefix_bytes = encode(&prefix_fields, &[1]);
    let prefix = core::array::from_fn(|index| {
        input_view::tests::oracle(
            &prefix_bytes,
            &prefix_fields,
            if index == 1 { [0; 8] } else { [u64::MAX; 8] },
            [0; 8],
            Vec::new(),
        )
        .2
    });
    for initial in [
        [1, 2, 3, 4, 5, 6, 7, 8],
        [u64::MAX; 8],
        [0, 11, 12, 13, u64::MAX, 15, 16, 0],
        [u64::MAX, 11, 12, 13, 0, 15, 16, 0],
        [0, 11, 12, 13, 0, 15, 16, u64::MAX],
    ] {
        for resource in [0usize, 4, 7] {
            for quota in [0, 1, 5, 100, u64::MAX] {
                let mut limits = [u64::MAX; 8];
                limits[resource] = quota;
                compare(
                    &invocation(&bytes, &fields),
                    &program,
                    &chosen,
                    limits,
                    initial,
                    prefix.clone(),
                );
            }
        }
        compare(
            &invocation(&bytes, &fields),
            &program,
            &[],
            [0; 8],
            initial,
            prefix.clone(),
        );
    }
    let empty_fields = [Vec::new(), Vec::new(), Vec::new()];
    let empty_bytes = core::array::from_fn(|_| std::vec![9, 0, 0, 0, 0]);
    let empty_program = ScalarProgram {
        inputs: &[],
        outputs: &[],
        nodes: &[],
        roots: &[],
    };
    let mut initial = [0; 8];
    initial[4] = u64::MAX - 10;
    compare(
        &invocation(&empty_bytes, &empty_fields),
        &empty_program,
        &[],
        [u64::MAX; 8],
        initial,
        prefix.clone(),
    );
    let outcome = oracle(
        &invocation(&empty_bytes, &empty_fields),
        &empty_program,
        &[],
        [u64::MAX; 8],
        initial,
        prefix.clone(),
    );
    assert!(
        matches!(outcome.0,Err(Failure::Record { source:Source::Context,refusal:input_view::Failure::Budget(error) }) if error.overflow)
    );
    assert_eq!(outcome.1[4], u64::MAX);
    initial[4] = 1;
    let mut limits = [0; 8];
    limits[4] = 0;
    let raw_empty = [Vec::new(), Vec::new(), Vec::new()];
    compare(
        &invocation(&raw_empty, &empty_fields),
        &empty_program,
        &[],
        limits,
        initial,
        prefix,
    );
}

#[test]
fn empty_thirty_two_and_one_hundred_field_records_have_no_new_bound() {
    for count in [0usize, 32, 100] {
        let fields = [
            (0..count)
                .map(|id| Field {
                    id: id as u16,
                    leaf: Leaf::I128 {
                        min: -100,
                        max: 100,
                    },
                })
                .collect::<Vec<_>>(),
            Vec::new(),
            Vec::new(),
        ];
        let values = (0..count).map(|value| value as i64).collect::<Vec<_>>();
        let bytes = [
            encode(&fields[0], &values),
            encode(&fields[1], &[]),
            encode(&fields[2], &[]),
        ];
        let chosen = bindings(&fields);
        let types = domains(&fields, &chosen);
        let nodes = (0..count).map(|i| Op::Input(i as u16)).collect::<Vec<_>>();
        let roots = (0..count).map(|i| i as u16).collect::<Vec<_>>();
        let program = ScalarProgram {
            inputs: &types,
            outputs: &types,
            nodes: &nodes,
            roots: &roots,
        };
        compare(
            &invocation(&bytes, &fields),
            &program,
            &chosen,
            [u64::MAX; 8],
            [0; 8],
            empty_prefix(),
        );
        let outcome = execute(
            &invocation(&bytes, &fields),
            &program,
            &chosen,
            super::super::Limits {
                counters: [u64::MAX; 8],
            },
        );
        assert_eq!(
            outcome.usage().used(Resource::Byte),
            bytes.iter().map(|b| b.len() as u64).sum()
        );
        assert_eq!(outcome.usage().used(Resource::Read), count as u64);
        assert_eq!(outcome.usage().used(Resource::Step), count as u64);
        assert_eq!(outcome.into_parts().0, Ok(values));
    }
}
