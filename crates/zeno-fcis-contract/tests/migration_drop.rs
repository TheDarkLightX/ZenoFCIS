//! Strict, versioned state-drop declarations at the migration input boundary.

use zeno_fcis_contract::internals::rules::read_migration;

#[test]
fn version_two_accepts_an_explicit_state_drop() {
    let source = r#"{"schema":"zeno-fcis/migration/2","state":{"121":{"from":121}},"drop":[124]}"#;
    let parsed = read_migration(source, "migration.json");
    assert!(parsed.is_ok(), "explicit drop was refused: {parsed:?}");
}

#[test]
fn dropped_ids_are_canonical_and_absence_keeps_existing_migrations() {
    let parsed = read_migration(
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":[124,120]}"#,
        "migration.json",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(parsed.dropped, [120, 124]);
    for schema in ["zeno-fcis/migration/1", "zeno-fcis/migration/2"] {
        let source = format!(r#"{{"schema":"{schema}","state":{{}}}}"#);
        let parsed =
            read_migration(&source, "migration.json").unwrap_or_else(|error| panic!("{error}"));
        assert!(parsed.dropped.is_empty());
    }
}

#[test]
fn state_drop_declarations_are_strict_and_versioned() {
    for source in [
        r#"{"schema":"zeno-fcis/migration/1","state":{},"drop":[]}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":[124,124]}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":[true]}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":[-1]}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":[65536]}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"drop":"124"}"#,
        r#"{"schema":"zeno-fcis/migration/2","state":{},"payloads":{"300":{"drop":[]}}}"#,
    ] {
        assert!(
            read_migration(source, "migration.json").is_err(),
            "accepted: {source}"
        );
    }
}
