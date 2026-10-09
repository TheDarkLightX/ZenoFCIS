//! Every decision example against the library Authority, the contract's
//! genesis, and the examples as one persistent session.

use application::{authority, check, examples, genesis, genesis_numbers, state, v2_contract};
use zeno_fcis_synthesis::finite::{
    V2InputLeaf as InputLeaf, v2_authority::PublicationOutcome, v2_composition as c,
};

const EXAMPLES: &str = include_str!("decision-examples.txt");

#[test]
fn every_example_is_the_authority_decision() {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor).unwrap_or_else(|error| panic!("{error}"));
    let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
    for example in &examples {
        check(&authority, example).unwrap_or_else(|error| panic!("{error}"));
    }
    println!("decision examples checked: {}", examples.len());
}

#[test]
fn genesis_publishes_only_the_contract_genesis() {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor).unwrap_or_else(|error| panic!("{error}"));
    let initial = genesis().unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        authority.publish_genesis(&initial),
        PublicationOutcome::Commit(_)
    ));
    // Each other value a state field's domain allows is refused by law 990.
    let c::Schema::Record(fields) = descriptor.state else {
        panic!("the state root is a record")
    };
    let numbers = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
    let mut refused = 0;
    for (index, field) in fields.iter().enumerate() {
        let others: Vec<i128> = match &field.leaf {
            InputLeaf::Bool => vec![1 - numbers[index]],
            InputLeaf::I128 { min, max } => [numbers[index] - 1, numbers[index] + 1]
                .into_iter()
                .filter(|value| (i128::from(*min)..=i128::from(*max)).contains(value))
                .collect(),
            InputLeaf::Sum { variants, .. } => variants
                .iter()
                .map(|variant| i128::from(variant.id))
                .filter(|value| *value != numbers[index])
                .collect(),
            other => panic!("unexpected state leaf {other:?}"),
        };
        for other in others {
            let mut changed = numbers.clone();
            changed[index] = other;
            let changed = state(&descriptor, &changed).unwrap_or_else(|error| panic!("{error}"));
            assert!(
                !matches!(
                    authority.publish_genesis(&changed),
                    PublicationOutcome::Commit(_)
                ),
                "field {} = {other}",
                field.id
            );
            refused += 1;
        }
    }
    assert!(refused > 0, "no other genesis value to try");
}

