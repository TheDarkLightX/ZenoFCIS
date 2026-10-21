//! Permanent production store-substitution regression.
#[allow(dead_code, unused_imports)]
mod sim;
use sim::{database::Files, key, model, ok};
#[allow(dead_code, unused_imports)]
#[path = "sim/review_genesis.rs"]
mod fixture;
use zeno_fcis_shell::effects::{
    driver::Binding,
    handler::{Handled, Terminal},
    port::{self, Operation, Response, Sent},
};
use zeno_fcis_shell_sqlite::{
    effects::production::{NoTime, Production, Transport},
    v2::{Lineage, V2SqliteShell},
};
use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome;
struct Wire;
impl Transport for Wire {
    fn send(&mut self, _: &port::Delivery) -> Sent {
        Sent::NotSent(port::Code::Delivery)
    }
}
#[test]
fn production_rebind_rejects_other_genesis() {
    for kind in 0..7 {
        fixture::with_catalog(|catalog| {
            let lineage = ok(Lineage::bind(&[catalog], &[]));
            let authority = &lineage.authorities()[0];
            let original = Files::new();
            drop(sim::create(&original, authority));
            let mut handler: Production<'_, '_, Wire, NoTime> =
                ok(Production::open(&original.store, &lineage, 1, Wire, None));
            let old_store = handler.store();
            let op = match kind {
                0 => Operation::Reopen(old_store),
                1 => Operation::ReadState(old_store),
                2 | 6 => Operation::ReadHistory(old_store),
                3 => Operation::ReadOutbox(old_store),
                4 => Operation::Acknowledge(
                    old_store,
                    zeno_fcis_codec::Hash32::ZERO,
                    zeno_fcis_codec::Hash32::ZERO,
                ),
                _ => Operation::Commit(
                    old_store,
                    key(1),
                    zeno_fcis_shell_sqlite::effects::audited_publication(
                        authority,
                        &model::state(fixture::schema_hash(), [0, 0]),
                        &fixture::command(120),
                        &fixture::context(),
                    )
                    .unwrap_or_else(|error| panic!("{error:?}")),
                    port::CommitMode::Unversioned,
                ),
            };
            let binding = Binding::of(port::OpId::first(1), &op);
            if kind != 6 {
                handler.crash(&binding);
            }
            let replacement = Files::new();
            let different = model::state(fixture::schema_hash(), [1, 0]);
            let publication = match authority.publish_genesis(&different) {
                PublicationOutcome::Commit(p) => p,
                other => panic!("{other:?}"),
            };
            drop(ok(V2SqliteShell::create(
                &replacement.store,
                authority,
                publication,
            )));
            ok(std::fs::copy(&replacement.store, &original.store));
            let fresh: Production<'_, '_, Wire, NoTime> =
                ok(Production::open(&original.store, &lineage, 2, Wire, None));
            assert_ne!(fresh.store().identity, old_store.identity);
            let answer = handler.handle(&binding, &op);
            assert_eq!(
                answer,
                Handled::Answer(Response::refused(op.port(), port::Code::Identity)),
                "substituted genesis accepted on {kind}"
            );
            assert_eq!(handler.store(), old_store);
            println!("CHECK: substituted genesis is refused under the original Store identity");
        });
    }
}
