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
