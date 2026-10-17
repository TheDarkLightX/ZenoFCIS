//! The checker's identity covers every file of this crate, and the library
//! reaches no ambient effect and decides on no rendering.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::identity::{CHECKER_SOURCES, Identity};

/// Compiled only for tests, or written by `cargo package`; none of them is
/// part of the library.
const REVIEWED_FILES: &[&str] = &[
    ".cargo_vcs_info.json",
    "Cargo.toml.orig",
    "src/checked_composition/assembly_differential_tests.rs",
    "src/checked_composition/assembly_tests.rs",
    "src/checked_composition/compose_tests.rs",
    "src/checked_composition/domain_tests.rs",
    "src/checked_composition/hierarchy_tests.rs",
    "src/checked_composition/identity_tests.rs",
    "src/checked_composition/joint_tests.rs",
    "src/checked_composition/journal_tests.rs",
    "src/checked_composition/obligations_tests.rs",
    "src/checked_composition/receipt_tests.rs",
    "src/checked_composition/structure_tests.rs",
    "src/checked_composition/term_tests.rs",
    "src/checked_composition/tests.rs",
];
/// Integration tests and their fixtures.
const REVIEWED_PREFIXES: &[&str] = &["tests/"];

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
        CHECKER_SOURCES.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "the hashed files are sorted and unique"
    );
    for (path, bytes) in CHECKER_SOURCES {
        assert_eq!(
            fs::read(root.join(path)).ok().as_deref(),
            Some(*bytes),
            "{path} is stale"
        );
    }
    let hashed: BTreeSet<String> = CHECKER_SOURCES
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    for path in REVIEWED_FILES {
        assert!(
            !hashed.contains(*path),
            "{path} is both hashed and reviewed out"
        );
    }
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
fn the_identity_binds_each_part_by_value() {
    let identity = Identity::current();
    assert_eq!(identity, Identity::current());
    let json = identity.json();
    let sources = &json["sources"];
    assert_eq!(
        sources["spec"],
        super::hex(&zeno_fcis_spec::source_digest())
    );
    assert_eq!(
        sources["compose"],
        super::hex(&zeno_fcis_compose::source_digest())
    );
    assert_eq!(
        sources["evaluator"],
        super::hex(&zeno_fcis_synthesis::finite::v2_authority::EVALUATOR)
    );
    assert_eq!(json["source_digest"], super::hex(&identity.digest()));
}

/// The names a library file must not reach: the file system, processes,
/// the environment, the network, threads and the clock.
const AMBIENT: &[&str] = &["fs", "process", "env", "net", "thread", "time"];

/// Every ambient `std` module a source names, as `std::m` or in a
/// `use std::{...}` group, with whitespace ignored.
fn ambient(text: &str) -> Vec<String> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut found = Vec::new();
    for (at, _) in compact.match_indices("std::") {
        let rest = &compact[at + "std::".len()..];
        let named: Vec<&str> = if let Some(group) = rest.strip_prefix('{') {
            let group = &group[..group.find('}').unwrap_or(group.len())];
            group.split([',', '{', ':']).collect()
        } else {
            vec![rest]
        };
        for name in named {
            for module in AMBIENT {
                let follows = name.strip_prefix(module).is_some_and(|tail| {
                    !tail.starts_with(|c: char| c.is_alphanumeric() || c == '_')
                });
                if follows {
                    found.push(format!("std::{module}"));
                }
            }
        }
    }
    found
}

#[test]
fn the_ambient_gate_finds_each_form() {
    for text in [
        "use std::fs;",
        "let _ = std :: env :: var(\"X\");",
        "use std::{collections::BTreeMap, process};",
        "std::thread::spawn(f);",
        "use std::time::Instant;",
        "use std::net::TcpStream;",
    ] {
        assert!(!ambient(text).is_empty(), "{text}");
    }
    for text in [
        "use std::collections::BTreeSet;",
        "impl std::fmt::Display for X {}",
        "let fsm = 1; let envelope = 2; let timeout = 3;",
        "env!(\"CARGO_PKG_VERSION\")",
    ] {
        assert!(ambient(text).is_empty(), "{text}");
    }
}

#[test]
fn the_library_reaches_no_ambient_effect() {
    let mut found = Vec::new();
    for (path, bytes) in CHECKER_SOURCES {
        let text = std::str::from_utf8(bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
        found.extend(
            ambient(text)
                .into_iter()
                .map(|module| format!("{path}: {module}")),
        );
    }
    assert!(found.is_empty(), "{found:?}");
}

/// The checker's own files: its verdicts, tables and receipts.
fn checker_files() -> impl Iterator<Item = (&'static str, &'static str)> {
    CHECKER_SOURCES
        .iter()
        .filter(|(path, _)| path.starts_with("src/checked_composition"))
        .map(|(path, bytes)| {
            let text = std::str::from_utf8(bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
            (*path, text)
        })
}

#[test]
fn the_checker_formats_no_debug_rendering() {
    let mut found = Vec::new();
    for (path, text) in checker_files() {
        for (number, line) in text.lines().enumerate() {
            if line.contains(":?}") || line.contains(":#?}") {
                found.push(format!("{path}:{}", number + 1));
            }
        }
    }
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_checker_takes_only_the_framer_from_the_advisory_review() {
    // The review classifies some library refusals by their text; the
    // checker uses only its envelope framing.
    for (path, text) in checker_files() {
        for line in text
            .lines()
            .filter(|line| line.contains("review::evaluate"))
        {
            assert_eq!(
                line.trim(),
                "use crate::internals::review::evaluate::Framer;",
                "{path}"
            );
        }
        assert!(!text.contains("evaluate::evaluate"), "{path}");
        assert!(!text.contains("RefusalClass"), "{path}");
    }
}
