//! Ported behavior tests for the shared Python-compatible store.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use zeno_fcis_commons::{
    ARCHIVE_REFUSAL, Commons, CommonsError, DB_NAME, MAX_TEXT, canonical_json, normalize_resource,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "zeno-commons-integration-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        good(fs::create_dir_all(&path));
        Self(path)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn new(at: f64) -> Self {
        Self(Arc::new(AtomicU64::new(at.to_bits())))
    }

    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::SeqCst))
    }

    fn advance(&self, seconds: f64) {
        let _ = self
            .0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                Some((f64::from_bits(old) + seconds).to_bits())
            })
            .ok();
    }

    fn function(&self) -> Arc<dyn Fn() -> f64 + Send + Sync> {
        let clock = self.clone();
        Arc::new(move || clock.now())
    }
}

fn good<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn refused<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("expected operation to be refused"),
        Err(error) => error,
    }
}

fn open(directory: &TestDir, actor: &str, clock: &TestClock) -> Commons {
    let mut store = good(Commons::open_with_clock(
        directory.path(),
        actor,
        clock.function(),
    ));
    good(store.register("", "agent", "test"));
    store
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn record_add(store: &mut Commons, kind: &str, title: &str, tags: &[&str]) -> String {
    let result = good(store.record_add(kind, title, "", "open", &strings(tags), &[], ""));
    result["id"].as_str().unwrap_or_default().to_string()
}

#[test]
fn concurrent_writers_lose_nothing_and_sequence_only_grows() {
    let directory = TestDir::new();
    drop(good(Commons::open(directory.path(), "setup")));
    let writers = (0..8)
        .map(|number| {
            let path = directory.path().clone();
            thread::spawn(move || {
                let mut store = good(Commons::open(path, &format!("w{number}")));
                for index in 0..60 {
                    let sequence = good(store.emit(
                        "test",
                        "tick",
                        &format!("w{number}-{index}"),
                        json!({"i": index}),
                    ));
                    assert!(sequence > 0);
                }
            })
        })
        .collect::<Vec<_>>();
    for writer in writers {
        assert!(writer.join().is_ok());
    }
    let store = good(Commons::open(directory.path(), "reader"));
    let events = good(store.events(0, "", 10_000));
    assert_eq!(events.len(), 480);
    assert_eq!(
        events
            .iter()
            .map(|event| event["seq"].as_i64().unwrap_or_default())
            .collect::<Vec<_>>(),
        (1..=480).collect::<Vec<_>>()
    );
    let mut per_actor = std::collections::BTreeMap::<String, Vec<i64>>::new();
    for event in events {
        per_actor
            .entry(event["actor"].as_str().unwrap_or_default().to_string())
            .or_default()
            .push(event["body"]["i"].as_i64().unwrap_or_default());
    }
    assert_eq!(
        per_actor.keys().cloned().collect::<Vec<_>>(),
        (0..8).map(|n| format!("w{n}")).collect::<Vec<_>>()
    );
    for values in per_actor.values() {
        assert_eq!(values, &(0..60).collect::<Vec<_>>());
    }
    assert_eq!(good(store.verify())["ok"], true);
}

#[test]
fn lease_conflict_expiry_renew_release() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut owner = open(&directory, "builder-a", &clock);
    let mut other = open(&directory, "builder-b", &clock);
    good(owner.lease_take("/tmp/zf-x", 600.0, ""));
    assert!(other.lease_take("/tmp/zf-x", 3_600.0, "").is_err());
    good(owner.lease_renew("/tmp/zf-x", 600.0));
    clock.advance(601.0);
    let held = good(other.lease_check("/tmp/zf-x"));
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["state"], "stale");
    let error = refused(other.lease_take("/tmp/zf-x", 3_600.0, ""));
    assert!(error.to_string().contains("never released"));
    let taken = good(other.lease_take_force(
        "/tmp/zf-x",
        3_600.0,
        "",
        "builder-a posted finished, workspace released",
    ));
    assert_eq!(taken["displaced"], json!([["/tmp/zf-x", "builder-a"]]));
    let event = good(other.events(0, "lease.take", 100))
        .into_iter()
        .find(|event| event["actor"] == "builder-b")
        .unwrap_or_default();
    assert_eq!(
        event["body"]["force_reason"],
        "builder-a posted finished, workspace released"
    );
    assert!(owner.lease_release("/tmp/zf-x", "").is_err());
    good(owner.lease_release("/tmp/zf-x", "owner confirmed it finished"));
    assert!(good(other.leases()).is_empty());
}

