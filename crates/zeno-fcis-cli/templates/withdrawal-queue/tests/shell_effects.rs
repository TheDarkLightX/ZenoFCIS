#![cfg(feature = "shell-effects")]

use std::{cell::Cell, path::PathBuf, rc::Rc};
use withdrawal_queue::{
    bindings::GeneratedProject,
    create, deposit,
    generated::{Caller, Flag, TickContext},
    shell_effects::{Effects, RETRIES},
    v2_contract,
};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::{
    CommitStatus,
    effects::{
        port::{CommitMode, EpochTick, Fault, Outcome, PublicationData, Report, Sent, WaitError},
        program::CommitInvocation,
    },
};
use zeno_fcis_shell_sqlite::{
    effects::{
        self,
        production::{Time, Transport},
    },
    v2::Lineage,
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition::Raw,
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "withdrawal-effects-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Wire {
    calls: Rc<Cell<usize>>,
    fail: bool,
}
impl Transport for Wire {
    fn send(&mut self, d: &zeno_fcis_shell::effects::port::Delivery) -> Sent {
        self.calls.set(self.calls.get() + 1);
        assert!(!self.fail || self.calls.get() <= 3, "retry budget exceeded");
        assert_eq!(
            zeno_fcis_shell_sqlite::v2::relay::payload_sha256(&d.payload),
            d.payload_sha256
        );
        if self.fail {
            Sent::Unknown(Fault::Io)
        } else {
            Sent::Sent(Report {
                delivery_id: d.id,
                payload_sha256: d.payload_sha256,
            })
        }
    }
}
#[derive(Default)]
struct Ticks(u64);
impl Time for Ticks {
    fn clock(&mut self, epoch: u64) -> Result<EpochTick, Fault> {
        Ok(EpochTick {
            epoch,
            tick: self.0,
        })
    }
    fn random(&mut self, _: u64) -> Result<u64, Fault> {
        Ok(0)
    }
    fn wait(&mut self, deadline: EpochTick) -> Result<EpochTick, WaitError> {
        self.0 = deadline.tick;
        Ok(deadline)
    }
}

fn data(a: &Authority<'_>, state: &[u8]) -> PublicationData {
    let p = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let c = p
        .admit_command::<RustCryptoSha256>(&deposit(2), ValidationLimits::default())
        .unwrap();
    let x = p
        .admit_context::<RustCryptoSha256>(
            &TickContext {
                caller: Caller::Operator,
                alarm: Flag(false),
            },
            ValidationLimits::default(),
        )
        .unwrap();
    let c = c.admitted().envelope().canonical_bytes().unwrap();
    let x = x.admitted().envelope().canonical_bytes().unwrap();
    match a.publish(Raw {
        state,
        command: &c,
        context: &x,
    }) {
        PublicationOutcome::Commit(p) => effects::publication_data(&p),
        other => panic!("publication: {other:?}"),
    }
}

#[test]
fn recovery_uses_original_key() {
    let temp = Temp::new();
    let path = temp.0.join("commit.sqlite");
    v2_contract::with_lineage(|catalogs| {
        let lineage = Lineage::bind(catalogs, &[]).unwrap();
        let a = &lineage.authorities()[0];
        let mut direct = create(&path, a).unwrap();
        let data = data(a, direct.snapshot().unwrap().state());
        let key = Hash32::new([19; 32]);
        let original = direct
            .commit_at(0, key, effects::bind_publication(a, &data).unwrap())
            .unwrap();
        drop(direct); // The caller has lost the reply, but retained its inputs.
        let mut shell = Effects::open(
            &path,
            &lineage,
            Wire {
                calls: Rc::default(),
                fail: false,
            },
            Ticks::default(),
        )
        .unwrap();
        let invocation = || CommitInvocation {
            epoch: 1,
            store: shell.store(),
            key,
            publication: data.clone(),
            mode: CommitMode::At(0),
        };
        let request = invocation();
        assert!(shell.transcript().is_none());
        shell.record_private(true);
        let Outcome::Ok(receipt) = shell.recover(&request).unwrap() else {
            panic!("original-key recovery failed")
        };
        assert_eq!(
            (receipt.version, receipt.chain, receipt.status),
            (
                original.version(),
                original.chain(),
                CommitStatus::IdempotentReplay
            ),
            "original-key receipt changed"
        );
        shell.transcript().unwrap().verify().unwrap();
        let mut direct = withdrawal_queue::Shell::open(&path, a).unwrap();
        assert_eq!(direct.snapshot().unwrap().version(), 1);
        // History absence keeps the original stale guard: it cannot become a fresh decision.
        let request = CommitInvocation {
            epoch: 2,
            store: shell.store(),
            key: Hash32::new([20; 32]),
            publication: data,
            mode: CommitMode::At(0),
        };
        assert!(matches!(
            shell.recover(&request).unwrap(),
            Outcome::Refused(zeno_fcis_shell::effects::port::Code::Concurrent)
        ));
    })
    .unwrap();
}

