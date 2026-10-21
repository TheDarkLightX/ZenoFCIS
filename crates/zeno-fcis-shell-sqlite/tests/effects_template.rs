//! P1's unchanged 256 x 64 workload, with the opt-in template adapter also
//! exercised once per seed in the on lane. Each lane replays P1 twice.
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/withdrawal-queue/src/shell_effects.rs"]
mod app;
#[allow(dead_code, unused_imports)]
mod sim;

use sim::{database, fixture, model, ok, schedule::Schedule};
use zeno_fcis_shell::effects::{
    port::{self, CommitMode, EpochTick, Fault, Outcome, Report, Sent, WaitError},
    program::CommitInvocation,
};
use zeno_fcis_shell_sqlite::{
    effects::{
        self,
        production::{Time, Transport},
    },
    v2::Lineage,
};

struct Wire(database::Receiver, bool);
impl Transport for Wire {
    fn provenance(&self) -> Option<zeno_fcis_shell_sqlite::effects::production::AdapterProvenance> {
        Some(
            zeno_fcis_shell_sqlite::effects::production::AdapterProvenance {
                source: zeno_fcis_shell::effects::registry::source_digest(&[
                    ("adapter.rs", include_bytes!("effects_template.rs")),
                    ("receiver.rs", include_bytes!("sim/database.rs")),
                ]),
                configuration: format!("{:?}|{}", self.0.state(), self.1).into_bytes(),
                seed: 0,
            },
        )
    }

    fn send(&mut self, d: &port::Delivery) -> Sent {
        if self.1 {
            self.1 = false;
            return Sent::Unknown(Fault::Io);
        }
        self.0.receive(
            &model::Delivery {
                sequence: d.version,
                ordinal: d.ordinal,
                id: d.id,
                payload: d.payload.clone(),
                row: vec![],
                acknowledged: false,
            },
            false,
        );
        Sent::Sent(Report {
            delivery_id: d.id,
            payload_sha256: d.payload_sha256,
        })
    }
}
#[derive(Default)]
struct Ticks(u64);
impl Time for Ticks {
    fn provenance(&self) -> Option<zeno_fcis_shell_sqlite::effects::production::AdapterProvenance> {
        Some(
            zeno_fcis_shell_sqlite::effects::production::AdapterProvenance {
                source: zeno_fcis_shell::effects::registry::source_digest(&[(
                    "adapter.rs",
                    include_bytes!("effects_template.rs"),
                )]),
                configuration: self.0.to_be_bytes().to_vec(),
                seed: 0,
            },
        )
    }

    fn clock(&mut self, epoch: u64) -> Result<EpochTick, Fault> {
        Ok(EpochTick {
            epoch,
            tick: self.0,
        })
    }
    fn random(&mut self, _: u64) -> Result<u64, Fault> {
        Ok(0)
    }
    fn wait(&mut self, d: EpochTick) -> Result<EpochTick, WaitError> {
        self.0 = d.tick;
        Ok(d)
    }
}

fn mounted_roundtrip(lineage: &Lineage<'_, '_>, seed: u64) {
    let files = database::Files::memory();
    let a = &lineage.authorities()[0];
    drop(sim::create(&files, a));
    let input = fixture::input(a, sim::key(seed + 1), [0, 0], 120 + (seed % 2) as u16);
    let mut expected = model::Model::new(fixture::model_genesis(a));
    let mut shell = ok(app::Effects::open(
        &files.store,
        lineage,
        Wire(database::Receiver::open(&files.receiver), seed.is_multiple_of(2)),
        Ticks::default(),
    ));
    shell.record_private(seed.is_multiple_of(2));
    let request = |epoch| CommitInvocation {
        epoch,
        store: shell.store(),
        key: input.key,
        publication: ok(effects::audited_publication(
            a,
            &model::state(fixture::schema_hash(), input.before),
            &input.command,
            &input.context,
        )),
        mode: CommitMode::At(0),
    };
    let first = request(1);
    let retained = request(2);
    let Outcome::Ok(receipt) = ok(shell.commit(&first)) else {
        panic!("template commit")
    };
    assert_eq!(
        expected.commit(input.clone(), 0, None),
        model::Outcome::Committed(receipt.version, receipt.chain)
    );
    // Simulate a caller that discarded the reply and restarts the same request.
    let Outcome::Ok(replay) = ok(shell.recover(&retained)) else {
        panic!("template recovery")
    };
    assert_eq!(
        expected.commit(input, 0, None),
        model::Outcome::Replay(replay.version, replay.chain)
    );
    assert_eq!(ok(shell.deliver(3)), Outcome::Ok(2));
    for d in expected.deliveries.clone() {
        expected.receiver.insert(d.id, d.payload.clone());
        expected.acknowledge(d.id, sim::payload_hash(&d.payload));
    }
    ok(database::compare(
        &expected.database(),
        &database::snapshot(&files.store),
    ));
    assert_eq!(
        database::Receiver::open(&files.receiver).state(),
        expected.receiver
    );
    if let Some(t) = shell.transcript() {
        ok(t.verify());
    }
}

