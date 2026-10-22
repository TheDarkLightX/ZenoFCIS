//! Keyed collections: the view, the metadata and the type checks.

use zeno_fcis_spec::{
    DiagnosticCode, ProjectLimits, ProjectSpec, SourceLimits, SummaryKind, elaborate_project,
    expand_tables, parse_project,
};

const TOKENS: &str = include_str!("../../../examples/collections/token-balances/project.zeno");

fn spec(source: &str) -> ProjectSpec {
    elaborate_project(
        parse_project(source, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}")),
        ProjectLimits::default(),
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
}

/// The refusal paths of a source that adds `extra` to the token example.
fn refusals(extra: &str) -> Vec<(DiagnosticCode, String, String)> {
    let source = format!("{TOKENS}{extra}\n");
    parse_project(&source, SourceLimits::default())
        .err()
        .unwrap_or_else(|| panic!("unexpected acceptance: {extra}"))
        .diagnostics()
        .iter()
        .map(|d| {
            (
                d.code(),
                d.path().as_str().to_owned(),
                d.expected().to_owned(),
            )
        })
        .collect()
}

#[test]
fn collections_lower_to_a_view_of_named_entries() {
    let project = spec(TOKENS);
    let state: Vec<(u32, &str)> = project
        .fields()
        .iter()
        .filter(|f| f.owner().get() == 100)
        .map(|f| (f.id().get(), f.name().as_str()))
        .collect();
    assert_eq!(
        state,
        [
            (120, "supply"),
            (160, "sender"),
            (161, "sender_balance"),
            (170, "receiver"),
            (171, "receiver_balance")
        ]
    );
    let range = |id: u32| {
        let t = project
            .types()
            .iter()
            .find(|t| t.id().get() == id)
            .unwrap_or_else(|| panic!("missing declaration"));
        let r = t.range().unwrap_or_else(|| panic!("expected range"));
        (r.min(), r.max())
    };
    // Two key positions of AccountId: from and to.
    assert_eq!(range(103), (0, 1));
    assert_eq!(range(110), (0, 1));
    let collections = project.collections();
    assert_eq!(collections.keys()[0].positions(), 2);
    assert_eq!(collections.maps()[0].capacity(), 1_048_576);
    assert_eq!(collections.entries().len(), 2);
    assert_eq!(
        collections.summaries()[0].kind(),
        SummaryKind::Sum(
            zeno_fcis_spec::StableId::new(140).unwrap_or_else(|| panic!("invalid stable ID"))
        )
    );
    // The per-entry law is the conjunction over the named entries.
    let law = project
        .laws()
        .iter()
        .find(|l| l.id().get() == 501)
        .unwrap_or_else(|| panic!("missing declaration"));
    let written = spec(&TOKENS.replace(
        "forall e in entries 110 { post.100.e == 1 || post.100.e.140 == 0 }",
        "(post.100.160 == 1 || post.100.161 == 0) && (post.100.170 == 1 || post.100.171 == 0)",
    ));
    assert_eq!(
        law.formula(),
        written
            .laws()
            .iter()
            .find(|l| l.id().get() == 501)
            .unwrap_or_else(|| panic!("missing declaration"))
            .formula()
    );
}

#[test]
fn collections_print_their_view_and_bind_their_metadata() {
    let expansion =
        expand_tables(TOKENS, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    for line in [
        "type 103 int AccountId in 0..=1;",
        "type 110 int accounts_present in 0..=1;",
        "field 120 100 supply 106;",
        "field 160 100 sender 110;\nfield 161 100 sender_balance 105;",
        "field 170 100 receiver 110;\nfield 171 100 receiver_balance 105;",
    ] {
        assert!(expansion.text.contains(line), "{line}");
    }
    let original = spec(TOKENS);
    let view = spec(&expansion.text);
    assert!(view.collections().is_empty());
    assert_eq!(
        original
            .view()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("{error:?}")),
        view.canonical_bytes()
            .unwrap_or_else(|error| panic!("{error:?}"))
    );
    // The full encoding is format 5 and binds the capacity, which the view
    // does not.
    let bytes = original
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("{error:?}"));
    let marker = b"ZFCIS-PROJECT-SPEC\0".len();
    assert_eq!(&bytes[marker..marker + 2], &5u16.to_be_bytes());
    let smaller = spec(&TOKENS.replace("capacity 1048576", "capacity 1024"));
    assert_ne!(
        smaller
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("{error:?}")),
        bytes
    );
    assert_eq!(
        smaller
            .view()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("{error:?}")),
        original
            .view()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("{error:?}"))
    );
}

#[test]
fn collections_refuse_reading_a_map_except_through_named_entries() {
    for extra in [
        "law 503 whole_map on commit = post.100.110 == 0;",
        "law 503 deep on commit = pre.100.110.140 >= 0;",
    ] {
        assert!(
            refusals(extra).iter().any(|(code, path, expected)| *code
                == DiagnosticCode::InvalidDeclaration
                && path == "formula.key"
                && expected.contains("named entries")),
            "{extra}: {:?}",
            refusals(extra)
        );
    }
    // An entry is named only by a command or context key field.
    assert!(
        refusals("entry 165 by_amount of 110 at command.101.133;")
            .iter()
            .any(|(_, path, _)| path == "entry.key")
    );
}