#[test]
fn identities_have_unique_names_hash_ids_and_support_name_or_id_delivery() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut first = open(&directory, "builder-a", &clock);
    let second = open(&directory, "builder-b", &clock);
    let first_id = good(first.identity(""))["id"].clone();
    let second_id = good(second.identity(""))["id"].clone();
    let first_id_text = first_id.as_str().unwrap_or_default();
    assert_eq!(first_id_text.len(), 19);
    assert!(first_id_text.starts_with("ag-"));
    assert!(
        first_id_text[3..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert_ne!(first_id, second_id);
    let registrations = good(first.events(0, "identity.register", 10));
    assert_eq!(registrations.len(), 2);
    for registration in &registrations {
        assert_eq!(registration["body"]["kind"], "agent");
        assert_eq!(registration["body"]["role"], "test");
        assert_eq!(
            registration["body"]["nonce"]
                .as_str()
                .unwrap_or_default()
                .len(),
            32
        );
    }
    assert!(first.register("builder-b", "agent", "").is_err());
    assert_eq!(
        good(first.send(second_id.as_str().unwrap_or_default(), "by id"))["to"],
        "builder-b"
    );
    good(first.send("builder-b", "by name"));
    assert!(
        good(second.inbox(false))["messages"]
            .as_array()
            .is_some_and(|messages| messages.len() == 2)
    );
    assert!(first.send("nobody", "unknown").is_err());
    let agents = good(first.agents());
    assert_eq!(agents.len(), 2);
    assert!(agents.iter().any(|agent| agent["id"] == first_id));
}

#[test]
fn unregistered_actors_cannot_coordinate_but_emit_is_allowed() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut ghost = good(Commons::open_with_clock(
        directory.path(),
        "ghost",
        clock.function(),
    ));
    assert!(ghost.lease_take("/tmp/work-ghost", 3_600.0, "").is_err());
    assert!(ghost.status("working", "", "").is_err());
    assert!(
        ghost
            .record_add("lesson", "x", "", "open", &[], &[], "")
            .is_err()
    );
    assert!(ghost.record_update("missing", "", "", &[]).is_err());
    assert!(ghost.link("missing", "relates", "other").is_err());
    assert!(ghost.lease_renew("/tmp/work-ghost", 3_600.0).is_err());
    assert!(ghost.lease_release("/tmp/work-ghost", "").is_err());
    assert!(ghost.ack(0).is_err());
    assert!(ghost.send("lead", "hello").is_err());
    assert!(good(ghost.emit("atdd", "result", "scenario-x", json!({"exit": 0}))) > 0);
}

#[test]
fn imported_gap_tags_are_cleaned_to_python_names() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    let path = directory.path().join("gaps.jsonl");
    good(fs::write(
        &path,
        r#"{"agent":"Pilot A!","severity":"Blocker! Hi","need":"clean tags"}"#,
    ));
    good(store.import_gaps(&path));
    let record = good(store.records("gap", "", "", 10)).remove(0);
    assert_eq!(
        record["tags"],
        json!([
            "feature-gap",
            "pilot-a",
            "reporter-pilot-a",
            "severity-blocker-hi"
        ])
    );
}

