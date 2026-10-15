//! Data-migrating upgrades of spend-approval stores. Version 1 is the
//! spend-approval contract and version 2 the spend-approval-priority one,
//! whose state adds the urgent flag, both from the committed migrated
//! application fixture; its `STATE_STEPS` declares the migration
//! `zeno-fcis contract evolve --migration` recorded between them.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Envelope, Hash32, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::{
    FieldDef, FieldId, Schema, SchemaLimits, SumVariantDef, TypeDef, TypeId, TypeKind, VariantId,
};
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::v2::{
    Error, Lineage, Opened, Step, Store, Superseded, V2SqliteShell,
    migration::{self, Migration, Observation, Source, Target, Unsimulated},
    upgrade,
};
use zeno_fcis_synthesis::finite::{
    Domain as ScalarDomain, Op, Program, V2ScalarProgram,
    canonical_v2::schema as s,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog::{self as catalog, BoundCatalog},
    v2_composition::{self as c, Raw},
};
use zeno_fcis_value::{Field, Value};

/// The migrated spend-approval application: `migrated::v1` is version 1.
#[allow(dead_code, clippy::type_complexity)]
#[path = "../../zeno-fcis-cli/tests/fixtures/spend-approval-migrated/src/v2_contract.rs"]
pub mod migrated;

#[track_caller]
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected refusal: {error:?}"),
    }
}

#[track_caller]
fn refused<T>(result: Result<T, Error>) -> Error {
    match result {
        Ok(_) => panic!("expected a refusal"),
        Err(error) => error,
    }
}

/// Runs `test` with both versions' checked catalogs.
fn with_versions(test: impl FnOnce(&BoundCatalog<'_>, &BoundCatalog<'_>)) {
    let contract_1 = migrated::v1::Contract::new();
    let descriptor_1 = contract_1.descriptor();
    let v1 = ok(migrated::v1::checked_catalog(&descriptor_1));
    let contract_2 = migrated::Contract::new();
    let descriptor_2 = contract_2.descriptor();
    let v2 = ok(migrated::checked_catalog(&descriptor_2));
    test(&v1, &v2);
}

#[derive(Clone, Copy)]
enum SchemaChange {
    Destination { bytes: bool, max: u32 },
    PayloadType { max: i128 },
    PayloadName,
}

fn schema_name(bytes: &[u8]) -> &str {
    ok(std::str::from_utf8(bytes))
}

fn destination_lane<'a>(plans: &[c::DeliveryPlan<'a>], bytes: bool) -> Vec<c::DeliveryPlan<'a>> {
    plans
        .iter()
        .map(|plan| {
            let destination = match (bytes, plan.destination) {
                (true, c::Expr::Constant(c::Atom::Text(value))) => {
                    c::Expr::Constant(c::Atom::Bytes(value))
                }
                _ => plan.destination,
            };
            c::DeliveryPlan {
                destination,
                ..*plan
            }
        })
        .collect()
}