#[test]
fn commit_and_absent_key_recovery() {
    let temp = Temp::new();
    for recover in [false, true] {
        let path = temp.0.join(format!("{recover}.sqlite"));
        v2_contract::with_lineage(|catalogs| {
            let lineage = Lineage::bind(catalogs, &[]).unwrap();
            let a = &lineage.authorities()[0];
            let mut direct = create(&path, a).unwrap();
            let data = data(a, direct.snapshot().unwrap().state());
            drop(direct);
            let mut shell = Effects::open(
                &path,
                &lineage,
                Wire {
                    calls: Rc::default(),
                    fail: false,
                },
                Ticks::default(),
            )
            .unwrap();
            let request = CommitInvocation {
                epoch: 1,
                store: shell.store(),
                key: Hash32::new([1; 32]),
                publication: data,
                mode: CommitMode::At(0),
            };
            let outcome = if recover {
                shell.recover(&request)
            } else {
                shell.commit(&request)
            }
            .unwrap();
            assert!(
                matches!(outcome, Outcome::Ok(receipt) if receipt.status == CommitStatus::Committed)
            );
            assert!(
                shell.transcript().is_none(),
                "recording must be off by default"
            );
        })
        .unwrap();
    }
}

#[test]
fn retries_are_bounded() {
    let temp = Temp::new();
    let path = temp.0.join("payouts.sqlite");
    withdrawal_queue::payouts(&path).unwrap();
    let before = withdrawal_queue::relay_export(&path).unwrap();
    v2_contract::with_lineage(|catalogs| {
        let lineage = Lineage::bind(catalogs, &[]).unwrap();
        let calls = Rc::new(Cell::new(0));
        let mut shell = Effects::open(
            &path,
            &lineage,
            Wire {
                calls: calls.clone(),
                fail: true,
            },
            Ticks::default(),
        )
        .unwrap();
        shell.record_private(true);
        assert_eq!(shell.deliver(1).unwrap(), Outcome::Unknown(Fault::Io));
        assert_eq!(
            calls.get(),
            usize::from(RETRIES) + 1,
            "retry budget exceeded"
        );
        assert_eq!(withdrawal_queue::relay_export(&path).unwrap(), before);
        shell.transcript().unwrap().verify().unwrap();
    })
    .unwrap();
}

#[test]
fn private_recording_stays_private() {
    if std::env::var_os("WITHDRAWAL_PRIVATE_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "private_recording_stays_private", "--nocapture"])
            .env("WITHDRAWAL_PRIVATE_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("Transcript {"),
            "private transcript reached stdout"
        );
        return;
    }
    let temp = Temp::new();
    let path = temp.0.join("private.sqlite");
    withdrawal_queue::payouts(&path).unwrap();
    let before = withdrawal_queue::relay_export(&path).unwrap();
    v2_contract::with_lineage(|catalogs| {
        let lineage = Lineage::bind(catalogs, &[]).unwrap();
        let calls = Rc::new(Cell::new(0));
        let mut shell = Effects::open(
            &path,
            &lineage,
            Wire { calls, fail: false },
            Ticks::default(),
        )
        .unwrap();
        assert!(shell.transcript().is_none());
        shell.record_private(true);
        assert_eq!(shell.deliver(1).unwrap(), Outcome::Ok(2));
        let transcript = shell.transcript().unwrap();
        transcript.verify().unwrap();
        assert!(!transcript.links.is_empty());
        use std::num::NonZeroU64;
        use zeno_fcis_shell::effects::{
            codec,
            program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram},
            record,
        };
        fn done(outcome: &Outcome<u64>) -> Vec<u8> {
            codec::outcome(
                outcome,
                match outcome {
                    Outcome::Ok(n) => Some(*n),
                    _ => None,
                },
            )
        }
        assert_eq!(
            record::replay::<DeliveryProgram>(
                transcript,
                &transcript.manifest.start,
                1,
                DeliveryProgram::start(
                    DeliveryInvocation {
                        epoch: 1,
                        store: shell.store()
                    },
                    DeliveryPolicy {
                        retries: RETRIES,
                        backoff: 10,
                        jitter: NonZeroU64::MIN
                    }
                ),
                done,
            )
            .unwrap(),
            record::Ended::Done(done(&Outcome::Ok(2)))
        );
        assert_eq!(withdrawal_queue::relay_export(&path).unwrap(), "");
        assert_eq!(
            std::fs::read_dir(&temp.0).unwrap().count(),
            1,
            "private capture wrote an artifact"
        );
        assert!(!before.contains("Transcript"));
        shell.record_private(false);
        assert!(shell.transcript().is_none());
    })
    .unwrap();
}

#[test]
fn default_bytes_unchanged() {
    let temp = Temp::new();
    let expected = [
        "Accept", "Accept", "Reject", "Reject", "Accept", "Reject", "Accept", "Accept", "Accept",
        "Accept", "Accept", "Accept", "Accept", "Accept", "Accept", "Reject",
    ];
    let expected = format!(
        "{{\"status\":\"decided\",\"decisions\":[{}],\"bundles\":12,\"pending\":2}}",
        expected.map(|s| format!("\"{s}\"")).join(",")
    );
    assert_eq!(
        withdrawal_queue::payouts(&temp.0.join("default.sqlite")).unwrap(),
        expected,
        "default byte vector changed"
    );
}