#[cfg(feature = "sqlite")]
#[test]
fn examples_run_as_one_persistent_session() {
    let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
    let directory = std::env::temp_dir().join(format!(
        "{}-session-{}",
        env!("CARGO_PKG_NAME"),
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    let path = directory.join("store.sqlite");
    let _ = std::fs::remove_file(&path);
    let summary = application::journey(&path, &examples).unwrap_or_else(|error| panic!("{error}"));
    // The whole lineage replays the store, which already runs this version:
    // there is nothing to upgrade it to, and a refusal writes nothing.
    let head = application::audit(&path).unwrap_or_else(|error| panic!("{error}"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{error}"));
    let refused = application::upgrade(&path);
    assert_eq!(std::fs::read(&path).ok(), Some(bytes));
    let _ = std::fs::remove_dir_all(&directory);
    let committed = summary
        .decisions
        .iter()
        .filter(|decision| **decision != "Reject")
        .count();
    // One bundle per committed decision; genesis is the store's anchor.
    assert_eq!(summary.bundles, committed as u64);
    assert_eq!(summary.pending, 0);
    assert_eq!(head.contract_version, v2_contract::VERSION as usize);
    assert_eq!(head.commits, summary.bundles);
    assert_eq!(head.upgrades, 0);
    assert!(
        matches!(&refused, Err(error) if error.contains("SameContract")),
        "{refused:?}"
    );
    println!("session: {}", summary.json());
}

/// Each example written in the grammar's other forms, line for line: the
/// inputs split into state, command and context sections; comments
/// indented; and, when the contract declares one channel, each delivery as
/// its payload numbers alone. Every form reads as the same examples, as
/// `zeno-fcis contract review` reads them.
#[test]
fn every_written_form_of_an_example_reads_alike() {
    let written = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let width = |schema: c::Schema<'_>| match schema {
        c::Schema::Record(fields) => fields.len(),
        _ => 1,
    };
    let (state, command) = (width(descriptor.state), width(descriptor.command));
    let one_channel = descriptor.channels.len() == 1;
    let numbers = |values: &[i128]| {
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut rewritten = String::new();
    for (index, line) in EXAMPLES.lines().enumerate() {
        let Some(example) = written.iter().find(|example| example.line == index + 1) else {
            rewritten.push_str(&format!("  \t{line}\n"));
            continue;
        };
        let class = match example.class {
            c::Class::Accept => "accept",
            c::Class::Reject => "reject",
            c::Class::CommittedFailure => "failure",
            other => panic!("line {}: class {other:?}", example.line),
        };
        let reason = example
            .reason
            .map_or_else(|| "-".to_owned(), |reason| reason.to_string());
        let deliveries = if example.outbox.is_empty() {
            "-".to_owned()
        } else {
            example
                .outbox
                .iter()
                .map(|(channel, payload)| {
                    if one_channel && !payload.is_empty() {
                        numbers(payload)
                    } else {
                        format!("{channel} {}", numbers(payload))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        let (pre, rest) = example.inputs.split_at(state);
        let (order, context) = rest.split_at(command);
        rewritten.push_str(&format!(
            "{} | {} | {} | {class} {reason} {} | {deliveries}\n",
            numbers(pre),
            numbers(order),
            numbers(context),
            numbers(&example.post)
        ));
    }
    let reread = examples(&rewritten).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(reread, written);
    println!("decision example forms read alike: {}", written.len());
}

/// The operational command line, run in process: each command once as
/// intended and once refused, on a store the decision examples drive from
/// genesis through `submit`.
#[cfg(feature = "sqlite")]
mod commands {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use application::cli::{self, BLOCKED, FAILURE, INVALID, OK, Outcome, USAGE};
    use application::{Example, examples, genesis_numbers, journal_path, v2_contract};
    use zeno_fcis_synthesis::finite::v2_composition as c;

    use super::EXAMPLES;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "{}-commands-{name}-{}",
                env!("CARGO_PKG_NAME"),
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
            Self(directory)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(words: &[&str], extra: &[String]) -> Outcome {
        let arguments: Vec<OsString> = words
            .iter()
            .map(OsString::from)
            .chain(extra.iter().map(OsString::from))
            .collect();
        cli::run(&arguments).unwrap_or_else(|| panic!("{words:?} is not a command"))
    }

    fn path(path: &Path) -> &str {
        path.to_str().unwrap_or_else(|| panic!("{path:?}"))
    }

    fn expect(outcome: &Outcome, exit: u8, contains: &str) {
        assert_eq!(outcome.exit, exit, "{outcome:?}");
        assert!(
            outcome.stdout.contains(contains) || outcome.stderr.contains(contains),
            "{contains:?} not in {outcome:?}"
        );
    }

    fn widths() -> (usize, usize) {
        let contract = v2_contract::Contract::new();
        let descriptor = contract.descriptor();
        let width = |schema: c::Schema<'_>| match schema {
            c::Schema::Record(fields) => fields.len(),
            _ => 1,
        };
        (width(descriptor.state), width(descriptor.command))
    }

    /// The command and context words of an example.
    fn words(example: &Example) -> Vec<String> {
        let (state, command) = widths();
        let (order, context) = example.inputs[state..].split_at(command);
        cli::assignments(order, context).unwrap_or_else(|error| panic!("{error}"))
    }

    /// The first example on `state` whose class is not a reject.
    fn committing<'e>(examples: &'e [Example], state: &[i128]) -> Option<&'e Example> {
        examples.iter().find(|example| {
            example.class != c::Class::Reject && example.inputs.get(..state.len()) == Some(state)
        })
    }

    fn bytes(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    #[test]
    fn version_prints_the_contract_version_and_identity() {
        let outcome = run(&["version", "--format", "json"], &[]);
        expect(
            &outcome,
            OK,
            &format!("\"contract_version\":{}", v2_contract::VERSION),
        );
        let identity = application::identity().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(identity.len(), 64);
        assert!(outcome.stdout.contains(&identity));
        assert!(outcome.stdout.contains(env!("CARGO_PKG_NAME")));
        expect(&run(&["version", "extra"], &[]), USAGE, "usage");
        expect(
            &run(&["version", "--format", "xml"], &[]),
            USAGE,
            "--format",
        );
    }

    #[test]
    fn init_creates_a_store_and_refuses_an_existing_one() {
        let scratch = Scratch::new("init");
        let store = scratch.path("store.sqlite");
        expect(
            &run(&["init", path(&store), "--format", "json"], &[]),
            OK,
            "\"status\":\"initialized\"",
        );
        assert_eq!(bytes(&journal_path(&store)), Some(Vec::new()));
        let before = bytes(&store);
        expect(
            &run(&["init", path(&store)], &[]),
            INVALID,
            "already exists",
        );
        assert_eq!(bytes(&store), before);
        expect(&run(&["init"], &[]), USAGE, "usage");
    }

    #[test]
    fn state_reads_the_store_and_refuses_another_file() {
        let scratch = Scratch::new("state");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let current = application::current(&store).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            current.state,
            genesis_numbers().unwrap_or_else(|error| panic!("{error}"))
        );
        expect(
            &run(&["state", path(&store), "--format", "json"], &[]),
            OK,
            "\"commits\":0",
        );
        let other = scratch.path("other.sqlite");
        std::fs::write(&other, b"not a store").unwrap_or_else(|error| panic!("{error}"));
        expect(&run(&["state", path(&other)], &[]), INVALID, "store:");
        assert_eq!(bytes(&other).as_deref(), Some(b"not a store".as_slice()));
    }

    #[test]
    fn decide_commits_nothing_and_refuses_an_unknown_field() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let scratch = Scratch::new("decide");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let before = bytes(&store);
        let mut tried = 0;
        for example in examples
            .iter()
            .filter(|example| example.inputs.get(..genesis.len()) == Some(genesis.as_slice()))
        {
            let outcome = run(
                &["decide", path(&store), "--format", "json"],
                &words(example),
            );
            let class = match example.class {
                c::Class::Accept => "Accept",
                c::Class::Reject => "Reject",
                _ => "CommittedFailure",
            };
            let exit = if class == "Reject" { BLOCKED } else { OK };
            expect(&outcome, exit, &format!("\"class\":\"{class}\""));
            assert!(outcome.stdout.contains("\"commit\":null"), "{outcome:?}");
            tried += 1;
        }
        assert_eq!(bytes(&store), before);
        expect(
            &run(&["decide", path(&store), "no_such_field=1"], &[]),
            INVALID,
            "unknown field `no_such_field`",
        );
        assert_eq!(bytes(&store), before);
        println!("decide: {tried} examples at genesis");
    }

    #[test]
    fn submit_commits_records_and_refuses_bad_input_without_writing() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let scratch = Scratch::new("submit");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let before = bytes(&store);
        // Refusals: an unknown field, a value of no declared form, a missing
        // field, and a value outside its declared range.
        expect(
            &run(&["submit", path(&store), "no_such_field=1"], &[]),
            INVALID,
            "unknown field",
        );
        let Some(example) = committing(&examples, &genesis) else {
            println!("submit: no committing example at genesis");
            return;
        };
        let mut written = words(example);
        let (first, _) = written[0].split_once('=').unwrap_or_default();
        let first = first.to_owned();
        written[0] = format!("{first}=?");
        expect(&run(&["submit", path(&store)], &written), INVALID, &first);
        expect(
            &run(&["submit", path(&store)], &words(example)[1..]),
            INVALID,
            "missing field",
        );
        let contract = v2_contract::Contract::new();
        let descriptor = contract.descriptor();
        let (state, _) = widths();
        let leaves: Vec<_> = [descriptor.command, descriptor.context]
            .into_iter()
            .flat_map(|schema| match schema {
                c::Schema::Record(fields) => fields.iter().map(|field| &field.leaf).collect(),
                c::Schema::Leaf(leaf) => vec![leaf],
                _ => Vec::new(),
            })
            .collect();
        let numbers = &example.inputs[state..];
        if let Some((index, max)) = leaves
            .iter()
            .enumerate()
            .find_map(|(index, leaf)| match leaf {
                zeno_fcis_synthesis::finite::V2InputLeaf::I128 { max, .. } => Some((index, *max)),
                _ => None,
            })
        {
            let mut outside = numbers.to_vec();
            outside[index] = i128::from(max) + 1;
            let (command, context) = outside.split_at(widths().1);
            let written =
                cli::assignments(command, context).unwrap_or_else(|error| panic!("{error}"));
            expect(
                &run(&["submit", path(&store)], &written),
                INVALID,
                "outside the domain",
            );
        }
        assert_eq!(bytes(&store), before);
        assert_eq!(bytes(&journal_path(&store)), Some(Vec::new()));
        // The committing example commits at position 1 and is recorded.
        let outcome = run(
            &["submit", path(&store), "--format", "json"],
            &words(example),
        );
        expect(&outcome, OK, "\"status\":\"committed\"");
        assert!(outcome.stdout.contains("\"commit\":1"), "{outcome:?}");
        let current = application::current(&store).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(current.state, example.post);
        assert_eq!(current.head.commits, 1);
        let journal = std::fs::read_to_string(journal_path(&store)).unwrap_or_default();
        assert_eq!(journal.lines().count(), 1);
    }

    #[test]
    fn a_rejected_submission_writes_nothing() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let Some(example) = examples.iter().find(|example| {
            example.class == c::Class::Reject
                && example.inputs.get(..genesis.len()) == Some(genesis.as_slice())
        }) else {
            println!("submit: no rejected example at genesis");
            return;
        };
        let scratch = Scratch::new("reject");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let before = (bytes(&store), bytes(&journal_path(&store)));
        expect(
            &run(
                &["submit", path(&store), "--format", "json"],
                &words(example),
            ),
            BLOCKED,
            "\"status\":\"rejected\"",
        );
        assert_eq!((bytes(&store), bytes(&journal_path(&store))), before);
    }

    /// The command and context numbers of an example.
    fn inputs(example: &Example) -> Vec<i128> {
        example.inputs[widths().0..].to_vec()
    }

    /// The commit position a JSON `submit` outcome reports.
    fn commit_of(outcome: &Outcome) -> Option<u64> {
        let (_, rest) = outcome.stdout.split_once("\"commit\":")?;
        rest.split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    }

    /// Inputs that commit on the store's current state and differ from
    /// `base`, each `base` with one number moved a little, as the Authority
    /// decides them through `decide`.
    fn alternatives(store: &Path, base: &[i128]) -> Vec<Vec<i128>> {
        let width = widths().1;
        let mut found: Vec<Vec<i128>> = Vec::new();
        for index in 0..base.len() {
            for delta in [-2, -1, 1, 2] {
                let mut candidate = base.to_vec();
                candidate[index] += delta;
                let (command, context) = candidate.split_at(width);
                if application::preview(store, command, context)
                    .is_ok_and(|decision| decision.class != "Reject")
                    && !found.contains(&candidate)
                {
                    found.push(candidate);
                }
            }
        }
        found
    }

    fn written(numbers: &[i128]) -> Vec<String> {
        let (command, context) = numbers.split_at(widths().1);
        cli::assignments(command, context).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn concurrent_submissions_keep_the_journal_true_to_the_store() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let Some(example) = committing(&examples, &genesis) else {
            println!("submit: no committing example at genesis");
            return;
        };
        let scratch = Scratch::new("race-search");
        let probe = scratch.path("probe.sqlite");
        assert_eq!(run(&["init", path(&probe)], &[]).exit, OK);
        let first = inputs(example);
        let Some(second) = alternatives(&probe, &first).into_iter().next() else {
            println!("submit: no second committing input at genesis");
            return;
        };
        for trial in 0..12 {
            let scratch = Scratch::new(&format!("race-{trial}"));
            let store = scratch.path("store.sqlite");
            assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
            let barrier = std::sync::Barrier::new(2);
            let outcomes: Vec<(Outcome, Vec<i128>)> = std::thread::scope(|scope| {
                let handles: Vec<_> = [&first, &second]
                    .into_iter()
                    .map(|numbers| {
                        let (barrier, store) = (&barrier, &store);
                        scope.spawn(move || {
                            let words = written(numbers);
                            barrier.wait();
                            (
                                run(&["submit", path(store), "--format", "json"], &words),
                                numbers.clone(),
                            )
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or_else(|_| panic!("submit panicked")))
                    .collect()
            });
            let history = application::history(&store)
                .unwrap_or_else(|error| panic!("trial {trial}: {error}: {outcomes:?}"));
            let current = application::current(&store).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(history.entries.len() as u64, current.head.commits);
            let journal = std::fs::read_to_string(journal_path(&store)).unwrap_or_default();
            assert_eq!(
                journal.lines().count() as u64,
                current.head.commits,
                "trial {trial}: a submission that did not commit left its line: {journal:?} {outcomes:?}"
            );
            for (outcome, submitted) in &outcomes {
                if outcome.exit != OK {
                    continue;
                }
                let commit = commit_of(outcome).unwrap_or_else(|| panic!("{outcome:?}"));
                let entry = &history.entries[usize::try_from(commit - 1).unwrap_or(usize::MAX)];
                let recorded: Vec<i128> = entry
                    .command
                    .iter()
                    .chain(&entry.context)
                    .copied()
                    .collect();
                assert_eq!(
                    &recorded, submitted,
                    "trial {trial}: history shows other inputs for commit {commit}"
                );
            }
        }
    }

    #[test]
    fn a_journal_line_past_the_head_is_ignored_and_then_removed() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let Some(example) = committing(&examples, &genesis) else {
            println!("submit: no committing example at genesis");
            return;
        };
        let scratch = Scratch::new("stale");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let numbers = |values: &[i128]| {
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let submitted = inputs(example);
        let (command, context) = submitted.split_at(widths().1);
        let line = format!(
            "1 {} | {} | {}\n",
            v2_contract::VERSION,
            numbers(command),
            numbers(context)
        );
        // A process that stopped between its journal line and its commit,
        // here with a second line cut short.
        std::fs::write(journal_path(&store), format!("{line}2 1 | 1"))
            .unwrap_or_else(|error| panic!("{error}"));
        let history = application::history(&store).unwrap_or_else(|error| panic!("{error}"));
        assert!(history.entries.is_empty());
        expect(
            &run(
                &["submit", path(&store), "--format", "json"],
                &words(example),
            ),
            OK,
            "\"commit\":1",
        );
        assert_eq!(
            std::fs::read_to_string(journal_path(&store)).unwrap_or_default(),
            line
        );
        assert_eq!(
            application::history(&store)
                .unwrap_or_else(|error| panic!("{error}"))
                .entries
                .len(),
            1
        );
        // Two lines for one commit are refused, not resolved by order.
        std::fs::write(journal_path(&store), format!("{line}{line}"))
            .unwrap_or_else(|error| panic!("{error}"));
        expect(
            &run(&["history", path(&store)], &[]),
            INVALID,
            "out of order",
        );
        // A store with commits whose journal is gone takes no submission,
        // and the refusal names a remedy that works.
        std::fs::remove_file(journal_path(&store)).unwrap_or_else(|error| panic!("{error}"));
        let before = bytes(&store);
        expect(
            &run(&["submit", path(&store)], &words(example)),
            FAILURE,
            "restore it from a backup, or create it empty",
        );
        assert_eq!(bytes(&store), before);
        assert!(!journal_path(&store).exists());
        // The second remedy: an empty journal lets submit continue, and
        // history then refuses the commit it does not record.
        std::fs::write(journal_path(&store), b"").unwrap_or_else(|error| panic!("{error}"));
        let after = run(&["submit", path(&store)], &words(example));
        assert_ne!(after.exit, FAILURE, "{after:?}");
        expect(
            &run(&["history", path(&store)], &[]),
            INVALID,
            "no submission for commit 1",
        );
    }

    #[test]
    fn a_missing_journal_at_genesis_is_created_by_submit() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let genesis = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let Some(example) = committing(&examples, &genesis) else {
            println!("submit: no committing example at genesis");
            return;
        };
        let scratch = Scratch::new("genesis-journal");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        // As an init that stopped between creating the store and its journal
        // leaves it; init itself refuses the existing store.
        std::fs::remove_file(journal_path(&store)).unwrap_or_else(|error| panic!("{error}"));
        expect(
            &run(&["init", path(&store)], &[]),
            INVALID,
            "already exists",
        );
        expect(
            &run(
                &["submit", path(&store), "--format", "json"],
                &words(example),
            ),
            OK,
            "\"commit\":1",
        );
        let journal = std::fs::read_to_string(journal_path(&store)).unwrap_or_default();
        assert_eq!(journal.lines().count(), 1, "{journal:?}");
        assert!(journal.starts_with("1 "), "{journal:?}");
        assert_eq!(
            application::history(&store)
                .unwrap_or_else(|error| panic!("{error}"))
                .entries
                .len(),
            1
        );
    }

    /// Accepts the first delivery and refuses every later one.
    struct FirstOnly(Vec<String>);

    impl application::Destination for FirstOnly {
        fn send(
            &mut self,
            delivery: &application::Outgoing,
        ) -> Result<application::Sent, application::Failure> {
            if self.0.is_empty() {
                self.0.push(delivery.id.clone());
                Ok(application::Sent::Accepted)
            } else {
                Err(application::Failure::Refused(
                    "refused by the test".to_owned(),
                ))
            }
        }
    }

    #[test]
    fn a_failed_deliver_names_the_deliveries_it_completed() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let scratch = Scratch::new("partial");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let (_, deliveries) = drive(&store, &examples);
        if deliveries < 2 {
            // The example contracts deliver at most once per store; the
            // message itself is checked by the session's unit test.
            println!("deliver: fewer than two deliveries");
            return;
        }
        let mut destination = FirstOnly(Vec::new());
        let failure = application::deliver_to(&store, &mut destination)
            .err()
            .unwrap_or_else(|| panic!("the second delivery was refused"));
        let message = failure.to_string();
        assert!(matches!(failure, application::Failure::Refused(_)));
        assert!(message.contains("stay delivered"), "{message}");
        assert!(message.contains(&destination.0[0]), "{message}");
        assert!(message.contains("refused by the test"), "{message}");
        let after = application::pending(&store).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(after.head.pending, deliveries as u64 - 1);
    }

    /// Submits, while one remains, the first example whose pre-state is the
    /// store's state; returns how many committed and how many deliveries
    /// they made.
    fn drive(store: &Path, examples: &[Example]) -> (u64, usize) {
        let mut state = genesis_numbers().unwrap_or_else(|error| panic!("{error}"));
        let mut remaining: Vec<&Example> = examples.iter().collect();
        let (mut commits, mut deliveries) = (0, 0);
        while let Some(position) = remaining
            .iter()
            .position(|example| example.inputs.get(..state.len()) == Some(state.as_slice()))
        {
            let example = remaining.remove(position);
            let outcome = run(&["submit", path(store)], &words(example));
            if example.class == c::Class::Reject {
                assert_eq!(outcome.exit, BLOCKED, "{outcome:?}");
            } else {
                assert_eq!(outcome.exit, OK, "{outcome:?}");
                commits += 1;
                deliveries += example.outbox.len();
                state.clone_from(&example.post);
            }
        }
        (commits, deliveries)
    }

    #[test]
    fn history_decides_every_submission_again_and_refuses_a_lost_journal() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let scratch = Scratch::new("history");
        let store = scratch.path("store.sqlite");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let (commits, _) = drive(&store, &examples);
        let history = application::history(&store).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(history.entries.len() as u64, commits);
        assert_eq!(history.head.commits, commits);
        let current = application::current(&store).unwrap_or_else(|error| panic!("{error}"));
        if let Some(last) = history.entries.last() {
            assert_eq!(last.decision.post, current.state);
        }
        expect(
            &run(&["history", path(&store), "--format", "json"], &[]),
            OK,
            "\"status\":\"history\"",
        );
        if commits > 0 {
            // A journal that lost its submissions cannot account for the store.
            std::fs::write(journal_path(&store), b"").unwrap_or_else(|error| panic!("{error}"));
            expect(
                &run(&["history", path(&store)], &[]),
                INVALID,
                "no submission for commit 1",
            );
        }
        std::fs::remove_file(journal_path(&store)).unwrap_or_else(|error| panic!("{error}"));
        expect(
            &run(&["history", path(&store)], &[]),
            FAILURE,
            "submission journal",
        );
    }

    #[test]
    fn pending_and_deliver_send_each_delivery_once_to_the_file() {
        let examples = examples(EXAMPLES).unwrap_or_else(|error| panic!("{error}"));
        let scratch = Scratch::new("deliver");
        let store = scratch.path("store.sqlite");
        let file = scratch.path("deliveries.jsonl");
        assert_eq!(run(&["init", path(&store)], &[]).exit, OK);
        let (_, deliveries) = drive(&store, &examples);
        let waiting = application::pending(&store).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(waiting.head.pending, deliveries as u64);
        expect(
            &run(&["pending", path(&store), "--format", "json"], &[]),
            OK,
            &format!("\"pending\":{deliveries}"),
        );
        expect(&run(&["deliver", path(&store)], &[]), USAGE, "--to");
        if let Some(next) = &waiting.next {
            // A file that holds the next delivery's ID with other content is
            // refused, and the delivery stays pending.
            let forged = format!("{{\"delivery_id\":\"{}\",\"forged\":true}}\n", next.id);
            std::fs::write(&file, &forged).unwrap_or_else(|error| panic!("{error}"));
            expect(
                &run(&["deliver", path(&store), "--to", path(&file)], &[]),
                INVALID,
                "other content",
            );
            let after = application::pending(&store).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(after.head.pending, deliveries as u64);
            // An interrupted append: the delivery's line without its newline
            // is completed, not duplicated.
            let partial = next.json();
            std::fs::write(&file, &partial.as_bytes()[..partial.len() / 2])
                .unwrap_or_else(|error| panic!("{error}"));
        }
        let outcome = run(
            &[
                "deliver",
                path(&store),
                "--to",
                path(&file),
                "--format",
                "json",
            ],
            &[],
        );
        expect(&outcome, OK, "\"pending\":0");
        let lines = std::fs::read_to_string(&file).unwrap_or_default();
        assert_eq!(lines.lines().count(), deliveries);
        assert!(lines.is_empty() || lines.ends_with('\n'));
        if let Some(next) = &waiting.next {
            assert_eq!(lines.lines().next(), Some(next.json().as_str()));
        }
        // Delivering again sends nothing and leaves the file as it was.
        expect(
            &run(&["deliver", path(&store), "--to", path(&file)], &[]),
            OK,
            "delivered 0",
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap_or_default(), lines);
        let other = scratch.path("other.sqlite");
        std::fs::write(&other, b"not a store").unwrap_or_else(|error| panic!("{error}"));
        expect(&run(&["pending", path(&other)], &[]), INVALID, "store:");
        println!("delivered to the file: {deliveries}");
    }
}