/// Rebind actual original schema bytes and the complete immutable descriptor.
/// The schema crate is the independent canonical encoding oracle here.
fn with_schema_change(
    source: &BoundCatalog<'_>,
    change: SchemaChange,
    test: impl FnOnce(&BoundCatalog<'_>),
) {
    let bytes = matches!(change, SchemaChange::Destination { bytes: true, .. });
    let alias = [s::Field {
        id: 145,
        name: b"payment_tier",
        type_id: 111,
    }];
    let renamed = [s::Field {
        id: 145,
        name: b"renamed_tier",
        type_id: 105,
    }];
    let mut definitions = source.original_description().definitions.to_vec();
    for definition in &mut definitions {
        match (&change, definition.id) {
            (SchemaChange::Destination { bytes, max }, 103) => {
                definition.kind = if *bytes {
                    s::Kind::Bytes { min: 1, max: *max }
                } else {
                    s::Kind::Text { min: 1, max: *max }
                };
            }
            (SchemaChange::PayloadType { .. }, 104) => definition.kind = s::Kind::Record(&alias),
            (SchemaChange::PayloadName, 104) => {
                definition.name = b"RenamedPayment";
                definition.kind = s::Kind::Record(&renamed);
            }
            _ => {}
        }
    }
    if let SchemaChange::PayloadType { max } = change {
        definitions.push(s::Definition {
            id: 111,
            name: b"TierAlias",
            kind: s::Kind::I128 { min: 0, max },
        });
    }
    let description = s::Description {
        definitions: &definitions,
        ..*source.original_description()
    };
    let limits = SchemaLimits::default();
    let types = definitions
        .iter()
        .map(|definition| {
            let kind = match definition.kind {
                s::Kind::Bool => TypeKind::Bool,
                s::Kind::I128 { min, max } => TypeKind::I128 { min, max },
                s::Kind::Bytes { min, max } => TypeKind::Bytes {
                    min_len: min,
                    max_len: max,
                },
                s::Kind::Text { min, max } => TypeKind::Text {
                    min_len: min,
                    max_len: max,
                },
                s::Kind::Record(fields) => TypeKind::Record {
                    fields: fields
                        .iter()
                        .map(|field| {
                            ok(FieldDef::try_new(
                                FieldId::new(field.id),
                                schema_name(field.name),
                                TypeId::new(field.type_id),
                            ))
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                },
                s::Kind::Sum(variants) => TypeKind::Sum {
                    variants: variants
                        .iter()
                        .map(|variant| {
                            ok(SumVariantDef::try_new(
                                VariantId::new(variant.id),
                                schema_name(variant.name),
                                None,
                            ))
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                },
                other => panic!("unexpected migrated fixture kind: {other:?}"),
            };
            ok(TypeDef::try_new(
                TypeId::new(definition.id),
                schema_name(definition.name),
                kind,
                limits,
            ))
        })
        .collect();
    let schema = ok(Schema::try_new(
        ok(std::str::from_utf8(description.profile)),
        description.version,
        TypeId::new(description.root),
        types,
        limits,
    ));
    let original = ok(schema.canonical_bytes());
    let hash = ok(schema.schema_hash::<RustCryptoSha256>());
    let mut framing = *source.framing();
    framing.state.schema = *hash.as_bytes();
    framing.command.schema = *hash.as_bytes();
    framing.context.schema = *hash.as_bytes();
    let old = source.descriptor();
    let payload = [c::TypedField {
        field: 145,
        domain: c::Domain::I128 {
            min: 0,
            max: match change {
                SchemaChange::PayloadType { max } => max,
                _ => 3,
            },
        },
    }];
    let channels = old
        .channels
        .iter()
        .map(|channel| c::Channel {
            destination: if bytes {
                c::Domain::Bytes
            } else {
                channel.destination
            },
            payload: if matches!(change, SchemaChange::PayloadType { .. }) && channel.id == 300 {
                &payload
            } else {
                channel.payload
            },
            ..*channel
        })
        .collect::<Vec<_>>();
    let effects = old
        .branches
        .iter()
        .map(|branch| destination_lane(branch.effects, bytes))
        .collect::<Vec<_>>();
    let outboxes = old
        .branches
        .iter()
        .map(|branch| destination_lane(branch.outbox, bytes))
        .collect::<Vec<_>>();
    let branches = old
        .branches
        .iter()
        .enumerate()
        .map(|(index, branch)| c::Branch {
            effects: &effects[index],
            outbox: &outboxes[index],
            ..*branch
        })
        .collect::<Vec<_>>();
    let descriptor = c::Descriptor {
        state: old.state,
        command: old.command,
        context: old.context,
        program: V2ScalarProgram {
            inputs: old.program.inputs,
            outputs: old.program.outputs,
            nodes: old.program.nodes,
            roots: old.program.roots,
        },
        bindings: old.bindings,
        output_types: old.output_types,
        decision_output: old.decision_output,
        branches: &branches,
        reasons: old.reasons,
        channels: &channels,
        laws: old.laws,
        required: old.required,
        limits: old.limits,
    };
    let policy = authority::policy_bytes(&descriptor, &original, &framing, source.channel_roots())
        .unwrap_or_else(|| panic!("complete original policy encoding"));
    let bound = ok(catalog::bind_original(
        &original,
        &description,
        catalog::Limits {
            schema: s::Limits {
                bytes: u64::MAX,
                types: u32::MAX,
                fields: u32::MAX,
                variants: u32::MAX,
            },
            contract_bytes: u64::MAX,
        },
        &policy,
        &descriptor,
        &framing,
        source.channel_roots(),
    ));
    test(&bound);
}

fn refused_schema_upgrade(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>) -> Error {
    let fields = declared();
    let steps = [Step::Migration {
        migration: Migration { fields: &fields },
        claims: &[],
        review: b"schema comparison regression",
    }];
    let lineage = ok(Lineage::bind_steps(&[from, to], &steps));
    let file = StoreFile::new();
    let initial = request(from.framing(), EMPTY, 0, false, None);
    let genesis = match lineage.authorities()[0].publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => panic!("original genesis refused: {other:?}"),
    };
    drop(ok(V2SqliteShell::create(
        &file.0,
        &lineage.authorities()[0],
        genesis,
    )));
    let before = ok(fs::read(&file.0));
    let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
    assert_eq!(ok(fs::read(&file.0)), before);
    error
}

#[test]
fn projected_destination_domains_do_not_erase_original_text_or_bytes_bounds() {
    with_versions(|v1, v2| {
        for bytes in [false, true] {
            with_schema_change(v1, SchemaChange::Destination { bytes, max: 32 }, |old| {
                with_schema_change(v2, SchemaChange::Destination { bytes, max: 31 }, |new| {
                    let domains = (
                        old.descriptor().channels[0].destination,
                        new.descriptor().channels[0].destination,
                    );
                    assert!(matches!(
                        domains,
                        (c::Domain::Text, c::Domain::Text) | (c::Domain::Bytes, c::Domain::Bytes)
                    ));
                    assert_eq!(old.channel_roots(), new.channel_roots());
                    assert!(matches!(
                        refused_schema_upgrade(old, new),
                        Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Shape))
                    ));
                });
            });
        }
    });
}

#[test]
fn original_payload_type_ids_need_a_map_but_names_allow_legacy_migration() {
    with_versions(|v1, v2| {
        with_schema_change(v2, SchemaChange::PayloadType { max: 3 }, |alias| {
            assert!(matches!(
                (
                    v1.descriptor().channels[0].payload[0].domain,
                    alias.descriptor().channels[0].payload[0].domain
                ),
                (
                    c::Domain::I128 { min: 0, max: 3 },
                    c::Domain::I128 { min: 0, max: 3 }
                )
            ));
            assert_eq!(v1.channel_roots(), alias.channel_roots());
            assert!(matches!(
                refused_schema_upgrade(v1, alias),
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Payload {
                    channel: 300
                }))
            ));
            let fields = declared();
            let payment = [Target {
                id: 145,
                source: Source::Field(145),
            }];
            let payloads = [migration::Payload {
                channel: 300,
                fields: &payment,
            }];
            let old_authority = ok(authority::bind(v1));
            let new_authority = ok(authority::bind(alias));
            assert!(
                migration::simulate_with_payloads(
                    v1,
                    &old_authority,
                    alias,
                    &new_authority,
                    &Migration { fields: &fields },
                    &payloads
                )
                .is_ok()
            );
        });
        with_schema_change(v2, SchemaChange::PayloadName, |renamed| {
            let fields = declared();
            let old_authority = ok(authority::bind(v1));
            let new_authority = ok(authority::bind(renamed));
            assert!(
                migration::simulate(
                    v1,
                    &old_authority,
                    renamed,
                    &new_authority,
                    &Migration { fields: &fields }
                )
                .is_ok()
            );
            assert_eq!(
                ok(Migration { fields: &fields }.encode())[b"ZFCIS-MIGRATION\0".len()],
                1
            );
        });
    });
}

