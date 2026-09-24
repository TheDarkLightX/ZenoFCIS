//! Checks that the executed application decides every admitted input the same
//! way on every run.
//!
//! Each input is decided eight times in this process through
//! `execute_probed`, which withholds a decision whose runs disagree. The whole
//! corpus is then decided again in child processes started with a changed
//! environment: a cleared environment, another time zone and locale, a glibc
//! allocator that fills memory with a pattern, and another working directory.
//! Each child is a fresh process, so it also gets new hash seeds and new
//! addresses. Every decision digest must match.
//!
//! Agreement is evidence about these runs only; it does not prove the
//! application deterministic. `zeno-fcis purity src/program.rs src/laws.rs
//! synthesized/transition.rs` checks the decision code statically for the
//! sources of nondeterminism that these runs might not exercise.

use durable_counter::{Authority, authority, bindings::GeneratedProject, generated::*, profile};
use std::process::Command;
use zeno_fcis_authority::ProbeRuns;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;

const INCREMENT: u16 = 120;
const RECORD_FAILURE: u16 = 121;
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

fn command(id: u16) -> CounterCommand {
    match id {
        INCREMENT => CounterCommand::Increment,
        RECORD_FAILURE => CounterCommand::RecordFailure,
        other => panic!("unknown command {other}"),
    }
}

fn state(count: i128, failures: i128) -> CounterState {
    CounterState {
        count: CounterValue(count),
        failures: CounterValue(failures),
    }
}

/// Decides every admitted input `runs` times and returns one digest per input,
/// in a fixed order.
fn corpus_digests(authority: &Authority, runs: u8) -> Vec<String> {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let runs = ProbeRuns::try_new(runs).unwrap();
    let mut digests = Vec::new();
    for count in 0..=3 {
        for failures in 0..=3 {
            for command_id in [INCREMENT, RECORD_FAILURE] {
                for allowed in [false, true] {
                    let input = format!("{count} {failures} {command_id} {allowed}");
                    let root = project
                        .admit_root::<RustCryptoSha256>(&state(count, failures), limits)
                        .unwrap();
                    let admitted_command = project
                        .admit_command::<RustCryptoSha256>(&command(command_id), limits)
                        .unwrap();
                    let context = project
                        .admit_context::<RustCryptoSha256>(&CounterContext(allowed), limits)
                        .unwrap();
                    let witness = authority
                        .admit_invocation(
                            root,
                            admitted_command.admitted().clone(),
                            context.admitted().clone(),
                            profile::digest("example/durable-counter/principal", b"determinism"),
                            profile::digest(
                                "example/durable-counter/authentication",
                                b"determinism",
                            ),
                            profile::digest("example/durable-counter/replay", input.as_bytes()),
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
    }
    digests
}

#[test]
fn every_admitted_input_decides_identically_across_repeated_runs() {
    let authority = authority().unwrap();
    let digests = corpus_digests(&authority, 8);
    assert_eq!(digests.len(), 64);
    // Each input has its own replay identity, so every decision is distinct.
    let distinct: std::collections::BTreeSet<_> = digests.iter().collect();
    assert_eq!(distinct.len(), 64);
    println!("determinism: 64 inputs x 8 runs in this process, 0 divergences");
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
    println!("determinism: 64 inputs in 3 changed-environment processes, 0 divergences");
}

#[test]
#[ignore = "spawned by decisions_match_in_child_processes_with_a_changed_environment"]
fn corpus_digest_helper() {
    let authority = authority().unwrap();
    println!("{DIGESTS}{}", corpus_digests(&authority, 2).join(","));
}
