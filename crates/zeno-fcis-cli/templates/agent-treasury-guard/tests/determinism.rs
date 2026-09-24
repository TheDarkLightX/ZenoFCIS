//! Checks that the executed application decided a corpus of inputs the same
//! way in every run this test makes.
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
//! these runs might not exercise, such as a clock or price-feed read.

use agent_treasury_guard::{
    Authority, authority, bindings::GeneratedProject, context, failed, generated::*, profile,
    propose, settled, treasury,
};
use std::process::Command;
use zeno_fcis_authority::ProbeRuns;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;

use Caller::{Agent, Dex};
use Direction::{BuyBase, SellBase};
use ModelId::{TreasuryAgentV1 as V1, TreasuryAgentV2 as V2};
use PendingSwap::{NoSwap, PendingBuy, PendingSell};

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
/// The corpus is six treasuries (genesis, idle after a day of trading, a
/// pending buy, a pending sell, one at the reserve, and one with the budget
/// spent), each with twelve commands (six proposals, four settlements, two
/// failures) under twelve contexts: both callers, at the last tick, the next
/// tick, and the next day, with a fresh price under the approved model and a
/// stale price under a retired one.
fn corpus_digests(authority: &Authority, runs: u8) -> Vec<String> {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let runs = ProbeRuns::try_new(runs).unwrap();
    let states = [
        treasury(6, 1, 0, 0, NoSwap, 0, 0),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        treasury(4, 1, 2, 1, PendingBuy, 2, 2),
        treasury(4, 2, 4, 3, PendingSell, 1, 2),
        treasury(2, 6, 0, 5, NoSwap, 0, 0),
        treasury(6, 1, 4, 3, NoSwap, 0, 0),
    ];
    let commands = [
        propose(BuyBase, 1, 1),
        propose(BuyBase, 2, 1),
        propose(BuyBase, 3, 3),
        propose(SellBase, 1, 2),
        propose(SellBase, 2, 3),
        propose(SellBase, 3, 3),
        settled(1, 1),
        settled(1, 2),
        settled(3, 1),
        settled(3, 2),
        failed(1),
        failed(3),
    ];
    let mut digests = Vec::new();
    for state in &states {
        let last_seen = state.last_seen.0;
        for now in [last_seen, last_seen + 1, last_seen + 4] {
            for caller in [Agent, Dex] {
                for (price, price_time, model) in [(1, now, V2), (2, (now - 2).max(0), V1)] {
                    for command in &commands {
                        let input = format!(
                            "{state:?} {command:?} {caller:?} {now} {price} {price_time} {model:?}"
                        );
                        let root = project
                            .admit_root::<RustCryptoSha256>(state, limits)
                            .unwrap();
                        let admitted_command = project
                            .admit_command::<RustCryptoSha256>(command, limits)
                            .unwrap();
                        let admitted_context = project
                            .admit_context::<RustCryptoSha256>(
                                &context(caller.clone(), now, price, price_time, model.clone()),
                                limits,
                            )
                            .unwrap();
                        let witness = authority
                            .admit_invocation(
                                root,
                                admitted_command.admitted().clone(),
                                admitted_context.admitted().clone(),
                                profile::digest(
                                    "example/agent-treasury-guard/principal",
                                    b"determinism",
                                ),
                                profile::digest(
                                    "example/agent-treasury-guard/authentication",
                                    b"determinism",
                                ),
                                profile::digest(
                                    "example/agent-treasury-guard/replay",
                                    input.as_bytes(),
                                ),
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
    }
    digests
}

#[test]
fn every_corpus_input_decides_identically_across_repeated_runs() {
    let authority = authority().unwrap();
    let digests = corpus_digests(&authority, 8);
    assert_eq!(digests.len(), 864);
    // Each input has its own replay identity, so every decision is distinct.
    let distinct: std::collections::BTreeSet<_> = digests.iter().collect();
    assert_eq!(distinct.len(), 864);
    println!("determinism: 864 inputs x 8 runs in this process, 0 divergences");
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
    println!("determinism: 864 inputs in 3 changed-environment processes, 0 divergences");
}

#[test]
#[ignore = "spawned by decisions_match_in_child_processes_with_a_changed_environment"]
fn corpus_digest_helper() {
    let authority = authority().unwrap();
    println!("{DIGESTS}{}", corpus_digests(&authority, 2).join(","));
}