#[test]
fn explicit_payload_map_cap_refuses_without_partial_admission_or_writes() {
    with_versions(|v1, v2| {
        let change = SchemaChange::PayloadType {
            max: i128::from(migration::MAX_PAYLOAD_TUPLES),
        };
        with_schema_change(v1, change, |old| {
            with_schema_change(v2, change, |new| {
                let fields = declared();
                let payment = [Target {
                    id: 145,
                    source: Source::Field(145),
                }];
                let payloads = [migration::Payload {
                    channel: 300,
                    fields: &payment,
                }];
                let steps = [Step::PayloadMigration {
                    migration: Migration { fields: &fields },
                    payloads: &payloads,
                    claims: &[],
                    review: b"explicit map cap",
                }];
                let lineage = ok(Lineage::bind_steps(&[old, new], &steps));
                let file = StoreFile::new();
                let initial = request(old.framing(), EMPTY, 0, false, None);
                let genesis = match lineage.authorities()[0].publish_genesis(&initial) {
                    PublicationOutcome::Commit(publication) => publication,
                    other => panic!("original genesis refused: {other:?}"),
                };
                drop(ok(V2SqliteShell::create(
                    &file.0,
                    &lineage.authorities()[0],
                    genesis,
                )));
                let before = ok(fs::read(&file.0));
                assert!(matches!(
                    refused(superseded(ok(lineage.open(&file.0))).upgrade()),
                    Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Payload {
                        channel: 300
                    }))
                ));
                assert_eq!(ok(fs::read(&file.0)), before);
            });
        });
    });
}

/// The migration the application declares, as the shell takes it.
fn declared() -> Vec<Target<'static>> {
    let Some((1, Some(fields))) = migrated::STATE_STEPS.first().copied() else {
        panic!("the fixture declares one migration after version 1");
    };
    fields
        .iter()
        .map(|(id, tag, operand, table)| Target {
            id: *id,
            source: match tag {
                0 => Source::Field(u16::try_from(*operand).unwrap_or_else(|_| panic!("field"))),
                1 => Source::Value(*operand),
                _ => Source::Table {
                    field: u16::try_from(*operand).unwrap_or_else(|_| panic!("field")),
                    cases: table,
                },
            },
        })
        .collect()
}

