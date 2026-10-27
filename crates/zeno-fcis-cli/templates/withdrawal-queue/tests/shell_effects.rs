#![cfg(feature = "shell-effects")]

use std::{cell::Cell, path::PathBuf, rc::Rc};
use withdrawal_queue::{
    bindings::GeneratedProject,
    create, deposit,
    generated::{Caller, Day, Flag, TickContext},
    shell_effects::{Effects, MAX_REQUESTS, RETRIES},
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
    fn provenance(&self) -> Option<zeno_fcis_shell_sqlite::effects::production::AdapterProvenance> {
        Some(
            zeno_fcis_shell_sqlite::effects::production::AdapterProvenance {
                source: zeno_fcis_shell::effects::registry::source_digest(&[(
                    "adapter.rs",
                    include_bytes!("shell_effects.rs"),
                )]),
                configuration: {
                    let mut bytes = vec![u8::from(self.fail)];
                    bytes.extend_from_slice(&(self.calls.get() as u64).to_be_bytes());
                    bytes
                },
                seed: 0,
            },
        )
    }

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
    fn provenance(&self) -> Option<zeno_fcis_shell_sqlite::effects::production::AdapterProvenance> {
        Some(
            zeno_fcis_shell_sqlite::effects::production::AdapterProvenance {
                source: zeno_fcis_shell::effects::registry::source_digest(&[(
                    "adapter.rs",
                    include_bytes!("shell_effects.rs"),
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
                now: Day(0),
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
        let transcript = shell.transcript().unwrap();
        transcript.verify().unwrap();
        let wrapper = transcript
            .manifest
            .implementations
            .iter()
            .find(|entry| entry.name == "withdrawal-queue")
            .expect("template source missing");
        assert_eq!(
            wrapper.source,
            zeno_fcis_shell::effects::registry::source_digest(&[(
                "shell_effects.rs",
                include_bytes!("../src/shell_effects.rs")
            ),])
        );
        let mut limits = vec![RETRIES];
        limits.extend_from_slice(&(MAX_REQUESTS as u64).to_be_bytes());
        assert!(
            transcript.manifest.configuration.ends_with(&limits),
            "execution limits missing"
        );
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

struct HiddenSuccess {
    payments: Rc<std::cell::RefCell<Vec<Hash32>>>,
    panic_once: Rc<Cell<bool>>,
}
impl Transport for HiddenSuccess {
    fn provenance(&self) -> Option<zeno_fcis_shell_sqlite::effects::production::AdapterProvenance> {
        Some(
            zeno_fcis_shell_sqlite::effects::production::AdapterProvenance {
                source: zeno_fcis_shell::effects::registry::source_digest(&[(
                    "payment-transport.rs",
                    include_bytes!("shell_effects.rs"),
                )]),
                configuration: (self.payments.borrow().len() as u64).to_be_bytes().to_vec(),
                seed: 0,
            },
        )
    }
    fn send(&mut self, delivery: &zeno_fcis_shell::effects::port::Delivery) -> Sent {
        // This provider deliberately has no idempotency: each call pays once,
        // then loses its reply. A retry would pay the same invoice again.
        self.payments.borrow_mut().push(delivery.id);
        assert!(
            !self.panic_once.replace(false),
            "lost control after payment"
        );
        Sent::Unknown(Fault::Io)
    }
}

fn payment_policy_case(unwind: bool) {
    use zeno_fcis_shell::effects::{
        policy::{Policy, Verdict},
        record::Entry,
    };
    let temp = Temp::new();
    let path = temp.0.join("payment-policy.sqlite");
    withdrawal_queue::payouts(&path).unwrap();
    let before = withdrawal_queue::relay_export(&path).unwrap();
    v2_contract::with_lineage(|catalogs| {
        let lineage = Lineage::bind(catalogs, &[]).unwrap();
        let payments = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut shell = Effects::open_with_policies(&path, &lineage,
            HiddenSuccess { payments: payments.clone(), panic_once: Rc::new(Cell::new(unwind)) },
            Ticks::default(), vec![Policy::AtMostOne { capacity: 8 }],
        ).unwrap();
        shell.record_private(false); // A monitored lifetime still requires evidence.
        if unwind {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shell.deliver(1))).is_err());
        } else {
            assert_eq!(shell.deliver(1).unwrap(), Outcome::Unknown(Fault::Io));
        }
        assert_eq!(payments.borrow().len(), 1, "double payment after hidden success");
        assert!(matches!(shell.deliver(2).unwrap(), Outcome::Refused(
            zeno_fcis_shell::effects::port::Code::PolicyDuplicate)));
        assert_eq!(payments.borrow().len(), 1, "a fresh run forgot the invoice reservation");
        assert_eq!(withdrawal_queue::relay_export(&path).unwrap(), before,
            "policy refusal silently removed an authorized invoice");
        let transcript = shell.transcript().expect("policy verdict transcript missing");
        transcript.verify().unwrap();
        assert!(transcript.links.iter().any(|link| matches!(&link.entry,
            Entry::Policy(_, check) if check.decisions.iter().any(|d|
                d.verdict == Verdict::Reject(zeno_fcis_shell::effects::port::Code::PolicyDuplicate)))));
        let first = zeno_fcis_shell::effects::program::DeliveryProgram::start(
            zeno_fcis_shell::effects::program::DeliveryInvocation { epoch: 2, store: shell.store() },
            zeno_fcis_shell::effects::program::DeliveryPolicy {
                retries: RETRIES, backoff: 10, jitter: std::num::NonZeroU64::MIN,
            },
        );
        zeno_fcis_shell::effects::record::replay::<zeno_fcis_shell::effects::program::DeliveryProgram>(
            transcript, &transcript.manifest.start, 2, first,
            |outcome| zeno_fcis_shell::effects::codec::outcome(outcome,
                match outcome { Outcome::Ok(n) => Some(*n), _ => None }),
        ).unwrap();
    }).unwrap();
}

#[test]
fn payment_policy_blocks_hidden_success_retry() {
    payment_policy_case(false);
}

#[test]
fn payment_policy_reservation_survives_terminal_unwind() {
    payment_policy_case(true);
}
