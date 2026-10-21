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
    // The state-update and genesis laws stay intact. The simulation's
    // decision law checks the two-notification branch shape. Exact application
    // behavior is separately compared against all 24 legal command inputs.
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
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: l::Program {
                nodes: if law.id == 991 {
                    &branch_shape
                } else {
                    law.program.nodes
                },
                root: if law.id == 991 { 10 } else { law.program.root },
            },
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
    run(&ok(authority::bind(&catalog)));
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