/// The declared migration with the urgent flag's source replaced.
fn with_urgent(source: Source<'_>) -> Vec<Target<'_>> {
    declared()
        .into_iter()
        .map(|target| {
            if target.id == 124 {
                Target { id: 124, source }
            } else {
                target
            }
        })
        .collect()
}

#[test]
fn explicit_payload_metadata_and_split_programs_bind_reopen_and_relay() {
    with_versions(|v1, v2| {
        let program = ok(Program::try_new_extended(
            vec![ScalarDomain::Int { min: 0, max: 3 }],
            vec![ScalarDomain::Bool],
            vec![Op::Input(0), Op::Int(0), Op::Lt(0, 1)],
            vec![2],
        ));
        let fields = with_urgent(Source::Program {
            fields: &[121],
            program: &program,
        });
        let payment = [Target {
            id: 145,
            source: Source::Field(145),
        }];
        let payloads = [migration::Payload {
            channel: 300,
            fields: &payment,
        }];
        let steps = [Step::PayloadMigration {
            migration: Migration { fields: &fields },
            payloads: &payloads,
            claims: &[],
            review: b"same tier; explicit lossless representation",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 3);
        let original = {
            let mut old = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
            ok(old.export_pending())
        };
        let history = ok(lineage.audit_history_read_only(&file.0));
        let (mut shell, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(ok(shell.export_pending()), original);
        assert_eq!(
            receipt
                .migration()
                .unwrap_or_else(|| panic!("migration"))
                .migration(),
            <RustCryptoSha256 as CommitmentHasher>::hash(&ok(migration::encode_with_payloads(
                &Migration { fields: &fields },
                &payloads
            )))
        );
        drop(shell);

        // A fresh lineage revalidates the complete description before export
        // and acknowledgment through the old delivery's historical contract.
        let fresh = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let mut reopened = current(ok(fresh.open(&file.0)));
        assert_eq!(ok(reopened.export_pending()), original);
        let delivery = original.first().unwrap_or_else(|| panic!("payment"));
        ok(zeno_fcis_shell_sqlite::v2::relay::acknowledge(
            &mut reopened,
            delivery.delivery_id(),
            zeno_fcis_shell_sqlite::v2::relay::payload_sha256(delivery.payload()),
        ));
        assert!(ok(reopened.export_pending()).is_empty());
        drop(reopened);
        let after = ok(fresh.audit_history_read_only(&file.0));
        assert_eq!(after.commits, history.commits);
        assert_eq!(after.genesis, history.genesis);

        // Another total/injective map with different canonical program bytes
        // does not authorize replay of the upgrade record already stored.
        let equivalent = ok(Program::try_new_extended(
            vec![ScalarDomain::Int { min: 0, max: 3 }],
            vec![ScalarDomain::Bool],
            vec![Op::Input(0), Op::Int(4), Op::Lt(1, 0)],
            vec![2],
        ));
        let other = with_urgent(Source::Program {
            fields: &[121],
            program: &equivalent,
        });
        let changed = [Step::PayloadMigration {
            migration: Migration { fields: &other },
            payloads: &payloads,
            claims: &[],
            review: b"same tier; explicit lossless representation",
        }];
        let changed = ok(Lineage::bind_steps(&[v1, v2], &changed));
        let bytes = ok(fs::read(&file.0));
        assert!(matches!(
            refused(changed.audit_read_only(&file.0).map(|_| ())),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        assert_eq!(ok(fs::read(&file.0)), bytes);
    });
}

#[test]
fn invalid_payload_maps_and_program_domains_refuse_without_writes() {
    with_versions(|v1, v2| {
        let fields = declared();
        let file = StoreFile::new();
        let setup = ok(Lineage::bind_steps(
            &[v1, v2],
            &[Step::Migration {
                migration: Migration { fields: &fields },
                claims: &[],
                review: b"setup",
            }],
        ));
        store_under(&file.0, &setup.authorities()[0], 3);
        let before = ok(fs::read(&file.0));
        let lossy = [Target {
            id: 145,
            source: Source::Table {
                field: 145,
                cases: &[(0, 0), (1, 0), (2, 0), (3, 0)],
            },
        }];
        let partial = [Target {
            id: 145,
            source: Source::Table {
                field: 145,
                cases: &[(0, 0), (1, 1), (2, 2)],
            },
        }];
        for payload_fields in [&lossy[..], &partial[..]] {
            let payloads = [migration::Payload {
                channel: 300,
                fields: payload_fields,
            }];
            let steps = [Step::PayloadMigration {
                migration: Migration { fields: &fields },
                payloads: &payloads,
                claims: &[],
                review: b"invalid",
            }];
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
            assert!(matches!(
                refused(superseded(ok(lineage.open(&file.0))).upgrade()),
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Payload {
                    channel: 300
                }))
            ));
            assert_eq!(ok(fs::read(&file.0)), before);
        }
        for input in [
            ScalarDomain::Int { min: 0, max: 2 },
            ScalarDomain::Int { min: 0, max: 3 },
        ] {
            let nodes = if input.bounds().1 == 2 {
                vec![Op::Input(0), Op::Int(0), Op::Lt(0, 1)]
            } else {
                vec![
                    Op::Input(0),
                    Op::Int(i64::MAX),
                    Op::Add(0, 1),
                    Op::Int(0),
                    Op::Lt(2, 3),
                ]
            };
            let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| panic!("root"));
            let program = ok(Program::try_new_extended(
                vec![input],
                vec![ScalarDomain::Bool],
                nodes,
                vec![root],
            ));
            let invalid = with_urgent(Source::Program {
                fields: &[121],
                program: &program,
            });
            let lineage = ok(Lineage::bind_steps(
                &[v1, v2],
                &[Step::Migration {
                    migration: Migration { fields: &invalid },
                    claims: &[],
                    review: b"invalid",
                }],
            ));
            assert!(matches!(
                refused(superseded(ok(lineage.open(&file.0))).upgrade()),
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Migration(
                    migration::Defect::Program { field: 124 }
                )))
            ));
            assert_eq!(ok(fs::read(&file.0)), before);
        }
    });
}

#[test]
fn split_programs_check_source_tuples_excluded_by_every_state_law() {
    with_versions(|v1, v2| {
        let fields = declared();
        let setup = ok(Lineage::bind_steps(
            &[v1, v2],
            &[Step::Migration {
                migration: Migration { fields: &fields },
                claims: &[],
                review: b"setup",
            }],
        ));
        let file = StoreFile::new();
        store_under(&file.0, &setup.authorities()[0], 3);
        let before = ok(fs::read(&file.0));
        // Empty/tier>0 violates the old state law. All admitted states map
        // identically, but these declared source tuples trap or leave the
        // output domain, so neither program may be admitted.
        let condition = vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(150),
            Op::Eq(0, 2),
            Op::Int(0),
            Op::Lt(4, 1),
            Op::And(3, 5),
        ];
        for (field, output, suffix, root) in [
            (
                124,
                ScalarDomain::Bool,
                vec![
                    Op::Int(i64::MAX),
                    Op::Select(6, 7, 4),
                    Op::Add(8, 1),
                    Op::Lt(9, 4),
                ],
                10,
            ),
            (
                121,
                ScalarDomain::Int { min: 0, max: 3 },
                vec![Op::Int(4), Op::Select(6, 7, 1)],
                8,
            ),
        ] {
            let mut nodes = condition.clone();
            nodes.extend(suffix);
            let program = ok(Program::try_new_extended(
                vec![
                    ScalarDomain::Int { min: 150, max: 154 },
                    ScalarDomain::Int { min: 0, max: 3 },
                ],
                vec![output],
                nodes,
                vec![root],
            ));
            assert!(program.evaluate(&[150, 1]).is_err());
            assert!(program.evaluate(&[151, 1]).is_ok());
            let fields: Vec<Target<'_>> = declared()
                .into_iter()
                .map(|target| {
                    if target.id == field {
                        Target {
                            id: field,
                            source: Source::Program {
                                fields: &[120, 121],
                                program: &program,
                            },
                        }
                    } else {
                        target
                    }
                })
                .collect();
            let lineage = ok(Lineage::bind_steps(
                &[v1, v2],
                &[Step::Migration {
                    migration: Migration { fields: &fields },
                    claims: &[],
                    review: b"invalid source tuple",
                }],
            ));
            assert!(
                matches!(refused(superseded(ok(lineage.open(&file.0))).upgrade()),
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Migration(migration::Defect::Program { field: failed }))) if failed == field)
            );
            assert_eq!(ok(fs::read(&file.0)), before);
        }
    });
}

fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

const EMPTY: u16 = 150;
const PENDING: u16 = 151;
const EXECUTED: u16 = 152;
const CLERK: u16 = 161;
const CFO: u16 = 162;
const CREATE: u16 = 170;
const APPROVE: u16 = 171;
const EXECUTE: u16 = 172;

/// A request under version 1, or under version 2 with the urgent flag.
fn request(
    framing: &c::Framing,
    status: u16,
    tier: i128,
    cfo: bool,
    urgent: Option<bool>,
) -> Vec<u8> {
    let mut fields = vec![
        Field::new(120, Value::sum(106, status, None)),
        Field::new(121, Value::signed(tier)),
        Field::new(122, Value::boolean(cfo)),
        Field::new(123, Value::boolean(false)),
    ];
    if let Some(urgent) = urgent {
        fields.push(Field::new(124, Value::boolean(urgent)));
    }
    envelope(framing.state, ok(Value::record_canonical(fields)))
}
fn command(framing: &c::Framing, action: u16, tier: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(130, Value::sum(109, action, None)),
        Field::new(131, Value::signed(tier)),
    ]));
    envelope(framing.command, value)
}
fn context(framing: &c::Framing, role: u16) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(140, Value::sum(107, role, None)),
        Field::new(141, Value::boolean(false)),
        Field::new(142, Value::boolean(true)),
    ]));
    envelope(framing.context, value)
}
fn publication<'a>(
    a: &'a Authority<'_>,
    state: &'a [u8],
    command: &'a [u8],
    context: &'a [u8],
) -> Publication<'a> {
    match a.publish(Raw {
        state,
        command,
        context,
    }) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("publication refused: {refused:?}"),
    }
}
fn key(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}
fn root(state: &[u8]) -> Hash32 {
    ok(commitment::<RustCryptoSha256>(domains::V2_STATE, state))
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zenofcis-v2-migration-{}-{}.db",
            std::process::id(),
            FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        Self(path)
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The first `steps` commits under version 1: the clerk creates a tier 1
/// request, the CFO approves it and the clerk executes it, which leaves the
/// payment pending.
fn store_under(path: &Path, v1: &Authority<'_>, steps: usize) {
    let framing = &migrated::v1::FRAMING;
    let initial = request(framing, EMPTY, 0, false, None);
    let genesis = match v1.publish_genesis(&initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    };
    let mut db = ok(V2SqliteShell::create(path, v1, genesis));
    let session = [
        (
            request(framing, EMPTY, 0, false, None),
            command(framing, CREATE, 1),
            context(framing, CLERK),
        ),
        (
            request(framing, PENDING, 1, false, None),
            command(framing, APPROVE, 0),
            context(framing, CFO),
        ),
        (
            request(framing, PENDING, 1, true, None),
            command(framing, EXECUTE, 0),
            context(framing, CLERK),
        ),
    ];
    for (index, (state, cmd, ctx)) in session.iter().take(steps).enumerate() {
        let receipt = ok(db.commit(key(index as u8 + 1), publication(v1, state, cmd, ctx)));
        assert_eq!(receipt.status(), CommitStatus::Committed);
    }
}

#[track_caller]
fn superseded<'a, 'p>(opened: Opened<'a, 'p>) -> Superseded<'a, 'p> {
    match opened {
        Opened::V10(Store::Superseded(store)) => store,
        Opened::V10(Store::Current(_)) => panic!("the store is current"),
        Opened::V9(_) => panic!("a v9 store"),
    }
}
#[track_caller]
fn current<'a, 'p>(opened: Opened<'a, 'p>) -> V2SqliteShell<'a, 'p> {
    match opened {
        Opened::V10(Store::Current(shell)) => shell,
        Opened::V10(Store::Superseded(store)) => {
            panic!("the store runs version {}", store.version())
        }
        Opened::V9(_) => panic!("a v9 store"),
    }
}

#[test]
fn a_declared_migration_moves_a_live_store_and_keeps_its_pending_deliveries() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 3);
        let (pending, checkpoint) = {
            let mut shell = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
            let checkpoint = ok(shell.checkpoint());
            let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("a pending payment"));
            (token.delivery().delivery_id(), checkpoint)
        };
        // The new build audits the old store without writing to it.
        let before = ok(fs::read(&file.0));
        let (version, head) = ok(lineage.audit_read_only(&file.0));
        assert_eq!((version, head.version(), head.pending()), (1, 3, 1));
        assert_eq!(ok(fs::read(&file.0)), before);
        let original_history = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(original_history.genesis_contract_version, 1);
        assert_eq!(original_history.commits.len(), 3);
        assert_eq!(
            original_history.commits[0].prestate,
            original_history.genesis
        );
        assert!(
            original_history
                .commits
                .iter()
                .all(|commit| commit.contract_version == 1)
        );
        assert_eq!(ok(fs::read(&file.0)), before);

        let store = superseded(ok(lineage.open(&file.0)));
        let (mut shell, receipt) = ok(store.upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::Migration);
        assert_eq!(receipt.kind().tag(), "migration");
        assert_eq!((receipt.ordinal(), receipt.version()), (1, 3));
        assert!(receipt.premises().is_none() && receipt.behaviour().is_none());
        let migrated_state = request(&migrated::FRAMING, EXECUTED, 1, true, Some(false));
        let migrated = receipt
            .migration()
            .unwrap_or_else(|| panic!("a migration record"));
        let simulated = migrated.simulated();
        assert_eq!(
            (
                simulated.states(),
                simulated.admitted(),
                simulated.genesis(),
                simulated.tuples()
            ),
            (80, 57, 1, 14_592)
        );
        assert_eq!(simulated.observations(), 0b10_1111);
        assert_eq!(migrated.root(), root(&migrated_state));
        let encoding = ok(Migration { fields: &fields }.encode());
        assert_eq!(
            migrated.migration(),
            <RustCryptoSha256 as CommitmentHasher>::hash(&encoding)
        );
        let snapshot = ok(shell.snapshot());
        assert_eq!(snapshot.state(), migrated_state.as_slice());
        assert_eq!((snapshot.version(), snapshot.upgrades()), (3, 1));
        // The payment pending before the upgrade keeps its ID.
        let mut destination = MemoryDestination::default();
        let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("the pending payment"));
        assert_eq!(token.delivery().delivery_id(), pending);
        ok(ok(token.deliver(&mut destination)).acknowledge());
        assert!(ok(shell.next_pending()).is_none());
        drop(shell);
        let bytes = ok(fs::read(&file.0));
        let history = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(history.contract_version, 2);
        assert_eq!(history.snapshot.state(), migrated_state);
        assert_eq!(history.genesis, original_history.genesis);
        assert_eq!(history.commits, original_history.commits);
        assert_ne!(
            history
                .commits
                .last()
                .map(|commit| commit.poststate.as_slice()),
            Some(history.snapshot.state())
        );
        assert_eq!(ok(fs::read(&file.0)), bytes);
        // Both segments replay, the simulation re-derived from the
        // lineage's memo rather than run again.
        let mut reopened = current(ok(lineage.open(&file.0)));
        let moved = ok(reopened.audit());
        assert_eq!(lineage.simulations(), 1);
        drop(reopened);
        // A checkpoint saved after the migration opens at the moved state;
        // the one saved before it at the same head was replaced.
        assert!(matches!(
            ok(lineage.open_at_checkpoint(&file.0, &moved)),
            Store::Current(_)
        ));
        assert!(matches!(
            refused(lineage.open_at_checkpoint(&file.0, &checkpoint).map(|_| ())),
            Error::Checkpoint
        ));
        // The version 1 Authority alone no longer opens the store.
        assert!(matches!(
            refused(V2SqliteShell::open(&file.0, &lineage.authorities()[0])),
            Error::Identity
        ));
        // A fresh lineage value audits it too, simulating once.
        let fresh = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let (version, _) = ok(fresh.audit_read_only(&file.0));
        assert_eq!((version, fresh.simulations()), (2, 1));
    });
}

