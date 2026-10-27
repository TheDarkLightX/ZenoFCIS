use super::{
    model::{self, Genesis, Input},
    ok,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::MemoryDestination;
use zeno_fcis_synthesis::finite::{
    V2Resource as Resource,
    canonical_v2::schema,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog as catalog,
    v2_composition::{self as c, Raw},
    v2_laws as l,
};
use zeno_fcis_value::Value;

#[allow(dead_code, unreachable_pub)]
#[path = "../../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub(crate) mod counter;

pub(crate) fn with_authority(run: impl FnOnce(&Authority<'_>)) {
    with_catalog(|catalog| run(&ok(authority::bind(catalog))));
}
pub(crate) fn with_catalog(run: impl FnOnce(&catalog::BoundCatalog<'_>)) {
    with_destinations(false, run);
}

pub(crate) fn with_destinations(
    two_destinations: bool,
    run: impl FnOnce(&catalog::BoundCatalog<'_>),
) {
    let contract = counter::Contract::new();
    let mut descriptor = contract.descriptor();
    // Same counter commands; two notifications make omission of a second entry observable.
    let plans: Vec<_> = descriptor
        .branches
        .iter()
        .map(|b| {
            let mut plans = b.outbox.to_vec();
            if let Some(first) = plans.first().copied() {
                plans.push(c::DeliveryPlan {
                    ordinal: 1,
                    destination: if two_destinations {
                        c::Expr::Constant(c::Atom::Text(b"other-observer"))
                    } else {
                        first.destination
                    },
                    ..first
                });
            }
            plans
        })
        .collect();
    let branches: Vec<_> = descriptor
        .branches
        .iter()
        .zip(&plans)
        .map(|(b, outbox)| c::Branch { outbox, ..*b })
        .collect();
    // Preserve the counter's state, guard and genesis laws. This fixture has
    // two notifications, so its delivery laws require both exact payloads and
    // destinations. The model separately compares all 24 legal command inputs.
    let payloads = [
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::ToI128(0),
        l::Op::Literal(l::Atom::I128(2)),
        l::Op::Eq(1, 2),
        l::Op::Observe(l::Observation::Post(110)),
        l::Op::Observe(l::Observation::Post(111)),
        l::Op::Observe(l::Observation::OutboxPayload(0, 112)),
        l::Op::Eq(6, 4),
        l::Op::Observe(l::Observation::OutboxPayload(0, 113)),
        l::Op::Eq(8, 5),
        l::Op::Observe(l::Observation::OutboxPayload(1, 112)),
        l::Op::Eq(10, 4),
        l::Op::Observe(l::Observation::OutboxPayload(1, 113)),
        l::Op::Eq(12, 5),
        l::Op::And(7, 9),
        l::Op::And(11, 13),
        l::Op::And(14, 15),
        l::Op::And(3, 16),
    ];
    let destinations = [
        l::Op::Observe(l::Observation::OutboxDestination(0)),
        l::Op::Literal(l::Atom::Text(b"local-observer")),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::OutboxDestination(1)),
        l::Op::Literal(l::Atom::Text(if two_destinations {
            b"other-observer"
        } else {
            b"local-observer"
        })),
        l::Op::Eq(3, 4),
        l::Op::And(2, 5),
    ];
    let branch_shape = [
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::ToI128(0),
        l::Op::Literal(l::Atom::I128(2)),
        l::Op::Eq(1, 2),
        l::Op::Observe(l::Observation::Class),
        l::Op::ToI128(4),
        l::Op::Literal(l::Atom::I128(1)),
        l::Op::Eq(5, 6),
        l::Op::Literal(l::Atom::I128(0)),
        l::Op::Eq(1, 8),
        l::Op::Select(7, 9, 3),
    ];
    let laws: Vec<_> = descriptor
        .laws
        .iter()
        .map(|law| l::Law {
            program: match law.id {
                510 => l::Program {
                    nodes: &payloads,
                    root: 17,
                },
                511 => l::Program {
                    nodes: &destinations,
                    root: 6,
                },
                991 => l::Program {
                    nodes: &branch_shape,
                    root: 10,
                },
                _ => l::Program {
                    nodes: law.program.nodes,
                    root: law.program.root,
                },
            },
            ..*law
        })
        .collect();
    let required: Vec<_> = laws.iter().map(|l| l.id).collect();
    descriptor.branches = &branches;
    descriptor.laws = &laws;
    descriptor.required = &required;
    descriptor.limits = descriptor
        .limits
        .with_limit(Resource::Effect, 2)
        .with_limit(Resource::Byte, 4096)
        .with_limit(Resource::Step, 2000);
    let policy = authority::policy_bytes(
        &descriptor,
        counter::ORIGINAL_SCHEMA,
        &counter::FRAMING,
        counter::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("policy"));
    let catalog = ok(catalog::bind_original(
        counter::ORIGINAL_SCHEMA,
        &counter::DESCRIPTION,
        catalog::Limits {
            schema: schema::Limits {
                bytes: counter::ORIGINAL_SCHEMA.len() as u64,
                types: 6,
                fields: 2,
                variants: 2,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &counter::FRAMING,
        counter::CHANNEL_ROOTS,
    ));
    run(&catalog);
}
pub(crate) fn schema_hash() -> Hash32 {
    Hash32::new(counter::FRAMING.state.schema)
}
pub(crate) fn command(variant: u16) -> Vec<u8> {
    model::envelope(101, schema_hash(), Value::sum(101, variant, None))
}
pub(crate) fn context() -> Vec<u8> {
    model::envelope(102, schema_hash(), Value::boolean(true))
}
pub(crate) fn genesis<'a>(a: &'a Authority<'_>, initial: &'a [u8]) -> Publication<'a> {
    match a.publish_genesis(initial) {
        PublicationOutcome::Commit(p) => p,
        r => panic!("genesis {r:?}"),
    }
}
pub(crate) fn publication<'a>(
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
        r => panic!("publication {r:?}"),
    }
}
pub(crate) fn model_genesis(a: &Authority<'_>) -> Genesis {
    let initial = model::state(schema_hash(), [0, 0]);
    let p = genesis(a, &initial);
    Genesis {
        schema: schema_hash(),
        identity: a.identity().to_vec(),
        subject: p.subject().to_vec(),
        interpreter: ok(MemoryDestination::interpreter_identity()),
        entries: 2,
    }
}
pub(crate) fn input(a: &Authority<'_>, key: Hash32, before: [i128; 2], variant: u16) -> Input {
    let state = model::state(schema_hash(), before);
    let command = command(variant);
    let context = context();
    let p = publication(a, &state, &command, &context);
    let subject = p.subject().to_vec();
    Input {
        key,
        before,
        variant,
        command,
        context,
        subject,
    }
}

// Immutable request bytes for the complete 4x4 bounded counter input domain.
// This avoids evaluating the fixture twice per commit; the actual shell path
// still receives a newly admitted publication on every invocation and retry.
pub(crate) fn cases(a: &Authority<'_>) -> Vec<Input> {
    let mut cases = Vec::new();
    for count in 0..=3 {
        for failures in 0..=3 {
            for variant in [120, 121] {
                let before = [count, failures];
                if before[usize::from(variant == 121)] < 3 {
                    cases.push(input(a, super::key(0), before, variant));
                }
            }
        }
    }
    cases
}
pub(crate) fn request(cases: &[Input], key: Hash32, before: [i128; 2], variant: u16) -> Input {
    let Some(mut input) = cases
        .iter()
        .find(|p| p.before == before && p.variant == variant)
        .cloned()
    else {
        assert_eq!(before[usize::from(variant == 121)], 3);
        return Input {
            key,
            before,
            variant,
            command: command(variant),
            context: context(),
            subject: vec![],
        };
    };
    input.key = key;
    input
}