#[test]
fn duplicate_link_folds_once_and_selects_highest_severity() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut first = open(&directory, "sec-review", &clock);
    let mut second = open(&directory, "zenomail", &clock);
    let canonical = good(first.record_add(
        "gap",
        "Judges cannot see deliveries",
        "lemmas see state only",
        "open",
        &strings(&["judges", "severity-medium"]),
        &strings(&["log-a"]),
        "",
    ))["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let duplicate = good(second.record_add(
        "gap",
        "Lemmas cannot observe deliveries a decision makes",
        "paid at most once",
        "open",
        &strings(&["security", "severity-high"]),
        &strings(&["log-b"]),
        "",
    ))["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    good(second.link(&duplicate, "duplicates", &canonical));
    good(second.link(&duplicate, "duplicates", &canonical));
    let merged = good(first.record(&canonical));
    assert_eq!(good(first.record(&duplicate))["status"], "superseded");
    assert_eq!(
        merged["tags"],
        json!(["judges", "security", "severity-high"])
    );
    let mut expected_refs = vec!["log-a".to_string(), "log-b".to_string(), duplicate.clone()];
    expected_refs.sort();
    assert_eq!(merged["refs"], json!(expected_refs));
    assert_eq!(
        merged["body"]
            .as_str()
            .unwrap_or_default()
            .matches("folded in duplicate")
            .count(),
        1
    );
    let gaps = good(first.gaps(false, 200));
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["id"], canonical);
    assert_eq!(gaps[0]["severity"], "high");
    assert_eq!(gaps[0]["reporters"], json!(["sec-review", "zenomail"]));
    let error = refused(first.link(&canonical, "duplicates", &duplicate));
    assert!(error.to_string().contains("itself superseded"));
}

#[test]
fn imported_gap_uses_original_reporter_and_reimport_writes_no_event() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    let path = directory.path().join("gaps.jsonl");
    good(fs::write(
        &path,
        r#"{"agent":"sec-review","severity":"high","need":"Lemmas cannot see deliveries"}"#,
    ));
    good(store.import_gaps(&path));
    let gaps = good(store.gaps(false, 200));
    assert_eq!(gaps[0]["reporters"], json!(["sec-review"]));
    let events = good(store.events(0, "", 1_000)).len();
    good(store.import_gaps(&path));
    assert_eq!(good(store.events(0, "", 1_000)).len(), events);
}

#[test]
fn gaps_rank_by_severity_then_reporters_then_age() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut first = open(&directory, "agent-a", &clock);
    let mut second = open(&directory, "agent-b", &clock);
    let low = record_add(&mut first, "gap", "Low one", &["severity-low"]);
    let unrated = record_add(&mut first, "gap", "Unrated one", &[]);
    let high_old = record_add(&mut first, "gap", "High old", &["severity-high"]);
    let high_new = record_add(&mut first, "gap", "High new", &["severity-high"]);
    let twice = record_add(&mut first, "gap", "High twice", &["severity-high"]);
    let echo = record_add(
        &mut second,
        "gap",
        "High twice, again",
        &["severity-medium"],
    );
    let blocker = record_add(&mut second, "gap", "Blocker", &["severity-blocker"]);
    good(second.link(&echo, "duplicates", &twice));
    good(first.record_update_full(&unrated, "", "", &[], "", "medium", &[]));
    let done = record_add(&mut first, "gap", "Already fixed", &["severity-blocker"]);
    good(first.record_update(&done, "resolved", "", &[]));
    let order = good(first.gaps(false, 200))
        .into_iter()
        .map(|gap| gap["id"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(order, [blocker, twice, high_old, high_new, unrated, low]);
    assert!(
        good(first.gaps(true, 200))
            .iter()
            .any(|gap| gap["id"] == done)
    );
}

#[test]
fn dedup_proposes_similar_gaps_and_leaves_distinct_ones() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    let older = record_add(
        &mut store,
        "gap",
        "Per-request work grows with the store's history",
        &[],
    );
    let newer = good(store.record_add(
        "gap",
        "Serve state and command in time that does not grow with the store's history",
        "replays every decision on each request",
        "open",
        &[],
        &[],
        "",
    ))["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    good(store.record_add(
        "gap",
        "Show token amounts with decimals",
        "1.5 tokens at scale e18",
        "open",
        &[],
        &[],
        "",
    ));
    let pairs = good(store.similar("gap", 0.2, 100));
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["older"], older);
    assert_eq!(pairs[0]["newer"], newer);
}