#[test]
fn a_migrated_store_keeps_committing_under_the_new_layout() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 1);
        let (mut shell, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(
            (receipt.kind(), receipt.version()),
            (upgrade::Kind::Migration, 1)
        );
        let framing = &migrated::FRAMING;
        let v2a = &lineage.authorities()[1];
        let session = [
            (
                request(framing, PENDING, 1, false, Some(false)),
                command(framing, APPROVE, 0),
                context(framing, CFO),
            ),
            (
                request(framing, PENDING, 1, true, Some(false)),
                command(framing, EXECUTE, 0),
                context(framing, CLERK),
            ),
        ];
        for (index, (state, cmd, ctx)) in session.iter().enumerate() {
            let committed =
                ok(shell.commit(key(10 + index as u8), publication(v2a, state, cmd, ctx)));
            assert_eq!(committed.status(), CommitStatus::Committed);
        }
        let mut destination = MemoryDestination::default();
        let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("the new payment"));
        ok(ok(token.deliver(&mut destination)).acknowledge());
        drop(shell);
        let mut reopened = current(ok(lineage.open(&file.0)));
        let head = ok(reopened.snapshot());
        assert_eq!((head.version(), head.pending(), head.upgrades()), (3, 0, 1));
        assert_eq!(
            head.state(),
            request(framing, EXECUTED, 1, true, Some(false)).as_slice()
        );
    });
}

#[test]
fn a_migration_the_simulation_or_the_lineage_does_not_admit_writes_nothing() {
    with_versions(|v1, v2| {
        let file = StoreFile::new();
        {
            let fields = declared();
            let steps = [Step::Migration {
                migration: Migration { fields: &fields },
                claims: &[],
                review: b"no auxiliary claims",
            }];
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
            store_under(&file.0, &lineage.authorities()[0], 2);
        }
        let before = ok(fs::read(&file.0));
        let attempt = |step: Step<'_>| {
            let lineage = ok(Lineage::bind_steps(&[v1, v2], &[step]));
            let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
            assert_eq!(ok(fs::read(&file.0)), before, "{error:?}");
            error
        };
        // The urgent flag set: the old genesis state does not map.
        let urgent = with_urgent(Source::Value(1));
        let error = attempt(Step::Migration {
            migration: Migration { fields: &urgent },
            claims: &[],
            review: b"no auxiliary claims",
        });
        assert!(
            matches!(
                error,
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Differs {
                    observation: Observation::Genesis,
                    ordinal: 0
                }))
            ),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("genesis state at input tuple 0 maps to a state"),
            "{error}"
        );
        // A new field without a value.
        let missing: Vec<Target<'static>> = declared()
            .into_iter()
            .filter(|target| target.id != 124)
            .collect();
        assert!(matches!(
            attempt(Step::Migration {
                migration: Migration { fields: &missing },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Migration(
                migration::Defect::Missing { field: 124 }
            )))
        ));
        // A value outside the new field's domain.
        let wide = with_urgent(Source::Value(2));
        assert!(matches!(
            attempt(Step::Migration {
                migration: Migration { fields: &wide },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Value {
                field: 124,
                ..
            }))
        ));
        // Declared as a rename, which it is not; or as an adoption, which
        // keeps the state schema.
        assert!(matches!(
            attempt(Step::Rename),
            Error::Upgrade(upgrade::Refusal::Rename)
        ));
        assert!(matches!(
            attempt(Step::Adoption([7; 32])),
            Error::Upgrade(upgrade::Refusal::StateSchema)
        ));
    });
}

