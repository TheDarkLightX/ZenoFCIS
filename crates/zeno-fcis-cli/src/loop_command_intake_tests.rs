//! Actual candidate entry-point billing and durable refusal boundaries.
#[cfg(target_os = "linux")]
use super::CANDIDATE;
use super::{Directory, ORIGINAL};
use crate::loop_command::{Store, candidate_command, resume_command};
use crate::neural_loop::canonical_json;
#[cfg(target_os = "linux")]
use crate::neural_loop::ledger::{Kind, Ledger, Stage};
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
#[cfg(target_os = "linux")]
use crate::neural_loop::program_json::program_to_json;
use crate::neural_loop::request::{Policy, Request};
use crate::neural_loop::session::Session;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use zeno_fcis_synthesis::finite_runtime::import_program;

fn must<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
}

fn setup(directory: &Directory, precharged: u64) -> PathBuf {
    let path = directory.0.join("session");
    let request = must(Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    ));
    assert_eq!(request.json()["work_policy"], "zeno-fcis/checker-work/4");
    let mut session = Session::open(request);
    if precharged != 0 {
        must(session.reserve_preparation(precharged));
    }
    let mut store = must(Store::create(&path, session.request()));
    must(store.persist(session.ledger()));
    path
}

#[cfg(target_os = "linux")]
fn ledger(path: &Path) -> Ledger {
    let bytes = must(fs::read(path.join("ledger.jsonl")));
    must(Ledger::from_entries(must(Ledger::parse_lines(&bytes))))
}

