//! Checks that the executed application decided a corpus of reachable inputs
//! the same way in every run this test makes.
//!
//! Each input is decided eight times in this process through
//! `execute_probed`, which withholds a decision whose runs disagree. The whole
//! corpus is then decided again in child processes started with a changed
//! environment: a cleared environment, another time zone and locale, a glibc
//! allocator that fills memory with a pattern, and another working directory.
//! Each child is a fresh process, so it also gets new `HashMap` seeds and,
//! where address-space layout randomization is enabled, new addresses. Every
//! decision digest must match.
//!
//! Agreement is evidence about these runs only; it does not prove the
//! application deterministic. `zeno-fcis purity src/program.rs src/laws.rs`
//! checks the decision code statically for the sources of nondeterminism that
//! these runs might not exercise.

use order_fulfillment::{
    Authority, authority, bindings::GeneratedProject, command, generated::*, profile,
};
use std::process::Command;
use zeno_fcis_authority::ProbeRuns;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;

const DIGESTS: &str = "decision-digests: ";

/// One changed environment for a child process.
struct Environment {
    label: &'static str,
    clear: bool,
    variables: &'static [(&'static str, &'static str)],
    other_directory: bool,
}

const ENVIRONMENTS: [Environment; 3] = [
    Environment {
        label: "cleared environment",
        clear: true,
        variables: &[("TZ", "Pacific/Kiritimati"), ("LC_ALL", "C")],
        other_directory: false,
    },
    Environment {
        label: "other time zone, locale, and allocator fill",
        clear: false,
        variables: &[
            ("TZ", "America/Adak"),
            ("LANG", "tr_TR.UTF-8"),
            ("LC_ALL", "tr_TR.UTF-8"),
            ("MALLOC_PERTURB_", "165"),
            ("ZENO_FCIS_PROBE_NOISE", "1"),
        ],
        other_directory: false,
    },
    Environment {
        label: "other working directory",
        clear: false,
        variables: &[("TZ", "Asia/Kathmandu")],
        other_directory: true,
    },
];

/// Decides every corpus input `runs` times and returns one digest per input,
/// in a fixed order.
///
/// The corpus is every reachable state with every action and caller, the
/// callback naming the current attempt, plus each payment callback naming a
/// stale attempt.
fn corpus_digests(authority: &Authority, runs: u8) -> Vec<String> {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let runs = ProbeRuns::try_new(runs).unwrap();
    let statuses = [
        OrderStatus::Placed,
        OrderStatus::AwaitingPayment,
        OrderStatus::Paid,
        OrderStatus::Shipped,
        OrderStatus::Delivered,
        OrderStatus::Cancelled,
    ];
    let actions = [
        OrderAction::Checkout,
        OrderAction::PaymentCaptured,
        OrderAction::PaymentDeclined,
        OrderAction::ParcelDispatched,
        OrderAction::ParcelDelivered,
        OrderAction::CancelOrder,
    ];
    let callers = [Caller::Customer, Caller::PaymentProvider, Caller::Carrier];
    let mut corpus = Vec::new();
    for status in &statuses {
        for attempts in 0..=3 {
            // The reachable states: past checkout, an order has an attempt.
            let started = !matches!(status, OrderStatus::Placed | OrderStatus::Cancelled);
            if started && attempts == 0 {
                continue;
            }
            for action in &actions {
                for caller in &callers {
                    corpus.push((
                        status.clone(),
                        attempts,
                        action.clone(),
                        attempts,
                        caller.clone(),
                    ));
                }
            }
            for action in [OrderAction::PaymentCaptured, OrderAction::PaymentDeclined] {
                let stale = (attempts + 1) % 4;
                corpus.push((
                    status.clone(),
                    attempts,
                    action,
                    stale,
                    Caller::PaymentProvider,
                ));
            }
        }
    }
    let mut digests = Vec::new();
    for (status, attempts, action, callback_attempt, caller) in corpus {
        let input = format!("{status:?} {attempts} {action:?} {callback_attempt} {caller:?}");
        let root = project
            .admit_root::<RustCryptoSha256>(
                &Order {
                    status,
                    payment_attempts: Attempts(attempts),
                },
                limits,
            )
            .unwrap();
        let admitted_command = project
            .admit_command::<RustCryptoSha256>(&command(action, callback_attempt), limits)
            .unwrap();
        let admitted_context = project
            .admit_context::<RustCryptoSha256>(&CallerContext { caller }, limits)
            .unwrap();
        let witness = authority
            .admit_invocation(
                root,
                admitted_command.admitted().clone(),
                admitted_context.admitted().clone(),
                profile::digest("example/order-fulfillment/principal", b"determinism"),
                profile::digest("example/order-fulfillment/authentication", b"determinism"),
                profile::digest("example/order-fulfillment/replay", input.as_bytes()),
            )
            .unwrap();
        let (_, probe) = authority
            .execute_probed(witness, runs)
            .unwrap_or_else(|error| panic!("input {input}: {error:?}"));
        assert_eq!(probe.runs(), runs.get());
        digests.push(probe.decision_digest().to_string());
    }
    digests
}

#[test]
fn every_corpus_input_decides_identically_across_repeated_runs() {
    let authority = authority().unwrap();
    let digests = corpus_digests(&authority, 8);
    assert_eq!(digests.len(), 400);
    // Each input has its own replay identity, so every decision is distinct.
    let distinct: std::collections::BTreeSet<_> = digests.iter().collect();
    assert_eq!(distinct.len(), 400);
    println!("determinism: 400 inputs x 8 runs in this process, 0 divergences");
}

#[test]
fn decisions_match_in_child_processes_with_a_changed_environment() {
    let authority = authority().unwrap();
    let expected = corpus_digests(&authority, 2);
    let executable = std::env::current_exe().unwrap();
    for environment in &ENVIRONMENTS {
        let label = environment.label;
        let mut child = Command::new(&executable);
        child.args([
            "--exact",
            "corpus_digest_helper",
            "--ignored",
            "--nocapture",
            "--test-threads",
            "1",
        ]);
        if environment.clear {
            child.env_clear();
        }
        child.envs(environment.variables.iter().copied());
        if environment.other_directory {
            child.current_dir(std::env::temp_dir());
        }
        let output = child.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{label}: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Libtest prints the test's name on the same line as its output.
        let line = stdout
            .lines()
            .find_map(|line| line.split_once(DIGESTS).map(|(_, digests)| digests.trim()))
            .unwrap_or_else(|| panic!("{label}: no digests in {stdout}"));
        let digests: Vec<String> = line.split(',').map(str::to_string).collect();
        assert_eq!(digests, expected, "{label}: a decision differed");
    }
    println!("determinism: 400 inputs in 3 changed-environment processes, 0 divergences");
}

#[test]
#[ignore = "spawned by decisions_match_in_child_processes_with_a_changed_environment"]
fn corpus_digest_helper() {
    let authority = authority().unwrap();
    println!("{DIGESTS}{}", corpus_digests(&authority, 2).join(","));
}
