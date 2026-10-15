//! Full-width numeric data through the actual catalog, Authority and SQLite store.
//! The fixture supplies declarative data only; the library evaluates every decision/law.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::{FieldDef, FieldId, Schema, SchemaLimits, TypeDef, TypeId, TypeKind};
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::v2::{Error, V2SqliteShell};
use zeno_fcis_synthesis::finite::{
    V2InputField, V2NumericOp as N, V2NumericProgram, V2Resource as R,
    canonical_v2::schema as s,
    v2_authority::{self as a, Authority, Publication, PublicationOutcome},
    v2_catalog as catalog, v2_composition as c, v2_laws as l, v2_zero_limits,
};
use zeno_fcis_value::{Field, Value};

fn ok<T, E: std::fmt::Debug>(v: Result<T, E>) -> T {
    v.unwrap_or_else(|e| panic!("unexpected numeric fixture refusal: {e:?}"))
}
const SIGNED: c::Domain<'static> = c::Domain::I128 {
    min: i128::MIN,
    max: i128::MAX,
};
const UNSIGNED: c::Domain<'static> = c::Domain::U128 {
    min: 0,
    max: u128::MAX,
};
const BOOL: c::Domain<'static> = c::Domain::Bool;
const STATE: &[V2InputField<c::Domain<'static>>] = &[
    V2InputField {
        id: 10,
        leaf: SIGNED,
    },
    V2InputField {
        id: 11,
        leaf: UNSIGNED,
    },
];
const COMMAND: &[V2InputField<c::Domain<'static>>] = &[
    V2InputField {
        id: 20,
        leaf: SIGNED,
    },
    V2InputField {
        id: 21,
        leaf: UNSIGNED,
    },
];
const DESCRIPTION: s::Description<'static> = s::Description {
    profile: b"numeric_store_witness",
    version: 1,
    root: 100,
    definitions: &[
        s::Definition {
            id: 100,
            name: b"State",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 10,
                    name: b"signed",
                    type_id: 103,
                },
                s::Field {
                    id: 11,
                    name: b"unsigned",
                    type_id: 104,
                },
            ]),
        },
        s::Definition {
            id: 101,
            name: b"Command",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 20,
                    name: b"signed",
                    type_id: 103,
                },
                s::Field {
                    id: 21,
                    name: b"unsigned",
                    type_id: 104,
                },
            ]),
        },
        s::Definition {
            id: 102,
            name: b"Context",
            kind: s::Kind::Bool,
        },
        s::Definition {
            id: 103,
            name: b"Signed",
            kind: s::Kind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        s::Definition {
            id: 104,
            name: b"Unsigned",
            kind: s::Kind::U128 {
                min: 0,
                max: u128::MAX,
            },
        },
    ],
};
const BRANCHES: &[c::Branch<'static>] = &[
    c::Branch {
        code: 0,
        class: c::Class::Reject,
        reason: Some(200),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 1,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 10,
                value: c::Expr::Output(1),
                domain: SIGNED,
            },
            c::Assignment {
                field: 11,
                value: c::Expr::Output(2),
                domain: UNSIGNED,
            },
        ],
        effects: &[],
        outbox: &[],
    },
];
const LAWS: &[l::Law<'static>] = &[
    l::Law {
        id: 500,
        kind: l::Kind::StateInvariant,
        scope: l::Scope::Committing,
        genesis: true,
        program: l::Program {
            nodes: &[
                l::Op::Observe(l::Observation::Post(10)),
                l::Op::Literal(c::Atom::I128(i128::MIN)),
                l::Op::Lt(0, 1),
                l::Op::Not(2),
                l::Op::Observe(l::Observation::Post(11)),
                l::Op::Literal(c::Atom::U128(u128::MAX)),
                l::Op::Lt(5, 4),
                l::Op::Not(6),
                l::Op::And(3, 7),
            ],
            root: 8,
        },
    },
    l::Law {
        id: 501,
        kind: l::Kind::RejectNoAuthority,
        scope: l::Scope::Reject,
        genesis: false,
        program: l::Program {
            nodes: &[
                l::Op::Literal(c::Atom::U128(0)),
                l::Op::Observe(l::Observation::PostLength),
                l::Op::Eq(0, 1),
                l::Op::Observe(l::Observation::PatchLength),
                l::Op::Eq(0, 3),
                l::Op::And(2, 4),
                l::Op::Observe(l::Observation::EffectLength),
                l::Op::Eq(0, 6),
                l::Op::And(5, 7),
                l::Op::Observe(l::Observation::OutboxLength),
                l::Op::Eq(0, 9),
                l::Op::And(8, 10),
            ],
            root: 11,
        },
    },
    // This policy has no committing-failure branch; one must never slip through.
    l::Law {
        id: 502,
        kind: l::Kind::CommittedFailureEffects,
        scope: l::Scope::CommittedFailure,
        genesis: false,
        program: l::Program {
            nodes: &[l::Op::Literal(c::Atom::Bool(false))],
            root: 0,
        },
    },
    l::Law {
        id: 503,
        kind: l::Kind::DecisionConformance,
        scope: l::Scope::Always,
        genesis: false,
        program: l::Program {
            nodes: &[
                l::Op::Observe(l::Observation::ContextRoot),
                l::Op::Literal(c::Atom::I128(0)),
                l::Op::Literal(c::Atom::I128(1)),
                l::Op::Select(0, 1, 2),
                l::Op::Observe(l::Observation::Class),
                l::Op::Eq(3, 4),
            ],
            root: 5,
        },
    },
    l::Law {
        id: 504,
        kind: l::Kind::InitialCondition,
        scope: l::Scope::Always,
        genesis: true,
        program: l::Program {
            nodes: &[
                l::Op::Observe(l::Observation::Initial(10)),
                l::Op::Literal(c::Atom::I128(0)),
                l::Op::Eq(0, 1),
                l::Op::Observe(l::Observation::Initial(11)),
                l::Op::Literal(c::Atom::U128(0)),
                l::Op::Eq(3, 4),
                l::Op::And(2, 5),
            ],
            root: 6,
        },
    },
    // These comparisons consume full U128 directly: no signed narrowing in laws.
    l::Law {
        id: 505,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: &[
                l::Op::Observe(l::Observation::Post(10)),
                l::Op::Observe(l::Observation::Command(20)),
                l::Op::Eq(0, 1),
                l::Op::Observe(l::Observation::Post(11)),
                l::Op::Observe(l::Observation::Command(21)),
                l::Op::Eq(3, 4),
                l::Op::And(2, 5),
            ],
            root: 6,
        },
    },
];
fn descriptor() -> c::NumericDescriptor<'static> {
    let mut limits = v2_zero_limits();
    for resource in [
        R::Read,
        R::Write,
        R::Candidate,
        R::Effect,
        R::Byte,
        R::WitnessByte,
        R::Depth,
        R::Step,
    ] {
        limits = limits.with_limit(resource, 100_000);
    }
    c::NumericDescriptor {
        state: c::Schema::Record(STATE),
        command: c::Schema::Record(COMMAND),
        context: c::Schema::Leaf(&BOOL),
        program: V2NumericProgram {
            inputs: &[SIGNED, UNSIGNED, SIGNED, UNSIGNED, BOOL],
            outputs: &[c::Domain::I128 { min: 0, max: 1 }, SIGNED, UNSIGNED],
            nodes: &[
                N::Input(0),
                N::Input(1),
                N::Input(2),
                N::Input(3),
                N::Input(4),
                N::I128(0),
                N::I128(1),
                N::Select(4, 6, 5),
            ],
            roots: &[7, 2, 3],
        },
        bindings: &[
            c::Binding {
                source: c::Source::State,
                selector: c::Selector::Field(10),
            },
            c::Binding {
                source: c::Source::State,
                selector: c::Selector::Field(11),
            },
            c::Binding {
                source: c::Source::Command,
                selector: c::Selector::Field(20),
            },
            c::Binding {
                source: c::Source::Command,
                selector: c::Selector::Field(21),
            },
            c::Binding {
                source: c::Source::Context,
                selector: c::Selector::Root,
            },
        ],
        output_types: &[c::Domain::I128 { min: 0, max: 1 }, SIGNED, UNSIGNED],
        decision_output: 0,
        branches: BRANCHES,
        reasons: &[c::Reason {
            id: 200,
            class: c::Class::Reject,
        }],
        channels: &[],
        laws: LAWS,
        required: &[500, 501, 502, 503, 504, 505],
        limits,
    }
}
fn original_schema() -> Schema {
    let limits = SchemaLimits::default();
    let record = |signed_id, unsigned_id| TypeKind::Record {
        fields: vec![
            ok(FieldDef::try_new(
                FieldId::new(signed_id),
                "signed",
                TypeId::new(103),
            )),
            ok(FieldDef::try_new(
                FieldId::new(unsigned_id),
                "unsigned",
                TypeId::new(104),
            )),
        ]
        .into_boxed_slice(),
    };
    ok(Schema::try_new(
        "numeric_store_witness",
        1,
        TypeId::new(100),
        vec![
            ok(TypeDef::try_new(
                TypeId::new(100),
                "State",
                record(10, 11),
                limits,
            )),
            ok(TypeDef::try_new(
                TypeId::new(101),
                "Command",
                record(20, 21),
                limits,
            )),
            ok(TypeDef::try_new(
                TypeId::new(102),
                "Context",
                TypeKind::Bool,
                limits,
            )),
            ok(TypeDef::try_new(
                TypeId::new(103),
                "Signed",
                TypeKind::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
                limits,
            )),
            ok(TypeDef::try_new(
                TypeId::new(104),
                "Unsigned",
                TypeKind::U128 {
                    min: 0,
                    max: u128::MAX,
                },
                limits,
            )),
        ],
        limits,
    ))
}
fn framing(hash: [u8; 32]) -> c::Framing {
    let binding = |root| c::FrameBinding {
        root,
        schema: hash,
        max_bytes: 4096,
    };
    c::Framing {
        state: binding(100),
        command: binding(101),
        context: binding(102),
    }
}
fn with_authority(run: impl FnOnce(&Authority<'_, c::NumericDescriptor<'_>>, &c::Framing, &[u8])) {
    with_custom_authority(descriptor(), run);
}
fn with_custom_authority(
    descriptor: c::NumericDescriptor<'_>,
    run: impl FnOnce(&Authority<'_, c::NumericDescriptor<'_>>, &c::Framing, &[u8]),
) {
    let schema = original_schema();
    let raw = ok(schema.canonical_bytes());
    let hash = ok(schema.schema_hash::<RustCryptoSha256>());
    let framing = framing(*hash.as_bytes());
    let policy = a::numeric_policy_bytes(&descriptor, &raw, &framing, &[])
        .unwrap_or_else(|| panic!("numeric policy encoding refused"));
    let bound = ok(catalog::bind_original(
        &raw,
        &DESCRIPTION,
        catalog::Limits {
            schema: s::Limits {
                bytes: raw.len() as u64,
                types: 5,
                fields: 4,
                variants: 0,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &framing,
        &[],
    ));
    let authority = ok(a::bind(&bound));
    run(&authority, &framing, &raw);
}

#[test]
fn exact_wide_operations_cross_catalog_laws_publication_and_durable_replay() {
    let mut descriptor = descriptor();
    let mut nodes = descriptor.program.nodes.to_vec();
    nodes.extend_from_slice(&[
        N::MulDivFloor(3, 3, 3),
        N::MulDivCeil(3, 3, 3),
        N::Eq(8, 9),
        N::ProductsGe([3, 3, 3], [3, 3, 3]),
        N::And(10, 11),
        N::And(4, 12),
        N::Select(13, 6, 5),
    ]);
    descriptor.program.nodes = &nodes;
    descriptor.program.roots = &[14, 2, 9];
    with_custom_authority(descriptor, |authority, f, _| {
        let file = StoreFile::new();
        let mut pre = numbers(f.state, 0, 0, 10);
        let context = envelope(f.context, Value::boolean(true));
        let mut db = ok(V2SqliteShell::create(
            &file.0,
            authority,
            commit(authority.publish_genesis(&pre)),
        ));
        for (index, unsigned) in [1u128 << 63, 1u128 << 127, u128::MAX]
            .into_iter()
            .enumerate()
        {
            // x*x/x == x and comparing a three-factor product with itself is true.
            // These independent algebraic expectations require no executable helper.
            let command = numbers(f.command, 0, unsigned, 20);
            let expected = numbers(f.state, 0, unsigned, 10);
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let publication = commit(authority.publish(raw));
            assert_eq!(publication.poststate(), expected);
            assert!(
                publication
                    .evaluation()
                    .outcome()
                    .diagnostics()
                    .iter()
                    .any(|d| d.id == 505 && d.verdict == l::Verdict::Satisfied)
            );
            let subject = publication.subject().to_vec();
            assert_eq!(
                commit(authority.replay_publication(raw, &subject)).subject(),
                subject
            );
            let key = Hash32::new([ok(u8::try_from(index + 1)); 32]);
            assert_eq!(
                ok(db.commit(key, publication)).status(),
                CommitStatus::Committed
            );
            assert_eq!(
                ok(db.commit(key, commit(authority.publish(raw)))).status(),
                CommitStatus::IdempotentReplay
            );
            ok(db.audit());
            drop(db);
            db = ok(V2SqliteShell::open(&file.0, authority));
            assert_eq!(ok(db.snapshot()).state(), expected);
            assert_eq!(ok(db.snapshot()).version(), index as u64 + 1);
            pre = expected;
        }
        let before = ok(db.snapshot());
        let command = numbers(f.command, 0, 0, 20);
        // Eager division by zero withholds publication and leaves the store untouched.
        assert!(matches!(
            authority.publish(c::Raw {
                state: &pre,
                command: &command,
                context: &context
            }),
            PublicationOutcome::Refused { .. }
        ));
        let after = ok(db.snapshot());
        assert_eq!(before.state(), after.state());
        assert_eq!(before.version(), after.version());
        ok(db.audit());
    });
}
fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}
fn numbers(binding: c::FrameBinding, signed: i128, unsigned: u128, first: u16) -> Vec<u8> {
    envelope(
        binding,
        ok(Value::record_canonical(vec![
            Field::new(first, Value::signed(signed)),
            Field::new(first + 1, Value::unsigned(unsigned)),
        ])),
    )
}
fn commit(outcome: PublicationOutcome<'_>) -> Publication<'_> {
    match outcome {
        PublicationOutcome::Commit(p) => p,
        other => panic!("expected publication, got {other:?}"),
    }
}
static COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zenofcis-numeric-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        ok(std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p));
        Self(p)
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn full_width_candidate_laws_and_existing_sqlite_create_open_replay() {
    with_authority(|a, f, _| {
        let initial = numbers(f.state, 0, 0, 10);
        let context = envelope(f.context, Value::boolean(true));
        let file = StoreFile::new();
        let mut db = ok(V2SqliteShell::create(
            &file.0,
            a,
            commit(a.publish_genesis(&initial)),
        ));
        let mut pre = initial;
        let mut previous = (0_i128, 0_u128);
        for (index, (signed, unsigned)) in [
            (i128::from(i64::MAX) + 1, (i128::MAX as u128) + 1),
            (i128::MIN, u128::MAX),
            (i128::MAX, 0),
        ]
        .into_iter()
        .enumerate()
        {
            let command = numbers(f.command, signed, unsigned, 20);
            let expected = numbers(f.state, signed, unsigned, 10);
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let publication = commit(a.publish(raw));
            let evaluated = publication.evaluation();
            let candidate = ok(evaluated.outcome().result());
            assert_eq!(candidate.class(), c::Class::Accept);
            assert_eq!(candidate.reason(), None);
            assert_eq!(
                candidate.post(),
                &[
                    c::Field {
                        id: 10,
                        value: c::Atom::I128(signed)
                    },
                    c::Field {
                        id: 11,
                        value: c::Atom::U128(unsigned)
                    }
                ]
            );
            assert_eq!(
                candidate.pre(),
                &[
                    c::Field {
                        id: 10,
                        value: c::Atom::I128(previous.0)
                    },
                    c::Field {
                        id: 11,
                        value: c::Atom::U128(previous.1)
                    }
                ]
            );
            assert_eq!(
                candidate.patch(),
                &[
                    l::Patch {
                        field: 10,
                        before: c::Atom::I128(previous.0),
                        after: c::Atom::I128(signed)
                    },
                    l::Patch {
                        field: 11,
                        before: c::Atom::U128(previous.1),
                        after: c::Atom::U128(unsigned)
                    },
                ]
            );
            assert!(candidate.effects().is_empty() && candidate.outbox().is_empty());
            assert_eq!(publication.poststate(), expected);
            assert!(
                evaluated
                    .outcome()
                    .diagnostics()
                    .iter()
                    .any(|d| d.id == 505 && d.verdict == l::Verdict::Satisfied)
            );
            let evaluation_subject = ok(evaluated.subject()).to_vec();
            assert_eq!(
                ok(a.replay(raw, &evaluation_subject).subject()),
                evaluation_subject
            );
            let subject = publication.subject().to_vec();
            assert_eq!(
                commit(a.replay_publication(raw, &subject)).subject(),
                subject
            );
            // Evaluation and publication subjects are deliberately distinct.
            assert_eq!(
                a.replay(raw, &subject).subject(),
                Err(a::Refusal::ReplayMismatch)
            );
            let key = Hash32::new([ok(u8::try_from(index + 1)); 32]);
            assert_eq!(
                ok(db.commit(key, publication)).status(),
                CommitStatus::Committed
            );
            assert_eq!(
                ok(db.commit(key, commit(a.publish(raw)))).status(),
                CommitStatus::IdempotentReplay
            );
            assert_eq!(ok(db.snapshot()).state(), expected);
            ok(db.audit());
            drop(db);
            db = ok(V2SqliteShell::open(&file.0, a));
            assert_eq!(ok(db.snapshot()).state(), expected);
            assert_eq!(ok(db.snapshot()).version(), index as u64 + 1);
            pre = expected;
            previous = (signed, unsigned);
        }
    });
}

#[test]
fn numeric_reject_genesis_schema_and_replay_negatives_expose_no_publication() {
    with_authority(|a, f, _| {
        let initial = numbers(f.state, 0, 0, 10);
        let bad_initial = numbers(f.state, 1, 0, 10);
        assert!(matches!(
            a.publish_genesis(&bad_initial),
            PublicationOutcome::Refused { .. }
        ));
        let command = numbers(f.command, i128::MAX, u128::MAX, 20);
        let denied = envelope(f.context, Value::boolean(false));
        let allowed = envelope(f.context, Value::boolean(true));
        let denied_raw = c::Raw {
            state: &initial,
            command: &command,
            context: &denied,
        };
        let PublicationOutcome::Reject(e) = a.publish(denied_raw) else {
            panic!("business Reject required")
        };
        let candidate = ok(e.outcome().result());
        assert_eq!(candidate.reason(), Some(200));
        assert!(
            candidate.post().is_empty()
                && candidate.patch().is_empty()
                && candidate.effects().is_empty()
                && candidate.outbox().is_empty()
        );
        assert!(
            e.outcome()
                .diagnostics()
                .iter()
                .any(|d| d.id == 501 && d.verdict == l::Verdict::Satisfied)
        );
        let mut wrong = f.command;
        wrong.schema[0] ^= 1;
        let wrong_command = numbers(wrong, i128::MAX, u128::MAX, 20);
        assert!(matches!(
            a.publish(c::Raw {
                state: &initial,
                command: &wrong_command,
                context: &allowed
            }),
            PublicationOutcome::Refused { .. }
        ));
        let raw = c::Raw {
            state: &initial,
            command: &command,
            context: &allowed,
        };
        let mut subject = commit(a.publish(raw)).subject().to_vec();
        assert_eq!(
            commit(a.replay_publication(raw, &subject)).subject(),
            subject
        );
        subject[0] ^= 1;
        assert!(matches!(
            a.replay_publication(raw, &subject),
            PublicationOutcome::Refused {
                error: a::Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}

#[test]
fn numeric_store_refuses_different_complete_policy_identity() {
    with_authority(|authority, f, raw| {
        let initial = numbers(f.state, 0, 0, 10);
        let file = StoreFile::new();
        drop(ok(V2SqliteShell::create(
            &file.0,
            authority,
            commit(authority.publish_genesis(&initial)),
        )));
        let mut descriptor = descriptor();
        descriptor.limits = descriptor.limits.with_limit(R::Step, 100_001);
        let policy = a::numeric_policy_bytes(&descriptor, raw, f, &[])
            .unwrap_or_else(|| panic!("numeric policy encoding refused"));
        let catalog = ok(catalog::bind_original(
            raw,
            &DESCRIPTION,
            catalog::Limits {
                schema: s::Limits {
                    bytes: raw.len() as u64,
                    types: 5,
                    fields: 4,
                    variants: 0,
                },
                contract_bytes: policy.len() as u64,
            },
            &policy,
            &descriptor,
            f,
            &[],
        ));
        let different = ok(a::bind(&catalog));
        assert_ne!(authority.identity(), different.identity());
        assert!(matches!(
            V2SqliteShell::open(&file.0, &different),
            Err(Error::Identity)
        ));
    });
}

#[test]
fn numeric_catalog_refuses_wrong_original_root_and_complete_policy_bytes() {
    with_authority(|_, f, raw| {
        let descriptor = descriptor();
        let limits = |length| catalog::Limits {
            schema: s::Limits {
                bytes: raw.len() as u64,
                types: 5,
                fields: 4,
                variants: 0,
            },
            contract_bytes: length,
        };
        let mut wrong = *f;
        wrong.command.root = wrong.state.root;
        let wrong_policy = a::numeric_policy_bytes(&descriptor, raw, &wrong, &[])
            .unwrap_or_else(|| panic!("policy encoding refused"));
        assert_eq!(
            catalog::bind_original(
                raw,
                &DESCRIPTION,
                limits(wrong_policy.len() as u64),
                &wrong_policy,
                &descriptor,
                &wrong,
                &[]
            )
            .err(),
            Some(catalog::Failure::Roots)
        );
        let mut policy = a::numeric_policy_bytes(&descriptor, raw, f, &[])
            .unwrap_or_else(|| panic!("policy encoding refused"));
        policy[0] ^= 1;
        assert_eq!(
            catalog::bind_original(
                raw,
                &DESCRIPTION,
                limits(policy.len() as u64),
                &policy,
                &descriptor,
                f,
                &[]
            )
            .err(),
            Some(catalog::Failure::Policy)
        );
    });
}

#[test]
fn admitted_bad_assignment_is_refused_by_actual_full_width_law() {
    // The mutant remains structurally admitted and type correct. Only its
    // successor relation is wrong: it writes zero instead of the U128 command.
    const MUTANT: &[c::Branch<'static>] = &[
        BRANCHES[0],
        c::Branch {
            code: 1,
            class: c::Class::Accept,
            reason: None,
            assignments: &[
                c::Assignment {
                    field: 10,
                    value: c::Expr::Output(1),
                    domain: SIGNED,
                },
                c::Assignment {
                    field: 11,
                    value: c::Expr::Constant(c::Atom::U128(0)),
                    domain: UNSIGNED,
                },
            ],
            effects: &[],
            outbox: &[],
        },
    ];
    with_authority(|_, f, raw| {
        let mut descriptor = descriptor();
        descriptor.branches = MUTANT;
        let policy = a::numeric_policy_bytes(&descriptor, raw, f, &[])
            .unwrap_or_else(|| panic!("policy encoding refused"));
        let catalog = ok(catalog::bind_original(
            raw,
            &DESCRIPTION,
            catalog::Limits {
                schema: s::Limits {
                    bytes: raw.len() as u64,
                    types: 5,
                    fields: 4,
                    variants: 0,
                },
                contract_bytes: policy.len() as u64,
            },
            &policy,
            &descriptor,
            f,
            &[],
        ));
        let mutant = ok(a::bind(&catalog));
        let initial = numbers(f.state, 0, 0, 10);
        let command = numbers(f.command, i128::MAX, u128::MAX, 20);
        let context = envelope(f.context, Value::boolean(true));
        let PublicationOutcome::Refused { evaluation, error } = mutant.publish(c::Raw {
            state: &initial,
            command: &command,
            context: &context,
        }) else {
            panic!("bad successor must have no publication")
        };
        assert_eq!(
            error,
            a::Refusal::Core(c::Failure::Law(l::Failure::Violated))
        );
        assert_eq!(
            evaluation.outcome().diagnostics().last(),
            Some(&l::Diagnostic {
                id: 505,
                verdict: l::Verdict::Refused(l::Failure::Violated),
            })
        );
        assert!(evaluation.outcome().result().is_err());
        assert!(evaluation.subject().is_err());
    });
}
