//! Real SQLite write failures and child-process reservation recovery.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::{
    driver::Binding,
    durable::{self, Error, Relay, StateStore},
    handler::{Handled, Handling, Invocation, Terminal},
    policy::Policy,
    port::{Code, Fault, OpId, Operation, Response, Sent},
    record::Manifest,
    registry::Port,
    simulation,
};
use zeno_fcis_shell_sqlite::effects::durable::SqliteStateStore;

fn ok<T, E: core::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}
fn policies() -> Vec<Policy> {
    vec![Policy::AtMostOne { capacity: 2 }]
}
fn manifest() -> Manifest {
    Manifest::current(
        vec!["record", "simulation"],
        vec![],
        simulation::STORE,
        vec![],
    )
}
fn operation() -> Operation {
    Operation::Send(
        simulation::store(1),
        simulation::deliveries(1, Hash32::new([1; 32]), &[1])[0].clone(),
    )
}
fn binding() -> Binding {
    Binding::of(OpId::first(1), &operation())
}
fn open(path: &Path) -> Relay<SqliteStateStore> {
    ok(Relay::open(
        ok(SqliteStateStore::open(path)),
        simulation::STORE,
        &policies(),
        manifest(),
    ))
}
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "effect-durable-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        ok(fs::create_dir(&path));
        Self(path)
    }
    fn store(&self) -> PathBuf {
        self.0.join("reservations.sqlite")
    }
    fn sends(&self) -> PathBuf {
        self.0.join("sends")
    }
    fn create(&self) {
        drop(ok(SqliteStateStore::create(
            self.store(),
            &ok(durable::initial(simulation::STORE, &policies())),
        )));
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Wire {
    sends: PathBuf,
    stage: String,
    calls: usize,
}
impl Terminal for Wire {
    fn ports(&self) -> &'static [Port] {
        &Port::ALL
    }
    fn handle(&mut self, b: &Binding, op: &Operation) -> Handled {
        self.handle_with_invocation(b, op).outcome
    }
    fn handle_with_invocation(&mut self, _: &Binding, _: &Operation) -> Handling {
        if self.stage == "before-forward" {
            std::process::exit(73);
        }
        if self.stage.starts_with("release") {
            return Handling {
                outcome: Handled::Suspend,
                invocation: Invocation::NotInvoked,
            };
        }
        self.calls += 1;
        let mut file = ok(fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.sends));
        ok(file.write_all(b"send\n"));
        ok(file.sync_all());
        if self.stage == "after-forward" {
            std::process::exit(73);
        }
        Handling {
            outcome: Handled::Answer(Response::Send(Sent::Unknown(Fault::Io))),
            invocation: Invocation::MayHaveInvoked,
        }
    }
}
struct CutStore {
    inner: SqliteStateStore,
    stage: String,
    writes: usize,
}
impl StateStore for CutStore {
    fn load(&mut self) -> Result<Vec<u8>, Error> {
        self.inner.load()
    }
    fn commit(&mut self, expected: &[u8], next: &[u8]) -> Result<(), Error> {
        self.writes += 1;
        if (self.stage == "before-reserve" && self.writes == 1)
            || (self.stage == "release-before" && self.writes == 2)
        {
            std::process::exit(73);
        }
        self.inner.commit(expected, next)?;
        if (self.stage == "after-reserve" && self.writes == 1)
            || (self.stage == "release-after" && self.writes == 2)
        {
            std::process::exit(73);
        }
        Ok(())
    }
}

#[test]
fn durable_process_child() {
    let Some(root) = std::env::var_os("EFFECT_DURABLE_CHILD") else {
        return;
    };
    let root = PathBuf::from(root);
    let stage = ok(std::env::var("EFFECT_DURABLE_STAGE"));
    let store = CutStore {
        inner: ok(SqliteStateStore::open(root.join("reservations.sqlite"))),
        stage: stage.clone(),
        writes: 0,
    };
    let mut relay = ok(Relay::open(
        store,
        simulation::STORE,
        &policies(),
        manifest(),
    ));
    let mut wire = Wire {
        sends: root.join("sends"),
        stage,
        calls: 0,
    };
    ok(relay.handle(&mut wire, &binding(), &operation()));
    // No destructors: process loss after an answer, or after a completed release.
    std::process::exit(73);
}

