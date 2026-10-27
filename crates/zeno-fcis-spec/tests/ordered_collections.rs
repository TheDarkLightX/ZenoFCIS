//! Bounded ordered collection syntax and assurance checks.

use zeno_fcis_spec::{elaborate_project, expand_tables, parse_project};

const BOOK: &str = include_str!("../../../examples/collections/order-book/project.zeno");

#[test]
fn ordered_metadata_and_expansion_are_bound_separately() -> Result<(), Box<dyn std::error::Error>> {
    let spec = elaborate_project(
        parse_project(BOOK, Default::default()).map_err(|e| format!("{e:?}"))?,
        Default::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(spec.collections().keys()[0].is_ordered());
    assert_eq!(
        spec.collections().keys()[0]
            .order_range()
            .map(|r| (r.min(), r.max())),
        Some((0, 1000000))
    );
    assert_eq!(spec.collections().order_reads().len(), 1);
    let marker = b"ZFCIS-PROJECT-SPEC\0".len();
    assert_eq!(
        &spec.canonical_bytes()?[marker..marker + 2],
        &6u16.to_be_bytes()
    );
    let expanded = expand_tables(BOOK, Default::default()).map_err(|e| format!("{e:?}"))?;
    let view = elaborate_project(
        parse_project(&expanded.text, Default::default()).map_err(|e| format!("{e:?}"))?,
        Default::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(view.collections().is_empty());
    assert_eq!(spec.view().canonical_bytes()?, view.canonical_bytes()?);
    for changed in [
        BOOK.replace(" max into", " min into"),
        BOOK.replace("1000000;", "999999;"),
    ] {
        let changed = elaborate_project(
            parse_project(&changed, Default::default()).map_err(|e| format!("{e:?}"))?,
            Default::default(),
        )
        .map_err(|e| format!("{e:?}"))?;
        assert_ne!(spec.canonical_bytes()?, changed.canonical_bytes()?);
        assert_eq!(
            spec.view().canonical_bytes()?,
            changed.view().canonical_bytes()?
        );
    }
    Ok(())
}

#[test]
fn opaque_order_read_is_refused() {
    let source = BOOK
        .replace("ordered Price in 0..=1000000", "key Price")
        .replace(
            "command.101.131 >= context.102.132",
            "command.101.131 == context.102.132",
        );
    let parsed = parse_project(&source, Default::default());
    assert!(parsed.is_err(), "opaque order read accepted");
    let error = parsed.err().unwrap_or_else(|| panic!("asserted refusal"));
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|d| d.path().as_str() == "order.read"),
        "{error:?}"
    );
}

#[test]
fn ordered_keys_are_compared_but_never_numbered_or_mixed() {
    for expression in [
        "command.101.131 == 0",
        "command.101.131 + 1 == context.102.132",
        "command.101.131 == command.101.130",
    ] {
        let source = BOOK.replace("command.101.131 >= context.102.132", expression);
        assert!(
            parse_project(&source, Default::default()).is_err(),
            "{expression}"
        );
    }
    for op in ["==", "!=", "<", "<=", ">", ">="] {
        assert!(
            parse_project(&BOOK.replace(">=", op), Default::default()).is_ok(),
            "{op}"
        );
    }
}

#[test]
fn order_reads_reject_bad_outputs_chains_duplicates_and_unbounded_counts() {
    for source in [
        BOOK.replace("into context.102.132", "into command.101.131"),
        BOOK.replace("found context.102.133", "found context.102.132"),
        BOOK.replace(" max into", " successor context.102.132 into"),
        BOOK.replace(
            " max into",
            " range command.101.130 to command.101.131 into",
        ),
        format!(
            "{BOOK}\norder 180 duplicate of 110 min into context.102.132 found context.102.133;"
        ),
        BOOK.replace("order 180", "order 400"),
        BOOK.replace("0..=1000000", "9..=8"),
        format!(
            "{BOOK}\nlaw 505 observation_is_not_state on commit, genesis = context.102.133 == 0;"
        ),
    ] {
        assert!(
            parse_project(&source, Default::default()).is_err(),
            "accepted: {source}"
        );
    }
}

#[test]
fn ordered_state_laws_must_be_uniform_over_each_maps_named_entries() {
    let source = BOOK.replace(
        "forall e in entries 110 { post.100.e == 1 || post.100.e.140 == 0 }",
        "post.100.171 == 0",
    );
    let parsed = parse_project(&source, Default::default());
    assert!(
        parsed.is_err(),
        "query-selected invariant cannot justify universal entry induction"
    );
}
#[test]
fn order_read_count_is_bounded_even_with_distinct_outputs() {
    let mut source = BOOK.to_owned();
    for i in 0..4 {
        source += &format!(
            "field {} 102 result_{} 103;\nfield {} 102 found_{} 106;\norder {} extra_{} of 110 min into context.102.{} found context.102.{};\n",
            210 + i * 2,
            i,
            211 + i * 2,
            i,
            190 + i,
            i,
            210 + i * 2,
            211 + i * 2
        );
    }
    assert!(
        parse_project(&source, Default::default()).is_err(),
        "five reads accepted"
    );
}
