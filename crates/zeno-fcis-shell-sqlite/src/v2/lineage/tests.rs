//! Shared-cache contention must release SQLite before waiting for a comparison.
use super::*;
use rusqlite::hooks::{AuthAction, Authorization};
use std::sync::mpsc;
use std::time::Duration;
use zeno_fcis_codec::{CanonicalEncode, Envelope};
use zeno_fcis_synthesis::finite::{Op, V2Resource, canonical_v2::schema, v2_catalog as catalog};
use zeno_fcis_value::{Field, Value};

#[allow(dead_code, unreachable_pub)]
#[path = "../../../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
mod counter;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

struct StoreFile(std::path::PathBuf);
impl StoreFile {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "zenofcis-shared-lineage-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn a_shared_comparison_never_blocks_a_checkpoint_open_while_it_holds_sqlite() {
    let contract = counter::Contract::new();
    let old = contract.descriptor();
    let old_catalog = ok(counter::checked_catalog(&old));
    let mut new = contract.descriptor();
    // A different graph with the same outputs exercises an enumerated
    // succession, rather than the identical-program shortcut.
    let mut nodes = new.program.nodes.to_vec();
    nodes.push(Op::Bool(false));
    new.program.nodes = &nodes;
    new.limits = new
        .limits
        .with_limit(V2Resource::Step, new.limits.limit(V2Resource::Step) + 1);
    let policy = authority::policy_bytes(
        &new,
        counter::ORIGINAL_SCHEMA,
        &counter::FRAMING,
        counter::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("policy encodes"));
    let new_catalog = ok(catalog::bind_original(
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
        &new,
        &counter::FRAMING,
        counter::CHANNEL_ROOTS,
    ));
    let catalogs = [&old_catalog, &new_catalog];
    let receipts = [[7; 32]];
    let established = ok(Lineage::bind(&catalogs, &receipts));
    let file = StoreFile::new();
    let initial = ok(Value::record_canonical(vec![
        Field::new(110, Value::signed(0)),
        Field::new(111, Value::signed(0)),
    ]));
    let initial = ok(Envelope::new(
        counter::FRAMING.state.root,
        Hash32::new(counter::FRAMING.state.schema),
        initial,
    )
    .canonical_bytes());
    let first = &established.authorities()[0];
    let PublicationOutcome::Commit(genesis) = first.publish_genesis(&initial) else {
        panic!("genesis publishes");
    };
    let mut store = ok(V2SqliteShell::create(&file.0, first, genesis));
    let checkpoint = ok(store.checkpoint());
    drop(store);
    let Opened::V10(Store::Superseded(store)) = ok(established.open(&file.0)) else {
        panic!("the first version is superseded");
    };
    let (_, receipt) = ok(store.upgrade());
    assert_eq!(receipt.kind(), Kind::ProgramSuccessor);
    assert_eq!(receipt.premises().map(Premises::tuples), Some(64));
    assert_eq!(established.comparisons(), 1);

    let shared = ok(Lineage::bind(&catalogs, &receipts));
    // Hold the very mutex establish holds throughout comparison. This
    // pauses its serialization point deterministically, without depending
    // on a large graph taking longer than a sleep.
    let mut writer = ok(open_existing(&file.0));
    ok(writer.busy_timeout(Duration::from_secs(2)));
    let comparing = shared.memo();
    let (entered_tx, in_tx) = mpsc::channel();
    std::thread::scope(|scope| {
        // Full open waits for that same memo before beginning its write
        // transaction. The checkpoint open skips this preparation.
        let full = scope.spawn(|| ok(shared.open(&file.0)));
        let resumed = scope.spawn(|| {
            let connection = ok(open_existing(&file.0));
            let mut signalled = false;
            ok(
                connection.authorizer(Some(move |context: rusqlite::hooks::AuthContext<'_>| {
                    if !signalled
                        && matches!(
                            context.action,
                            AuthAction::Read {
                                table_name: "v2_upgrades",
                                ..
                            }
                        )
                    {
                        // In checkpoint reopen, this table is first read after
                        // BEGIN IMMEDIATE. The third connection starts only
                        // once that transaction really holds SQLite's lock.
                        signalled = true;
                        ok(entered_tx.send(()));
                    }
                    Authorization::Allow
                })),
            );
            ok(V2SqliteShell::reopen(
                connection,
                shared.members(shared.versions()),
                Some(&checkpoint),
            ))
        });
        let entered = in_tx.recv_timeout(Duration::from_secs(5));
        let wrote = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .and_then(rusqlite::Transaction::commit);
        // Always release the paused memo before assertions or joins so a
        // failing old implementation cannot strand the worker threads.
        drop(comparing);
        let full = full.join();
        let resumed = resumed.join();
        assert!(
            entered.is_ok(),
            "checkpoint never began its audit: {entered:?}"
        );
        assert!(
            wrote.is_ok(),
            "checkpoint held SQLite waiting for the shared memo: {wrote:?}"
        );
        assert!(
            matches!(full, Ok(Opened::V10(Store::Current(_)))),
            "full open did not reach the current contract"
        );
        let (_, version) = resumed.unwrap_or_else(|_| panic!("checkpoint open failed"));
        assert_eq!(version, 1);
    });
    assert_eq!(shared.comparisons(), 1, "both opens reuse one comparison");
    assert!(shared.settled(0, 1).is_some());
}
