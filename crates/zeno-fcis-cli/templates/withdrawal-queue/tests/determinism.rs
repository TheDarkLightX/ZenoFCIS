//! Checks that the executed application decided a corpus of vault commands
//! the same way in every run this test makes.
//!
//! The corpus takes five vaults (empty; both lanes arrived; both pending and
//! paused; must-serve with one lane arrived; one lane pending late in a
//! pause) with six commands (two deposits, two requests, a tick with and
//! without the alarm) from every caller: 120 inputs. `tests/conformance.rs`
//! runs the reachable grid once.
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
//! application deterministic. `zeno-fcis purity src/program.rs src/laws.rs
//! src/controller.rs synthesized/transition.rs` checks the decision code
//! statically for the sources of nondeterminism that these runs might not
//! exercise.

use std::process::Command;
use withdrawal_queue::{
    Authority, authority, bindings::GeneratedProject, deposit, generated::*, profile, request, tick,
};
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

#[allow(clippy::too_many_arguments)]
fn vault(
    balance: i128,
    lane_a: LaneStatus,
    amount_a: i128,
    lane_b: LaneStatus,
    amount_b: i128,
    pause: i128,
    must_serve: bool,
    priority: Lane,
) -> Vault {
    Vault {
        balance: Money(balance),
        lane_a,
        amount_a: Money(amount_a),
        lane_b,
        amount_b: Money(amount_b),
        pause: PauseTicks(pause),
        must_serve: Flag(must_serve),
        priority,
    }
}

/// Decides every corpus input `runs` times and returns one digest per input,
/// in a fixed order.
fn corpus_digests(authority: &Authority, runs: u8) -> Vec<String> {
    use LaneStatus::{Arrived, Empty, Pending};
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let runs = ProbeRuns::try_new(runs).unwrap();
    let vaults = [
        vault(0, Empty, 0, Empty, 0, 0, false, Lane::A),
        vault(4, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        vault(4, Pending, 2, Pending, 2, 2, false, Lane::A),
        vault(3, Pending, 1, Arrived, 2, 0, true, Lane::B),
        vault(2, Empty, 0, Pending, 2, 1, false, Lane::B),
    ];
    let commands = [
        (deposit(1), false),
        (deposit(2), false),
        (request(Lane::A, 1), false),
        (request(Lane::B, 2), false),
        (tick(), false),
        (tick(), true),
    ];
    let callers = [
        Caller::Operator,
        Caller::OwnerA,
        Caller::OwnerB,
        Caller::Keeper,
    ];
    let mut digests = Vec::new();
    for (v, state) in vaults.iter().enumerate() {
        for (c, (command, alarm)) in commands.iter().enumerate() {
            for caller in &callers {
                let input = format!("{v} {c} {caller:?}");
                let root = project
                    .admit_root::<RustCryptoSha256>(state, limits)
                    .unwrap();
                let admitted_command = project
                    .admit_command::<RustCryptoSha256>(command, limits)
                    .unwrap();
                let admitted_context = project
                    .admit_context::<RustCryptoSha256>(
                        &TickContext {
                            caller: caller.clone(),
                            alarm: Flag(*alarm),
                        },
                        limits,
                    )
                    .unwrap();
                let witness = authority
                    .admit_invocation(
                        root,
                        admitted_command.admitted().clone(),
                        admitted_context.admitted().clone(),
                        profile::digest("example/withdrawal-queue/principal", b"determinism")
                            .unwrap(),
                        profile::digest("example/withdrawal-queue/authentication", b"determinism")
                            .unwrap(),
                        profile::digest("example/withdrawal-queue/replay", input.as_bytes())
                            .unwrap(),
                    )
                    .unwrap();
                let (_, probe) = authority
                    .execute_probed(witness, runs)
                    .unwrap_or_else(|error| panic!("input {input}: {error:?}"));
                assert_eq!(probe.runs(), runs.get());
                digests.push(probe.decision_digest().to_string());
            }
        }
    }
    digests
}

#[test]
fn every_corpus_input_decides_identically_across_repeated_runs() {
    let authority = authority().unwrap();
    let digests = corpus_digests(&authority, 8);
    assert_eq!(digests.len(), 120);
    // Each input has its own replay identity, so every decision is distinct.
    let distinct: std::collections::BTreeSet<_> = digests.iter().collect();
    assert_eq!(distinct.len(), 120);
    println!("determinism: 120 inputs x 8 runs in this process, 0 divergences");
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
    println!("determinism: 120 inputs in 3 changed-environment processes, 0 divergences");
}

#[test]
#[ignore = "spawned by decisions_match_in_child_processes_with_a_changed_environment"]
fn corpus_digest_helper() {
    let authority = authority().unwrap();
    println!("{DIGESTS}{}", corpus_digests(&authority, 2).join(","));
}