#[cfg(target_os = "linux")]
fn preparations(ledger: &Ledger) -> Vec<u64> {
    ledger
        .entries()
        .iter()
        .filter_map(|entry| match entry.kind {
            Kind::Reserved {
                stage: Stage::Preparation { work },
                ..
            } => Some(work),
            _ => None,
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn input(directory: &Directory, json_input: bool) -> PathBuf {
    if json_input {
        let program = must(import_program(CANDIDATE));
        directory.write(
            "candidate.json",
            &canonical_json(&program_to_json(&program)),
        )
    } else {
        directory.write("candidate.zcve", CANDIDATE)
    }
}

fn supply(session: &Path, path: &Path, json_input: bool, provenance: Option<&Path>) -> (u8, Value) {
    if json_input {
        candidate_command(session, None, Some(path), provenance)
    } else {
        candidate_command(session, Some(path), None, provenance)
    }
}

#[cfg(target_os = "linux")]
fn asserted(session: &Path) -> Value {
    let text = must(fs::read_to_string(session.join("transcript.jsonl")));
    text.lines()
        .map(|line| must(serde_json::from_str::<Value>(line)))
        .find_map(|row| row.get("provenance").cloned())
        .unwrap_or_else(|| panic!("actual proposal provenance record"))["provider_asserted"]
        .clone()
}

#[cfg(target_os = "linux")]
#[test]
fn four_intake_routes_charge_actual_ledger_and_preserve_serial_receipt() {
    let request = must(Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    ));
    let serial = must(crate::transform::check(
        ORIGINAL,
        CANDIDATE,
        request.transform_limits(),
    ));
    // Fixed expected policy values, checked against actual durable reservations.
    for (json_input, present, intake) in [
        (false, false, 69_632),
        (false, true, 593_920),
        (true, false, 2_101_248),
        (true, true, 2_625_536),
    ] {
        let directory = Directory::new();
        let session = setup(&directory, 0);
        let path = input(&directory, json_input);
        let provenance = directory.write("provenance.json", br#"{"passed":true}"#);
        let (exit, report) = supply(
            &session,
            &path,
            json_input,
            present.then_some(provenance.as_path()),
        );
        assert_eq!(exit, crate::OK, "{report}");
        assert_eq!(report["status"], "checked-improvement");
        let stored = ledger(&session);
        assert_eq!(preparations(&stored), vec![intake, 635_904]);
        let totals = stored.accounting();
        assert_eq!(totals.work_reserved, intake + 635_904 + 282_871);
        assert_eq!((totals.attempts, totals.checks, totals.replays), (1, 1, 0));
        assert_eq!(
            report["detail"]["accounting"]["work_reserved"],
            totals.work_reserved
        );
        assert_eq!(
            must(fs::read(session.join("receipts/0.json"))),
            serial.receipt()
        );
        assert_eq!(
            asserted(&session),
            if present {
                json!({"passed":true})
            } else {
                Value::Null
            }
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn missing_then_malformed_provenance_keeps_each_actual_reservation() {
    for (json_input, intake) in [(false, 593_920), (true, 2_625_536)] {
        let directory = Directory::new();
        let session = setup(&directory, 0);
        let path = input(&directory, json_input);
        let provenance = directory.0.join("provenance.json");
        let (exit, report) = supply(&session, &path, json_input, Some(&provenance));
        assert_eq!(exit, crate::FAILURE, "{report}");
        assert_eq!(report["status"], "io-error");
        let before = ledger(&session);
        assert_eq!(preparations(&before), vec![intake]);
        assert_eq!(before.accounting().work_reserved, intake);
        assert_eq!(before.accounting().attempts, 0);
        assert!(!session.join("transcript.jsonl").exists());
        assert_eq!(must(fs::read_dir(session.join("artifacts"))).count(), 0);
        assert_eq!(must(fs::read_dir(session.join("receipts"))).count(), 0);
        must(fs::write(&provenance, b"not JSON"));
        let (exit, report) = supply(&session, &path, json_input, Some(&provenance));
        assert_eq!(exit, crate::OK, "{report}");
        assert_eq!(report["status"], "checked-improvement");
        let after = ledger(&session);
        assert_eq!(&after.entries()[..before.entries().len()], before.entries());
        assert_eq!(preparations(&after), vec![intake, intake, 635_904]);
        assert_eq!(
            after.accounting().work_reserved,
            2 * intake + 635_904 + 282_871
        );
        assert_eq!(asserted(&session), Value::Null);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn intake_exact_and_one_short_precede_reads_and_never_refund() {
    for (json_input, present, intake) in [
        (false, false, 69_632),
        (false, true, 593_920),
        (true, false, 2_101_248),
        (true, true, 2_625_536),
    ] {
        for one_short in [false, true] {
            let directory = Directory::new();
            let prior_work = Limits::CEILING.session_work - intake + u64::from(one_short);
            let session = setup(&directory, prior_work);
            let before = ledger(&session);
            // Both paths are deliberately absent: intake refusal must win over I/O.
            let candidate = directory.0.join("missing-candidate");
            let provenance = directory.0.join("missing-provenance");
            let (exit, report) = supply(
                &session,
                &candidate,
                json_input,
                present.then_some(provenance.as_path()),
            );
            let after = ledger(&session);
            assert_eq!(&after.entries()[..before.entries().len()], before.entries());
            assert_eq!(after.entries().len(), before.entries().len() + 1);
            assert_eq!(after.accounting().attempts, 0);
            if one_short {
                assert_eq!(exit, crate::BLOCKED, "{report}");
                assert_eq!(report["status"], "inconclusive");
                assert_eq!(
                    report["detail"]["reason"],
                    "unavailable:preparation-work-exhausted"
                );
                assert_eq!(preparations(&after), vec![prior_work]);
                assert_eq!(after.accounting().work_reserved, prior_work);
                assert!(matches!(&after.entries()[before.entries().len()].kind,
                    Kind::Closed { reason } if reason == "unavailable:preparation-work-exhausted"));
            } else {
                assert_eq!(exit, crate::FAILURE, "{report}");
                assert_eq!(report["status"], "io-error");
                assert_eq!(preparations(&after), vec![prior_work, intake]);
                assert_eq!(
                    after.accounting().work_reserved,
                    Limits::CEILING.session_work
                );
            }
            assert!(!session.join("transcript.jsonl").exists());
            assert_eq!(must(fs::read_dir(session.join("artifacts"))).count(), 0);
            assert_eq!(must(fs::read_dir(session.join("receipts"))).count(), 0);
        }
    }
}

#[test]
fn old_work_policy_refuses_before_candidate_reads_and_writes() {
    for old in ["zeno-fcis/checker-work/2", "zeno-fcis/checker-work/3"] {
        let directory = Directory::new();
        let session = setup(&directory, 0);
        let request_path = session.join("request.json");
        let mut request: Value = must(serde_json::from_slice(&must(fs::read(&request_path))));
        request["work_policy"] = json!(old);
        must(fs::write(&request_path, canonical_json(&request)));
        let names = ["request.json", "ledger.jsonl", "ledger.head"];
        let before: Vec<_> = names
            .iter()
            .map(|name| must(fs::read(session.join(name))))
            .collect();
        for (exit, report) in [
            resume_command(&session),
            supply(&session, &directory.0.join("absent"), false, None),
        ] {
            assert_eq!(exit, crate::BLOCKED, "{report}");
            assert_eq!(report["status"], "resume-refused");
            assert_eq!(report["detail"]["refusal"]["reason"], "work-policy-changed");
            assert_eq!(report["detail"]["trusted_incumbent"], Value::Null);
            let after: Vec<_> = names
                .iter()
                .map(|name| must(fs::read(session.join(name))))
                .collect();
            assert_eq!(
                after, before,
                "stale billing policy cannot append or adopt current charges"
            );
        }
        assert!(!session.join("transcript.jsonl").exists());
    }
}