#[test]
fn an_audit_re_derives_the_migration_and_refuses_a_forged_or_undeclared_one() {
    with_versions(|v1, v2| {
        let fields = declared();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"no auxiliary claims",
        }];
        let lineage = ok(Lineage::bind_steps(&[v1, v2], &steps));
        let file = StoreFile::new();
        store_under(&file.0, &lineage.authorities()[0], 2);
        drop(ok(superseded(ok(lineage.open(&file.0))).upgrade()));
        let intact = ok(fs::read(&file.0));
        let open_with = |step: Step<'_>| {
            let other = ok(Lineage::bind_steps(&[v1, v2], &[step]));
            let error = refused(other.open(&file.0).map(|_| ()));
            assert_eq!(ok(fs::read(&file.0)), intact, "{error:?}");
            error
        };
        // An equivalent migration with another encoding: the urgent flag
        // from a table over the CEO's approval, false either way.
        let cases: &'static [(i128, i128)] = &[(0, 0), (1, 0)];
        let table = with_urgent(Source::Table { field: 123, cases });
        assert!(matches!(
            open_with(Step::Migration {
                migration: Migration { fields: &table },
                claims: &[],
                review: b"no auxiliary claims"
            }),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        assert!(matches!(
            open_with(Step::Rename),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        assert!(matches!(
            open_with(Step::Adoption([7; 32])),
            Error::Succession(upgrade::Unsupported::Migration)
        ));
        // A stored migrated state that is not the migration's: damage.
        let forged = StoreFile::new();
        ok(fs::copy(&file.0, &forged.0));
        {
            let connection = ok(rusqlite::Connection::open(&forged.0));
            let mut publication: Vec<u8> = ok(connection.query_row(
                "SELECT publication FROM v2_upgrades WHERE ordinal=1",
                [],
                |r| r.get(0),
            ));
            let last = publication.len() - 2;
            publication[last] ^= 1;
            ok(connection.execute(
                "UPDATE v2_upgrades SET publication=?1 WHERE ordinal=1",
                [publication],
            ));
        }
        assert!(matches!(
            refused(lineage.open(&forged.0).map(|_| ())),
            Error::History
        ));
        // The intact store still opens under its own lineage.
        drop(current(ok(lineage.open(&file.0))));
    });
}

/// The actual priority contract, as version 1 of a priority-to-base lineage.
/// Only the framing and the unused urgent carry differ from store_under.
fn priority_store_under(path: &Path, priority: &Authority<'_>, steps: usize) {
    let framing = &migrated::FRAMING;
    let initial = request(framing, EMPTY, 0, false, Some(false));
    let genesis = match priority.publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => panic!("priority genesis refused: {other:?}"),
    };
    let mut shell = ok(V2SqliteShell::create(path, priority, genesis));
    let session = [
        (
            request(framing, EMPTY, 0, false, Some(false)),
            command(framing, CREATE, 1),
            context(framing, CLERK),
        ),
        (
            request(framing, PENDING, 1, false, Some(false)),
            command(framing, APPROVE, 0),
            context(framing, CFO),
        ),
        (
            request(framing, PENDING, 1, true, Some(false)),
            command(framing, EXECUTE, 0),
            context(framing, CLERK),
        ),
    ];
    for (index, (state, cmd, ctx)) in session.iter().take(steps).enumerate() {
        let receipt = ok(shell.commit(
            key(u8::try_from(index + 1).unwrap_or_else(|_| panic!("commit key"))),
            publication(priority, state, cmd, ctx),
        ));
        assert_eq!(receipt.status(), CommitStatus::Committed);
    }
}

fn dropping_urgent() -> Vec<Target<'static>> {
    declared()
        .into_iter()
        .filter(|target| target.id != 124)
        .collect()
}

