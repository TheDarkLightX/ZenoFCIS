//! Workspace guard for source text embedded by the published CLI package.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::{Component, Path};

#[test]
fn compiled_source_snapshots_equal_the_workspace_originals_byte_for_byte() {
    let cli = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../source-snapshots.json"))
            .unwrap_or_else(|error| panic!("source snapshot manifest: {error}"));
    assert_eq!(manifest["format"], "zeno-fcis/cli-source-snapshots/1");
    let sources = manifest["sources"]
        .as_array()
        .unwrap_or_else(|| panic!("source snapshot list"));
    assert!(!sources.is_empty());
    let mut seen = BTreeSet::new();
    for source in sources {
        let name = source.as_str().unwrap_or_else(|| panic!("snapshot name"));
        assert!(seen.insert(name));
        assert!(
            Path::new(name)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        );
        let original = cli.join("..").join(name);
        let snapshot = cli.join("source-snapshots").join(format!("{name}.txt"));
        assert_eq!(
            std::fs::read(&snapshot)
                .unwrap_or_else(|error| panic!("{}: {error}", snapshot.display())),
            std::fs::read(&original)
                .unwrap_or_else(|error| panic!("{}: {error}", original.display())),
            "source snapshot differs: {name}",
        );
    }
}