#[test]
fn record_title_correction_and_size_cap() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    let id = good(store.record_add(
        "lesson",
        "wrong cause named",
        "first body",
        "open",
        &[],
        &[],
        "",
    ))["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    good(store.record_update_title(
        &id,
        "",
        "the lead deleted it, not the agent",
        &[],
        "right cause named",
    ));
    let record = good(store.record(&id));
    assert_eq!(record["title"], "right cause named");
    assert!(
        record["body"]
            .as_str()
            .unwrap_or_default()
            .contains("title was: wrong cause named")
    );
    let body = record["body"].as_str().unwrap_or_default().to_string();
    let note = "x".repeat(MAX_TEXT - body.chars().count());
    assert!(store.record_update_title(&id, "", &note, &[], "").is_err());
    assert_eq!(good(store.record(&id))["body"], body);
}

#[test]
fn anchors_detect_rewritten_history_and_removed_events() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    for index in 0..6 {
        good(store.emit("atdd", "result", &format!("s-{index}"), json!({"exit": 0})));
    }
    let anchor = good(store.head());
    assert_eq!(
        good(store.verify_with(false, std::slice::from_ref(&anchor)))["ok"],
        true
    );
    let connection = good(Connection::open(directory.path().join(DB_NAME)));
    let mut statement =
        good(connection.prepare("SELECT seq,at,actor,kind,subject,body FROM events ORDER BY seq"));
    let rows = good(
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>()),
    );
    let mut previous = "0".repeat(64);
    for (seq, at, actor, kind, subject, text) in &rows {
        let mut body: serde_json::Value = good(serde_json::from_str(text));
        if *seq == 3 {
            body = json!({"exit": 1});
        }
        let body_text = good(canonical_json(&body));
        let payload = good(canonical_json(&json!([at, actor, kind, subject, body])));
        let digest = Sha256::digest(format!("{previous}{payload}").as_bytes());
        let mut hash = String::new();
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(hash, "{byte:02x}");
        }
        good(connection.execute(
            "UPDATE events SET body=?1,prev=?2,hash=?3 WHERE seq=?4",
            rusqlite::params![body_text, previous, hash, seq],
        ));
        previous = hash;
    }
    assert_eq!(good(store.verify())["ok"], true);
    let rewritten = good(store.verify_with(false, std::slice::from_ref(&anchor)));
    assert_eq!(rewritten["ok"], false);
    assert!(
        rewritten["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("rewritten")
    );
    good(connection.execute(
        "DELETE FROM events WHERE seq=(SELECT MAX(seq) FROM events)",
        [],
    ));
    let removed = good(store.verify_with(false, std::slice::from_ref(&anchor)));
    assert_eq!(removed["ok"], false);
    assert!(
        removed["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("removed")
    );
}

#[cfg(unix)]
#[test]
fn store_refuses_links_and_foreign_schemas_before_writing() {
    let directory = TestDir::new();
    let other = directory.path().join("other.sqlite3");
    let raw = good(Connection::open(&other));
    good(raw.execute("CREATE TABLE notes(x)", []));
    drop(raw);
    let linked = directory.path().join("linked");
    good(fs::create_dir(&linked));
    good(symlink(&other, linked.join(DB_NAME)));
    assert!(Commons::open(&linked, "lead").is_err());

    let foreign = directory.path().join("foreign");
    good(fs::create_dir(&foreign));
    let raw = good(Connection::open(foreign.join(DB_NAME)));
    good(raw.execute("CREATE TABLE notes(x)", []));
    drop(raw);
    let error = match Commons::open(&foreign, "lead") {
        Ok(_) => panic!("foreign schema was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("not Commons tables"));
    let raw = good(Connection::open(foreign.join(DB_NAME)));
    let mut statement = good(raw.prepare("SELECT name FROM sqlite_master"));
    let names: Vec<String> = good(
        statement
            .query_map([], |row| row.get(0))
            .and_then(|rows| rows.collect()),
    );
    assert_eq!(names, ["notes"]);

    let clean = directory.path().join("clean");
    let store = good(Commons::open(&clean, "lead"));
    drop(store);
    let raw = good(Connection::open(clean.join(DB_NAME)));
    good(
        raw.execute_batch(
            "CREATE TRIGGER t AFTER INSERT ON events BEGIN DELETE FROM records; END;",
        ),
    );
    drop(raw);
    let error = match Commons::open(&clean, "lead") {
        Ok(_) => panic!("foreign trigger was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("trigger t"));
    let raw = good(Connection::open(clean.join(DB_NAME)));
    good(raw.execute("DROP TRIGGER t", []));
    drop(raw);
    good(symlink(directory.path(), clean.join("archive")));
    assert!(Commons::open(&clean, "lead").is_err());
}

#[test]
fn lease_on_a_folder_protects_its_parent_and_children() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut reviewer = open(&directory, "reviewer", &clock);
    let lead = open(&directory, "lead", &clock);
    good(reviewer.lease_take("/tmp/work-sec/source", 3_600.0, ""));
    assert!(!good(lead.lease_check("/tmp/work-sec")).is_empty());
    assert!(!good(lead.lease_check("/tmp/work-sec/source/x")).is_empty());
    assert!(good(lead.lease_check("/tmp/work-secondary")).is_empty());
    let mut lead = lead;
    assert!(lead.lease_take("/tmp/work-sec", 3_600.0, "").is_err());
}

#[test]
fn done_requires_released_leases() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "builder", &clock);
    good(store.lease_take("branch:dev/2.6-x", 3_600.0, ""));
    assert!(store.status("done", "", "").is_err());
    clock.advance(3_601.0);
    assert!(store.status("done", "", "").is_err());
    good(store.lease_release("branch:dev/2.6-x", ""));
    assert_eq!(good(store.status("done", "", ""))["state"], "done");
    assert!(store.status("blocked", "", "").is_err());
}

#[test]
fn messages_and_read_receipts_only_move_forward() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut lead = open(&directory, "lead", &clock);
    let mut builder = open(&directory, "builder", &clock);
    let first = good(lead.send("builder", "use option A"))["seq"]
        .as_i64()
        .unwrap_or_default();
    let second = good(lead.send("*", "freeze moved to Friday"))["seq"]
        .as_i64()
        .unwrap_or_default();
    let inbox = good(builder.inbox(false));
    assert_eq!(inbox["messages"][0]["seq"], first);
    assert_eq!(inbox["messages"][1]["seq"], second);
    good(builder.ack(second));
    good(builder.ack(first));
    assert_eq!(good(builder.inbox(false))["read_through"], second);
    assert!(
        good(builder.inbox(false))["messages"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(
        good(lead.inbox(false))["messages"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(builder.ack(10_000).is_err());
}

#[test]
fn records_links_and_supersession() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "builder", &clock);
    let hypothesis = record_add(
        &mut store,
        "hypothesis",
        "Adaptive indexing halves search work",
        &["synthesis"],
    );
    let refutation = good(store.record_add(
        "refutation",
        "No gain below 300k comparisons",
        "",
        "open",
        &[],
        &strings(&["evidence/REPORT.md"]),
        "",
    ))["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    good(store.link(&refutation, "refutes", &hypothesis));
    good(store.record_update(
        &hypothesis,
        "refuted",
        "see refutation",
        &strings(&[&refutation]),
    ));
    let shown = good(store.record(&hypothesis));
    assert_eq!(shown["status"], "refuted");
    assert!(shown["links"].as_array().is_some_and(|links| {
        links.contains(&json!({"src": refutation, "rel": "refutes", "dst": hypothesis}))
    }));
    let new = record_add(
        &mut store,
        "lesson",
        "Create RAM-disk roots before launch",
        &["codex-launch"],
    );
    let old = record_add(
        &mut store,
        "lesson",
        "Launch builders directly",
        &["codex-launch"],
    );
    good(store.link(&new, "supersedes", &old));
    assert_eq!(good(store.record(&old))["status"], "superseded");
}

#[test]
fn advise_returns_negative_memory_for_the_action() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut lead = open(&directory, "lead", &clock);
    good(lead.record_add(
        "lesson",
        "Create the RAM-disk root before launching a sandboxed builder",
        "Otherwise each command gets a private /dev/shm and rebuilds.",
        "open",
        &strings(&["codex-launch", "ramdisk"]),
        &[],
        "",
    ));
    good(lead.record_add(
        "lesson",
        "Never run cargo fmt --all in a lane worktree",
        "",
        "open",
        &strings(&["cargo"]),
        &[],
        "",
    ));
    good(lead.record_add(
        "hypothesis",
        "Unrelated claim about launch timing",
        "",
        "open",
        &[],
        &[],
        "",
    ));
    let mut builder = open(&directory, "builder", &clock);
    good(builder.lease_take("/tmp/work-ui", 3_600.0, ""));
    let advice = good(lead.advise(
        "launch a sandboxed builder",
        &strings(&["codex-launch"]),
        "/tmp/work-ui cleanup",
        10,
    ));
    assert_eq!(
        advice["advice"][0]["title"],
        "Create the RAM-disk root before launching a sandboxed builder"
    );
    assert!(!advice["advice"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["title"] == "Unrelated claim about launch timing")
    }));
    assert_eq!(advice["leases"][0]["owner"], "builder");
}

#[test]
fn superseded_lessons_are_not_advice() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    let old = record_add(
        &mut store,
        "lesson",
        "Run the sweep on the RAM disk",
        &["sweep"],
    );
    good(store.record_update(&old, "superseded", "", &[]));
    assert!(
        good(store.advise("run the sweep", &strings(&["sweep"]), "", 10))["advice"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
}

#[test]
fn import_is_idempotent_and_writes_no_events_the_second_time() {
    let directory = TestDir::new();
    let log = directory.path().join("gaps.jsonl");
    good(fs::write(
        &log,
        "{\"schema\":\"zenofcis-feature-gap/1\"}\n{\"agent\":\"pilot-a\",\"severity\":\"blocker\",\"need\":\"A mailbox holding many messages\",\"what_zenofcis_lacks\":\"collections\"}\n{\"agent\":\"pilot-b\",\"severity\":\"high\",\"need\":\"u128 amounts in the UI\"}\n",
    ));
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    assert_eq!(
        good(store.import_gaps(&log)),
        json!({"added": 2, "existing": 0})
    );
    let before = good(store.events(0, "", 10_000)).len();
    assert_eq!(
        good(store.import_gaps(&log)),
        json!({"added": 0, "existing": 2})
    );
    assert_eq!(good(store.events(0, "", 10_000)).len(), before);
    assert_eq!(
        good(store.records("gap", "", "severity-blocker", 100)).len(),
        1
    );
}

#[test]
fn tampering_with_history_is_detected() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    for index in 0..5 {
        good(store.emit(
            "atdd",
            "result",
            &format!("scenario-{index}"),
            json!({"exit": 0}),
        ));
    }
    assert_eq!(good(store.verify())["ok"], true);
    let raw = good(Connection::open(directory.path().join(DB_NAME)));
    good(raw.execute("UPDATE events SET body=?1 WHERE seq=3", ["{\"exit\":1}"]));
    assert_eq!(
        good(store.verify()),
        json!({"ok": false, "broken_at": 3, "checked": 2, "reason": "hash chain mismatch"})
    );
}

#[test]
fn verify_uses_latest_checkpoint_and_checks_live_archive_event() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let store = open(&directory, "lead", &clock);
    let raw = good(Connection::open(directory.path().join(DB_NAME)));
    let archived_head: String =
        good(raw.query_row("SELECT hash FROM events WHERE seq=1", [], |row| row.get(0)));
    drop(raw);
    let segment = "events-000000000001-000000000001.jsonl.xz";
    let body = json!({
        "first": 1,
        "last": 1,
        "events": 1,
        "prev": "0".repeat(64),
        "head": archived_head,
        "segment": segment,
        "sha256": "a".repeat(64),
        "bytes": 42
    });
    let at = 1_800_000_000.0;
    let payload = good(canonical_json(&json!([
        at,
        "lead",
        "store.archived",
        segment,
        body.clone()
    ])));
    let digest = Sha256::digest(format!("{archived_head}{payload}").as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let raw = good(Connection::open(directory.path().join(DB_NAME)));
    let body_text = good(canonical_json(&body));
    good(raw.execute(
        "INSERT INTO events(at,actor,kind,subject,body,prev,hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![
            at,
            "lead",
            "store.archived",
            segment,
            body_text,
            archived_head,
            digest
        ],
    ));
    good(raw.execute(
        "INSERT INTO checkpoints VALUES(1,1,?1,?2,?3,?4,42,1,2)",
        rusqlite::params!["0".repeat(64), archived_head, segment, "a".repeat(64)],
    ));
    good(raw.execute("DELETE FROM events WHERE seq=1", []));
    drop(raw);
    let verified = good(store.verify());
    assert_eq!(verified["ok"], true);
    assert_eq!(verified["from_seq"], 2);
    assert_eq!(verified["checked"], 1);
    let raw = good(Connection::open(directory.path().join(DB_NAME)));
    good(raw.execute("UPDATE checkpoints SET sha256=?1", ["f".repeat(64)]));
    let stored_sha: String = good(raw.query_row(
        "SELECT sha256 FROM checkpoints WHERE last=1",
        [],
        |row| row.get(0),
    ));
    assert_eq!(stored_sha, "f".repeat(64));
    drop(raw);
    let mismatch = good(store.verify());
    assert_eq!(mismatch["ok"], false);
    assert_eq!(
        mismatch["reason"],
        "an archive record disagrees with its checkpoint"
    );
}

