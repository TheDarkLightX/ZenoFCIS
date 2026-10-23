//! Transaction, exact replay, root omission, CAS and abrupt-process controls.
#[path = "../service-app/store.rs"]
mod store;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use store::{Fault, Store};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_contract::{
    checked_composition::{
        LeafSources,
        service::{ServiceAuthority, ServiceRequest},
    },
    internals::{
        declarations::Declarations, model::Contract, policy, rules::Rules, schema,
        schema_commitment,
    },
};
use zeno_fcis_crypto::RustCryptoSha256;
fn fail<T>(e: impl std::fmt::Debug) -> T {
    panic!("test operation failed: {e:?}")
}
fn sha(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn authority() -> ServiceAuthority {
    authority_with_limit(1000000)
}
fn authority_with_limit(max_steps: u64) -> ServiceAuthority {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-contract/tests/fixtures/checked-composition-3/faults");
    let mut sources = Vec::new();
    let mut cores = Vec::new();
    let mut schemas = Vec::new();
    for (name, field, bindings) in [
        (
            "counter",
            "value",
            json!({"action":"150","authorized":"authorized"}),
        ),
        (
            "budget",
            "remaining",
            json!({"amount":"qty","authorized":"authorized"}),
        ),
    ] {
        let source = LeafSources {
            project: fs::read_to_string(root.join(name).join("project.zeno")).unwrap_or_else(fail),
            rules: fs::read_to_string(root.join(name).join("v2/policy.json")).unwrap_or_else(fail),
        };
        let rules = Rules::read(&source.rules).unwrap_or_else(fail);
        let declarations =
            Declarations::read(&source.project, &rules.leaf_bindings).unwrap_or_else(fail);
        let schema = schema::encode(&declarations).unwrap_or_else(fail);
        let commitment = schema_commitment(&schema).unwrap_or_else(fail);
        let contract = Contract::build(&declarations, &rules, commitment).unwrap_or_else(fail);
        let program =
            policy::with_authority(&contract, &schema, |a| sha(a.identity())).unwrap_or_else(fail);
        schemas.push(sha(&schema));
        cores.push(json!({"name":name,"path":name,"project_sha256":sha(source.project.as_bytes()),"policy_sha256":sha(source.rules.as_bytes()),"program_sha256":program,"owns":[format!("{name}.{field}")],"participates":"true","bind":bindings,"assume":"true","invariant":"true"}));
        sources.push(source);
    }
    let m = json!({"schema":"zeno-fcis/service/1","schemas":schemas,"max_steps":max_steps,"laws":[],"composition":{"schema":"zeno-fcis/checked-composition/3","checker":"zeno-fcis/composed-domain-checker/1","name":"atomic-pair","request":[{"name":"op","domain":{"kind":"int","min":0,"max":2}},{"name":"qty","domain":{"kind":"int","min":1,"max":2}},{"name":"authorized","domain":{"kind":"bool"}}],"components":cores,"couplings":[]}});
    ServiceAuthority::from_sources(&m.to_string(), &sources).unwrap_or_else(fail)
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    let root = std::env::var_os("ZFCIS_SERVICE_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "service-test-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap_or_else(fail);
    dir
}
fn request(key: &str, qty: i128, auth: i128) -> ServiceRequest {
    ServiceRequest::new("local", key, &[0, qty, auth]).unwrap_or_else(fail)
}
#[test]
fn one_core_refusal_leaves_both_states_and_journal_unchanged() {
    let dir = directory();
    let a = authority();
    let path = dir.join("state.db");
    let mut s = Store::open(&path, &a).unwrap_or_else(fail);
    let first = s.execute(&a, &request("one", 1, 1)).unwrap_or_else(fail);
    assert_eq!(first.version, 1);
    let before = s.snapshot(&a).unwrap_or_else(fail);
    assert_eq!(before.state.values(), [1, 1]);
    let error = s
        .execute(&a, &request("two", 2, 1))
        .err()
        .unwrap_or_else(|| panic!("budget refuses"));
    assert!(error.reason.starts_with("reject:budget"));
    assert!(error.usage.unwrap_or_default().core()[7] > 0);
    let after = s.snapshot(&a).unwrap_or_else(fail);
    assert_eq!(after.state, before.state);
    assert_eq!(after.version, before.version);
    assert_eq!(after.root, before.root);
    assert!(
        s.recover(&request("two", 2, 1))
            .unwrap_or_else(fail)
            .is_none()
    );
    s.audit(&a).unwrap_or_else(fail);
    drop(s);
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}
#[test]
fn receipt_covers_all_roots_and_fresh_journal_replay_is_exact() {
    let dir = directory();
    let a = authority();
    let path = dir.join("source.db");
    let mut s = Store::open(&path, &a).unwrap_or_else(fail);
    let one = s.execute(&a, &request("one", 1, 1)).unwrap_or_else(fail);
    let two = s.execute(&a, &request("two", 1, 1)).unwrap_or_else(fail);
    assert_ne!(one.receipt, two.receipt);
    drop(s);
    let reopened = Store::open(&path, &a);
    assert!(
        reopened.is_ok(),
        "replay must validate every core root: {reopened:?}"
    );
    let s = reopened.unwrap_or_else(fail);
    let mut fresh = Store::open(&dir.join("fresh.db"), &a).unwrap_or_else(fail);
    assert_eq!(s.replay_into(&a, &mut fresh).unwrap_or_else(fail), 2);
    assert_eq!(
        s.snapshot(&a).unwrap_or_else(fail).state,
        fresh.snapshot(&a).unwrap_or_else(fail).state
    );
    assert_eq!(
        fresh
            .recover(&request("two", 1, 1))
            .unwrap_or_else(fail)
            .unwrap_or_else(|| panic!("receipt"))
            .receipt,
        two.receipt
    );
    drop(fresh);
    drop(s);
    let db = zeno_fcis_shell_sqlite::sqlite::Connection::open(&path).unwrap_or_else(fail);
    let mut receipt: Vec<u8> = db
        .query_row(
            "SELECT receipt FROM service_journal WHERE version=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(fail);
    let id = receipt
        .windows(6)
        .position(|w| w == b"budget")
        .unwrap_or_else(|| panic!("core id"));
    let post = id + 6 + 32 * 3;
    receipt.drain(post..post + 32);
    db.execute(
        "UPDATE service_journal SET receipt=?1 WHERE version=1",
        [receipt],
    )
    .unwrap_or_else(fail);
    drop(db);
    assert!(
        Store::open(&path, &a).is_err(),
        "missing post-root must be detected"
    );
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}
#[test]
fn lost_ack_recovery_and_stale_global_cas_preserve_accounting() {
    let dir = directory();
    let a = authority();
    let path = dir.join("state.db");
    let mut s = Store::open(&path, &a).unwrap_or_else(fail);
    let snapshot = s.snapshot(&a).unwrap_or_else(fail);
    let command = request("one", 1, 1);
    let stale = a
        .decide(&snapshot.state, &request("stale", 1, 1), snapshot.version)
        .unwrap_or_else(fail);
    let publication = a
        .decide(&snapshot.state, &command, snapshot.version)
        .unwrap_or_else(fail);
    let receipt = publication.receipt();
    let error = s
        .commit_at(&a, publication, Some(Fault::AfterCommit), false)
        .err()
        .unwrap_or_else(|| panic!("lost ack"));
    assert!(error.usage.unwrap_or_default().core()[7] > 0);
    let recovered = s.execute(&a, &command).unwrap_or_else(fail);
    assert!(recovered.recovered);
    assert_eq!(recovered.receipt, receipt);
    assert_eq!(recovered.version, 1);
    let conflict = s
        .commit(&a, stale)
        .err()
        .unwrap_or_else(|| panic!("stale CAS"));
    assert_eq!(conflict.reason, "compare-and-swap");
    assert!(conflict.usage.unwrap_or_default().core()[7] > 0);
    assert!(s.execute(&a, &request("one", 2, 1)).is_err());
    s.audit(&a).unwrap_or_else(fail);
    drop(s);
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}
#[test]
fn revocation_linearizes_a_read_through_the_global_commit_fence() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/composition-service/runtime");
    let text = fs::read_to_string(root.join("service.json")).unwrap_or_else(fail);
    let m: Value = serde_json::from_str(&text).unwrap_or_else(fail);
    let sources = m["composition"]["components"]
        .as_array()
        .unwrap_or_else(|| panic!("cores"))
        .iter()
        .map(|c| {
            let d = root.join(c["path"].as_str().unwrap_or(""));
            LeafSources {
                project: fs::read_to_string(d.join("project.zeno")).unwrap_or_else(fail),
                rules: fs::read_to_string(d.join("v2/policy.json")).unwrap_or_else(fail),
            }
        })
        .collect::<Vec<_>>();
    let a = ServiceAuthority::from_sources(&text, &sources).unwrap_or_else(fail);
    let dir = directory();
    let mut s = Store::open(&dir.join("mail.db"), &a).unwrap_or_else(fail);
    for (key, op, target) in [("publish", 0, 1), ("anchor", 3, 1), ("deliver", 4, 1)] {
        let r = ServiceRequest::new("local", key, &[op, target]).unwrap_or_else(fail);
        s.execute(&a, &r).unwrap_or_else(fail);
    }
    let pre = s.snapshot(&a).unwrap_or_else(fail);
    let read = ServiceRequest::new("local", "read", &[5, 1]).unwrap_or_else(fail);
    let stale = a
        .decide(&pre.state, &read, pre.version)
        .unwrap_or_else(fail);
    s.execute(
        &a,
        &ServiceRequest::new("local", "revoke", &[2, 1]).unwrap_or_else(fail),
    )
    .unwrap_or_else(fail);
    assert_eq!(
        s.commit(&a, stale)
            .err()
            .unwrap_or_else(|| panic!("read fence"))
            .reason,
        "compare-and-swap"
    );
    assert!(s.execute(&a, &read).is_err());
    s.audit(&a).unwrap_or_else(fail);
    drop(s);
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("ZFCIS_CRASH_PATH") else {
        return;
    };
    let index = std::env::var("ZFCIS_CRASH_INDEX")
        .unwrap_or_else(fail)
        .parse::<usize>()
        .unwrap_or_else(fail);
    let a = authority();
    let mut s = Store::open(Path::new(&path), &a).unwrap_or_else(fail);
    let snapshot = s.snapshot(&a).unwrap_or_else(fail);
    let publication = a
        .decide(&snapshot.state, &request("crash", 1, 1), snapshot.version)
        .unwrap_or_else(fail);
    let faults = [
        Fault::BeforeTransaction,
        Fault::AfterJournal,
        Fault::AfterHead,
        Fault::BeforeCommit,
        Fault::AfterCommit,
    ];
    let _ = s.commit_at(&a, publication, Some(faults[index]), true);
    panic!("abort did not happen");
}
#[test]
#[cfg(unix)]
fn abrupt_process_exit_at_every_write_and_commit_boundary() {
    use std::os::unix::process::ExitStatusExt;
    let a = authority();
    let dir = directory();
    for index in 0..5 {
        let path = dir.join(format!("crash-{index}.db"));
        drop(Store::open(&path, &a).unwrap_or_else(fail));
        let output = std::process::Command::new(std::env::current_exe().unwrap_or_else(fail))
            .args(["--exact", "crash_child", "--nocapture"])
            .env("ZFCIS_CRASH_PATH", &path)
            .env("ZFCIS_CRASH_INDEX", index.to_string())
            .output()
            .unwrap_or_else(fail);
        assert_eq!(
            output.status.signal(),
            Some(6),
            "expected planted abort, got {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let reopened = Store::open(&path, &a).unwrap_or_else(fail);
        let snapshot = reopened.snapshot(&a).unwrap_or_else(fail);
        assert_eq!(snapshot.version, if index == 4 { 1 } else { 0 });
        assert_eq!(
            snapshot.state.values(),
            if index == 4 { &[1, 1] } else { &[0, 2] }
        );
        reopened.audit(&a).unwrap_or_else(fail);
    }
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}

#[test]
fn recovery_cannot_activate_a_different_service_identity() {
    let dir = directory();
    let path = dir.join("version.db");
    let a = authority();
    let other = authority_with_limit(2000000);
    assert_ne!(a.identity(), other.identity());
    let mut store = Store::open(&path, &a).unwrap_or_else(fail);
    let command = request("one", 1, 1);
    store.execute(&a, &command).unwrap_or_else(fail);
    assert_eq!(
        store
            .execute(&other, &command)
            .err()
            .unwrap_or_else(|| panic!("identity refusal"))
            .reason,
        "service-identity"
    );
    drop(store);
    assert!(Store::open(&path, &other).is_err());
    drop(Store::open(&path, &a).unwrap_or_else(fail));
    fs::remove_dir_all(dir).unwrap_or_else(fail);
}