#[test]
fn collections_compare_keys_only_by_equality() {
    for extra in [
        "law 503 ordered on commit = command.101.131 < command.101.132;",
        "law 503 numbered on commit = command.101.131 == 0;",
        "law 503 counted on commit = command.101.131 + 1 == command.101.132;",
    ] {
        assert!(
            refusals(extra)
                .iter()
                .any(|(_, path, _)| path == "formula.key"),
            "{extra}"
        );
    }
    // Equality and inequality between keys are the only key operations.
    let source = format!(
        "{TOKENS}law 503 distinct on commit = command.101.131 != command.101.132 || command.101.130 == 150;\n"
    );
    assert!(parse_project(&source, SourceLimits::default()).is_ok());
}

#[test]
fn collections_refuse_aggregates_without_a_summary() {
    for extra in [
        "law 503 few on commit = count e in entries 110 { post.100.e == 1 } <= 1;",
        "law 503 small on commit = sum e in entries 110 { post.100.e.140 } <= 3;",
    ] {
        assert!(
            refusals(extra)
                .iter()
                .any(|(_, path, _)| path == "entries.aggregate"),
            "{extra}"
        );
    }
    // A summary of a field the map's record lacks is refused too.
    assert!(
        refusals("summary 121 100 nothing 106 sum 110.999;")
            .iter()
            .any(|(_, path, _)| path == "summary.field")
    );
}

#[test]
fn collections_keep_keys_out_of_scalar_state_and_values_out_of_nested_records() {
    assert!(
        refusals("field 125 100 owner 103;")
            .iter()
            .any(|(_, path, _)| path == "field.type")
    );
    assert!(
        refusals("map 111 100 loose 103 105 capacity 3;")
            .iter()
            .any(|(_, path, _)| path == "map.value")
    );
}

#[test]
fn collections_state_laws_read_one_named_entry_or_only_scalars() {
    // Assumed on every pre-view, a state law must survive while unnamed
    // entries keep their values: a pair of entries, or an entry with scalar
    // state, could be false at keys no decision named.
    for extra in [
        "law 503 pair on commit, genesis = post.100.161 + post.100.171 <= 3;",
        "law 503 under_supply on commit, genesis = post.100.161 <= post.100.120;",
    ] {
        assert!(
            refusals(extra)
                .iter()
                .any(|(_, path, _)| path == "law.entries"),
            "{extra}"
        );
    }
    // Separate conjuncts, each on one entry or on scalars, are fine, and so
    // is a step law over both entries.
    for extra in [
        "law 503 each on commit, genesis = post.100.161 <= 3 && post.100.171 <= 3 && post.100.120 <= 6;",
        "law 503 step on commit = post.100.161 + post.100.171 <= 6;",
    ] {
        let source = format!("{TOKENS}{extra}\n");
        assert!(
            parse_project(&source, SourceLimits::default()).is_ok(),
            "{extra}"
        );
    }
}

#[test]
fn collection_format_five_keeps_rewards_before_collection_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let source = format!(
        "{TOKENS}\nrandom 600 entropy on command.101.133 bits 2 independent uniform;\nreward 610 payout random {{600}} on post.100.120 failure -1 {{ expectation >= 0; }}\n"
    );
    let project = spec(&source);
    let encoded = project.canonical_bytes()?;
    let view = project.view().canonical_bytes()?;
    let marker = b"ZFCIS-PROJECT-SPEC\0".len();
    assert_eq!(&encoded[marker..marker + 2], &5u16.to_be_bytes());
    assert_eq!(&view[marker..marker + 2], &4u16.to_be_bytes());
    assert_eq!(project.rewards().len(), 1);
    // Everything after the format word in the view, including the complete
    // random/distribution/reward section, precedes the collection metadata.
    assert!(encoded.len() > view.len());
    assert_eq!(&encoded[marker + 2..view.len()], &view[marker + 2..]);
    let changed = spec(&source.replace("expectation >= 0", "expectation >= 1"));
    assert_ne!(encoded, changed.canonical_bytes()?);
    Ok(())
}

#[test]
fn standalone_state_law_keeps_collection_entry_locality() {
    use zeno_fcis_spec::{LawApplicability, LawScope, parse_law_formula};
    let state = LawApplicability::new(LawScope::Committing, true);
    let step = LawApplicability::new(LawScope::Always, false);
    for formula in [
        "post.100.161 + post.100.171 <= 3",
        "post.100.161 <= post.100.120",
        "choose(true, post.100.161, post.100.171) >= 0",
    ] {
        assert!(
            parse_law_formula(TOKENS, formula, state, SourceLimits::default()).is_err(),
            "state law mixed entries: {formula}"
        );
        assert!(
            parse_law_formula(TOKENS, formula, step, SourceLimits::default()).is_ok(),
            "step formula: {formula}"
        );
    }
    assert!(
        parse_law_formula(
            TOKENS,
            "post.100.161 >= 0 && post.100.171 >= 0 && post.100.120 >= 0",
            state,
            SourceLimits::default()
        )
        .is_ok()
    );
}