#[test]
fn sqlite_real_process_loss() {
    for stage in [
        "before-reserve",
        "after-reserve",
        "before-forward",
        "after-forward",
        "after-answer",
        "release-before",
        "release-after",
        "release-answer",
    ] {
        let files = Files::new();
        files.create();
        let child = ok(Command::new(ok(std::env::current_exe()))
            .args(["--exact", "durable_process_child", "--nocapture"])
            .env("EFFECT_DURABLE_CHILD", &files.0)
            .env("EFFECT_DURABLE_STAGE", stage)
            .output());
        assert_eq!(
            child.status.code(),
            Some(73),
            "child {stage}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let released = matches!(stage, "before-reserve" | "release-after" | "release-answer");
        let mut wire = Wire {
            sends: files.sends(),
            stage: String::new(),
            calls: 0,
        };
        let handled = ok(open(&files.store()).handle(&mut wire, &binding(), &operation()));
        assert_eq!(
            matches!(
                handled,
                Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
            ),
            !released,
            "restart {stage}"
        );
        let copies = fs::read_to_string(files.sends())
            .unwrap_or_default()
            .lines()
            .count();
        assert!(copies <= 1, "second copy after real process loss {stage}");
        assert_eq!(
            copies,
            usize::from(released || matches!(stage, "after-forward" | "after-answer")),
            "non-vacuity {stage}"
        );
    }
}

#[test]
fn sqlite_missing_corrupt_binding_and_stale_handle() {
    let files = Files::new();
    assert!(SqliteStateStore::open(files.store()).is_err());
    assert!(!files.store().exists());
    files.create();
    assert!(SqliteStateStore::create(files.store(), &[]).is_err());
    let mut first = open(&files.store());
    let mut stale = open(&files.store());
    let mut wire = Wire {
        sends: files.sends(),
        stage: String::new(),
        calls: 0,
    };
    ok(first.handle(&mut wire, &binding(), &operation()));
    assert_eq!(
        stale.handle(&mut wire, &binding(), &operation()),
        Err(Error::Conflict)
    );
    assert_eq!(wire.calls, 1);
    let other = [Policy::AtMostOne { capacity: 3 }];
    assert!(matches!(
        Relay::open(
            ok(SqliteStateStore::open(files.store())),
            simulation::STORE,
            &other,
            manifest()
        ),
        Err(Error::Binding)
    ));
    let connection = ok(rusqlite::Connection::open(files.store()));
    ok(connection.execute("UPDATE effect_reservation SET bytes=x'00'", []));
    assert!(matches!(
        Relay::open(
            ok(SqliteStateStore::open(files.store())),
            simulation::STORE,
            &policies(),
            manifest()
        ),
        Err(Error::Corrupt)
    ));
    ok(connection.execute("DELETE FROM effect_reservation", []));
    assert!(SqliteStateStore::open(files.store()).is_err());
}

#[test]
fn sqlite_write_failure_sends_nothing() {
    for busy in [false, true] {
        let files = Files::new();
        files.create();
        let mut relay = open(&files.store());
        let connection = ok(rusqlite::Connection::open(files.store()));
        if busy {
            ok(connection.execute_batch("BEGIN IMMEDIATE"));
        } else {
            ok(connection.execute_batch("CREATE TRIGGER refuse_reservation BEFORE UPDATE ON effect_reservation BEGIN SELECT RAISE(ABORT, 'injected write error'); END;"));
        }
        let mut wire = Wire {
            sends: files.sends(),
            stage: String::new(),
            calls: 0,
        };
        assert_eq!(
            relay.handle(&mut wire, &binding(), &operation()),
            Err(Error::Storage)
        );
        assert_eq!(wire.calls, 0, "SQLite write failure reached transport");
        assert!(!files.sends().exists());
    }
}