#[test]
fn an_unused_field_drop_keeps_live_history_and_original_pending_payment_bytes() {
    with_versions(|base, priority| {
        let fields = dropping_urgent();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"remove unused urgent field; keep every observed behaviour",
        }];
        let lineage = ok(Lineage::bind_steps(&[priority, base], &steps));
        let file = StoreFile::new();
        priority_store_under(&file.0, &lineage.authorities()[0], 3);
        let original = {
            let mut shell = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
            ok(shell.export_pending())
        };
        assert_eq!(original.len(), 1);
        let before = ok(fs::read(&file.0));
        let original_history = ok(lineage.audit_history_read_only(&file.0));
        let (version, head) = ok(lineage.audit_read_only(&file.0));
        assert_eq!((version, head.version(), head.pending()), (1, 3, 1));
        assert_eq!(ok(fs::read(&file.0)), before);

        let (mut shell, receipt) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        assert_eq!(receipt.kind(), upgrade::Kind::Migration);
        let simulated = receipt
            .migration()
            .unwrap_or_else(|| panic!("drop migration"))
            .simulated();
        assert_eq!(
            (
                simulated.states(),
                simulated.admitted(),
                simulated.genesis(),
                simulated.tuples()
            ),
            (160, 114, 1, 29_184)
        );
        assert_eq!(ok(shell.export_pending()), original);
        let expected = request(&migrated::v1::FRAMING, EXECUTED, 1, true, None);
        let snapshot = ok(shell.snapshot());
        assert_eq!(snapshot.state(), expected);
        assert_eq!(
            (snapshot.version(), snapshot.pending(), snapshot.upgrades()),
            (3, 1, 1)
        );
        drop(shell);

        // A fresh binding must replay the old segment before exporting and
        // delivering its original bytes under its original contract.
        let fresh = ok(Lineage::bind_steps(&[priority, base], &steps));
        let mut shell = current(ok(fresh.open(&file.0)));
        assert_eq!(ok(shell.export_pending()), original);
        let mut destination = MemoryDestination::default();
        let token = ok(shell.next_pending()).unwrap_or_else(|| panic!("historical payment"));
        assert_eq!(token.delivery().delivery_id(), original[0].delivery_id());
        ok(ok(token.deliver(&mut destination)).acknowledge());
        assert!(ok(shell.next_pending()).is_none());
        drop(shell);
        let bytes = ok(fs::read(&file.0));
        let replayed = ok(fresh.audit_history_read_only(&file.0));
        assert_eq!(replayed.contract_version, 2);
        assert_eq!(replayed.genesis, original_history.genesis);
        assert_eq!(replayed.commits, original_history.commits);
        assert_eq!(replayed.snapshot.state(), expected);
        assert_eq!(
            (replayed.snapshot.version(), replayed.snapshot.pending()),
            (3, 0)
        );
        assert_eq!(ok(fs::read(&file.0)), bytes);
    });
}

#[test]
fn a_dropped_field_store_continues_committing_and_replays_both_contracts() {
    with_versions(|base, priority| {
        let fields = dropping_urgent();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"remove unused urgent; continue under base",
        }];
        let lineage = ok(Lineage::bind_steps(&[priority, base], &steps));
        let file = StoreFile::new();
        priority_store_under(&file.0, &lineage.authorities()[0], 1);
        let (mut shell, _) = ok(superseded(ok(lineage.open(&file.0))).upgrade());
        let framing = &migrated::v1::FRAMING;
        for (index, (state, cmd, ctx)) in [
            (
                request(framing, PENDING, 1, false, None),
                command(framing, APPROVE, 0),
                context(framing, CFO),
            ),
            (
                request(framing, PENDING, 1, true, None),
                command(framing, EXECUTE, 0),
                context(framing, CLERK),
            ),
        ]
        .iter()
        .enumerate()
        {
            let receipt = ok(shell.commit(
                key(10 + u8::try_from(index).unwrap_or_else(|_| panic!("commit key"))),
                publication(&lineage.authorities()[1], state, cmd, ctx),
            ));
            assert_eq!(receipt.status(), CommitStatus::Committed);
        }
        assert_eq!(ok(shell.export_pending()).len(), 1);
        drop(shell);
        let bytes = ok(fs::read(&file.0));
        let replayed = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(
            replayed
                .commits
                .iter()
                .map(|commit| commit.contract_version)
                .collect::<Vec<_>>(),
            vec![1, 2, 2]
        );
        assert_eq!(
            replayed.snapshot.state(),
            request(framing, EXECUTED, 1, true, None)
        );
        assert_eq!(ok(fs::read(&file.0)), bytes);
    });
}

#[test]
fn dropping_a_read_approval_field_refuses_a_live_store_without_any_write() {
    with_versions(|base, priority| {
        let fields: Vec<Target<'static>> = dropping_urgent()
            .into_iter()
            .map(|target| {
                if target.id == 123 {
                    Target {
                        id: 123,
                        source: Source::Value(0),
                    }
                } else {
                    target
                }
            })
            .collect();
        let steps = [Step::Migration {
            migration: Migration { fields: &fields },
            claims: &[],
            review: b"invalid: forget read ceo_ok as well as unused urgent",
        }];
        let lineage = ok(Lineage::bind_steps(&[priority, base], &steps));
        let file = StoreFile::new();
        priority_store_under(&file.0, &lineage.authorities()[0], 3);
        let before = ok(fs::read(&file.0));
        let before_audit = ok(lineage.audit_history_read_only(&file.0));
        let before_pending = {
            let mut shell = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
            ok(shell.export_pending())
        };
        let error = refused(superseded(ok(lineage.open(&file.0))).upgrade());
        assert!(
            matches!(
                error,
                Error::Upgrade(upgrade::Refusal::Migration(Unsimulated::Differs {
                    observation: Observation::Decision,
                    ..
                }))
            ),
            "{error:?}"
        );
        assert_eq!(ok(fs::read(&file.0)), before);
        let after = ok(lineage.audit_history_read_only(&file.0));
        assert_eq!(after.contract_version, 1);
        assert_eq!(after.genesis, before_audit.genesis);
        assert_eq!(after.commits, before_audit.commits);
        assert_eq!(after.snapshot.state(), before_audit.snapshot.state());
        let mut shell = ok(V2SqliteShell::open(&file.0, &lineage.authorities()[0]));
        assert_eq!(ok(shell.export_pending()), before_pending);
        drop(shell);
        assert_eq!(ok(fs::read(&file.0)), before);
    });
}