#[test]
fn archive_io_refuses_with_reference_tool_message() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    assert_eq!(
        refused(store.archive(Some(1), true, false)).to_string(),
        ARCHIVE_REFUSAL
    );
    assert_eq!(
        refused(store.verify_with(true, &[])).to_string(),
        ARCHIVE_REFUSAL
    );
    assert_eq!(
        refused(store.export_events(true)).to_string(),
        ARCHIVE_REFUSAL
    );
}

#[test]
fn refusals() {
    let directory = TestDir::new();
    let clock = TestClock::new(1_800_000_000.0);
    let mut store = open(&directory, "lead", &clock);
    assert!(
        store
            .record_add("opinion", "x", "", "open", &[], &[], "")
            .is_err()
    );
    assert!(
        store
            .record_add("lesson", "  ", "", "open", &[], &[], "")
            .is_err()
    );
    assert!(store.status("sleeping", "", "").is_err());
    assert!(store.link("a", "likes", "b").is_err());
    assert!(store.lease_take("/tmp/x", 1.0, "").is_err());
    assert!(store.send("bad name!", "hi").is_err());
    assert!(Commons::open(directory.path(), "bad actor name").is_err());
}

#[test]
fn paths_normalize() {
    assert_eq!(good(normalize_resource("/tmp/a/../b/")), "/tmp/b");
    assert_eq!(good(normalize_resource("branch:dev/2.6")), "branch:dev/2.6");
}

#[test]
fn commons_error_message_is_stable() {
    let error = CommonsError::new("refused");
    assert_eq!(error.to_string(), "refused");
}
