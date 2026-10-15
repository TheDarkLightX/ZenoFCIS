//! Every file of this crate is in its source identity or reviewed out: a file
//! added to the library without being hashed fails here.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use zeno_fcis_compose::{SOURCE_FILES, source_digest};

/// Compiled only for tests, or written by `cargo package`; none of them is
/// part of the library.
const REVIEWED_PREFIXES: &[&str] = &["tests/"];
const REVIEWED_FILES: &[&str] = &[
    ".cargo_vcs_info.json",
    "Cargo.toml.orig",
    "src/composition_identity_tests.rs",
];

fn collect(root: &Path, directory: &Path, files: &mut BTreeSet<String>) {
    let entries = fs::read_dir(directory).unwrap_or_else(|error| panic!("{directory:?}: {error}"));
    for entry in entries {
        let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or_else(|error| panic!("{error}"));
            let relative = relative
                .to_str()
                .unwrap_or_else(|| panic!("{path:?} is not UTF-8"));
            files.insert(relative.replace('\\', "/"));
        }
    }
}

#[test]
fn every_crate_file_is_hashed_or_reviewed_out() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut tree = BTreeSet::new();
    collect(root, root, &mut tree);
    assert!(
        SOURCE_FILES.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "the hashed files are sorted and unique"
    );
    for (path, bytes) in SOURCE_FILES {
        assert_eq!(
            fs::read(root.join(path)).ok().as_deref(),
            Some(*bytes),
            "{path} is stale"
        );
    }
    let hashed: BTreeSet<String> = SOURCE_FILES
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    let unhashed: Vec<&String> = tree
        .iter()
        .filter(|path| !hashed.contains(*path))
        .filter(|path| !REVIEWED_FILES.contains(&path.as_str()))
        .filter(|path| {
            !REVIEWED_PREFIXES
                .iter()
                .any(|prefix| path.starts_with(prefix))
        })
        .collect();
    assert!(unhashed.is_empty(), "hash or review out: {unhashed:?}");
    let missing: Vec<&String> = hashed.difference(&tree).collect();
    assert!(missing.is_empty(), "listed but absent: {missing:?}");
}

#[test]
fn the_digest_is_deterministic_and_lists_the_library_and_the_manifest() {
    assert_eq!(source_digest(), source_digest());
    assert!(SOURCE_FILES.iter().any(|(path, _)| *path == "src/lib.rs"));
    assert!(SOURCE_FILES.iter().any(|(path, _)| *path == "Cargo.toml"));
}