fn corpus(on: bool) {
    let start = std::time::Instant::now();
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            ok(std::process::Command::new(ok(std::env::current_exe()))
                .args(["--exact", "process_template_worker", "--nocapture"])
                .env("SHELL_TEMPLATE_WORKER", worker.to_string())
                .env("SHELL_TEMPLATE_ON", if on { "1" } else { "0" })
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn())
        })
        .collect();
    for worker in workers {
        let output = ok(worker.wait_with_output());
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let elapsed = start.elapsed();
    eprintln!("P1 256 x 64 x 2; template opt-in={on}: {elapsed:?}");
    assert!(
        elapsed.as_secs_f64() < 90.0,
        "corpus exceeded 90 s: {elapsed:?}"
    );
}

#[test]
fn corpus_opt_in_off() {
    corpus(false);
}
#[test]
fn corpus_opt_in_on() {
    corpus(true);
}
#[test]
fn process_template_worker() {
    let Ok(worker) = std::env::var("SHELL_TEMPLATE_WORKER") else {
        return;
    };
    let worker: u64 = ok(worker.parse());
    assert!(worker < 4);
    let on = std::env::var("SHELL_TEMPLATE_ON").as_deref() == Ok("1");
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let a = &lineage.authorities()[0];
        let cases = fixture::cases(a);
        let mut actions = [0_u64; 13];
        let mut crash_mask = 0_u16;
        for seed in (worker..256).step_by(4) {
            let schedule = Schedule::seeded(seed, 64);
            let first = sim::replay_with_authority(a, &cases, &schedule);
            for event in &first.events {
                actions[event.action as usize] += 1;
                if let model::Outcome::Refused(model::Refusal::Crash(point)) = event.decision {
                    crash_mask |= 1 << point;
                }
            }
            assert_eq!(
                first,
                sim::replay_with_authority(a, &cases, &schedule),
                "seed {seed}"
            );
            if on {
                mounted_roundtrip(&lineage, seed);
            }
        }
        assert!(actions.iter().all(|n| *n > 0), "action coverage");
        assert_eq!(crash_mask, 0xfe, "crash coverage");
    });
}

#[test]
fn production_manifest_binds_adapters_and_configuration() {
    use zeno_fcis_shell_sqlite::effects::production::{self, Production};
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let files = database::Files::memory();
        drop(sim::create(&files, &lineage.authorities()[0]));
        let one = ok(Production::open(
            &files.store,
            &lineage,
            1,
            Wire(database::Receiver::open(&files.receiver), false),
            Some(Ticks(0)),
        ));
        let first = one
            .manifest(vec![])
            .unwrap_or_else(|| panic!("missing qualified provenance"));
        assert!(first.provenance_complete());
        let production = first
            .implementations
            .iter()
            .find(|entry| entry.name == "production")
            .unwrap_or_else(|| panic!("missing production source"));
        assert_eq!(production.source, production::implementation_source());
        assert!(
            first
                .implementations
                .iter()
                .any(|entry| entry.name == "time")
        );
        assert!(
            first
                .implementations
                .iter()
                .any(|entry| entry.name == "transport")
        );
        for (fail, tick) in [(true, 0), (false, 1)] {
            let other = ok(Production::open(
                &files.store,
                &lineage,
                1,
                Wire(database::Receiver::open(&files.receiver), fail),
                Some(Ticks(tick)),
            ));
            let changed = other
                .manifest(vec![])
                .unwrap_or_else(|| panic!("missing provenance"));
            assert_ne!(first.digest(), changed.digest(), "adapter state omitted");
        }
        struct Unqualified;
        impl Transport for Unqualified {
            fn send(&mut self, _: &port::Delivery) -> Sent {
                Sent::Unknown(Fault::Io)
            }
        }
        let unqualified = ok(Production::open(
            &files.store,
            &lineage,
            1,
            Unqualified,
            Some(Ticks(0)),
        ));
        assert!(
            unqualified.manifest(vec![]).is_none(),
            "unqualified adapter recorded"
        );
    });
}
