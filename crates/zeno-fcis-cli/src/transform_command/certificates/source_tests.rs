//! The source manifest is generated and checked against the parsed module graph.
use super::*;
use std::collections::BTreeSet;
use syn::parse::Parser as _;

fn requires_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            let nested = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .unwrap_or_else(|error| panic!("cfg: {error}"));
            if list.path.is_ident("all") {
                nested.iter().any(requires_test)
            } else {
                !nested.is_empty() && nested.iter().all(requires_test)
            }
        }
        _ => false,
    }
}

fn module_graph(path: &Path, directory: &Path, files: &mut BTreeSet<PathBuf>) {
    let path = path
        .canonicalize()
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    if !files.insert(path.clone()) {
        return;
    }
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}"));
    let parsed =
        syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    modules(&parsed.items, &path, directory, files);
}

fn modules(items: &[syn::Item], source: &Path, directory: &Path, files: &mut BTreeSet<PathBuf>) {
    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        if module.attrs.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|meta| requires_test(&meta))
        }) {
            continue;
        }
        let name = module.ident.to_string();
        if let Some((_, items)) = &module.content {
            modules(items, source, &directory.join(name), files);
            continue;
        }
        let explicit = module.attrs.iter().find_map(|attribute| {
            if !attribute.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(meta) = &attribute.meta else {
                panic!("unsupported module path");
            };
            let syn::Expr::Lit(literal) = &meta.value else {
                panic!("unsupported module path");
            };
            let syn::Lit::Str(value) = &literal.lit else {
                panic!("unsupported module path");
            };
            Some(
                source
                    .parent()
                    .unwrap_or_else(|| panic!("module parent"))
                    .join(value.value()),
            )
        });
        let path = explicit.unwrap_or_else(|| {
            let flat = directory.join(format!("{name}.rs"));
            let nested = directory.join(&name).join("mod.rs");
            assert_ne!(
                flat.is_file(),
                nested.is_file(),
                "one module source: {name}"
            );
            if flat.is_file() { flat } else { nested }
        });
        let next = if path.file_name().is_some_and(|name| name == "mod.rs") {
            path.parent()
                .unwrap_or_else(|| panic!("module parent"))
                .to_path_buf()
        } else {
            path.with_extension("")
        };
        module_graph(&path, &next, files);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|error| panic!("source root: {error}"))
}

fn expected() -> Value {
    let root = root();
    let harness = root.join("verification/verus/scalar_node_refinement.rs");
    let mut closure = BTreeSet::new();
    module_graph(
        &harness,
        harness.parent().unwrap_or_else(|| panic!("harness parent")),
        &mut closure,
    );
    let hash = |path: &Path| {
        transform::sha256_hex(
            &std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
        )
    };
    let closure: serde_json::Map<_, _> = closure
        .iter()
        .map(|path| {
            (
                path.strip_prefix(&root)
                    .unwrap_or_else(|error| panic!("source escapes root: {error}"))
                    .to_string_lossy()
                    .into_owned(),
                json!(hash(path)),
            )
        })
        .collect();
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("verification/verus/authority_v2_sources.json"))
            .unwrap_or_else(|error| panic!("runtime manifest: {error}")),
    )
    .unwrap_or_else(|error| panic!("runtime manifest: {error}"));
    let mut runtime = BTreeSet::new();
    for path in manifest["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("runtime paths"))
    {
        runtime.insert(
            path.as_str()
                .unwrap_or_else(|| panic!("runtime path"))
                .to_owned(),
        );
    }
    // The owned-program getters and native adapters supplement the direct harness.
    runtime.extend(
        [
            "crates/zeno-fcis-synthesis/src/finite/ir/program.rs",
            "crates/zeno-fcis-synthesis/src/finite/mod.rs",
            "crates/zeno-fcis-synthesis/src/finite_runtime.rs",
            "crates/zeno-fcis-cli/src/symbolic.rs",
            "crates/zeno-fcis-cli/src/symbolic/program.rs",
            "crates/zeno-fcis-cli/src/symbolic/smt.rs",
            "crates/zeno-fcis-cli/src/symbolic/equivalence.rs",
        ]
        .map(str::to_owned),
    );
    let runtime: serde_json::Map<_, _> = runtime
        .into_iter()
        .map(|path| (path.clone(), json!(hash(&root.join(path)))))
        .collect();
    json!({"harness_include_closure": closure, "runtime_sources": runtime})
}

#[test]
fn runtime_owned_program_source_list_matches_its_module_graph() {
    let root = root();
    let entry = root.join("crates/zeno-fcis-synthesis/src/finite/ir.rs");
    let mut files = BTreeSet::new();
    module_graph(&entry, &entry.with_extension(""), &mut files);
    let expected: BTreeSet<_> = files
        .into_iter()
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap_or_else(|error| panic!("runtime source: {error}"))
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("verification/verus/authority_v2_sources.json"))
            .unwrap_or_else(|error| panic!("runtime manifest: {error}")),
    )
    .unwrap_or_else(|error| panic!("runtime manifest: {error}"));
    let listed: BTreeSet<_> = manifest["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("runtime paths"))
        .iter()
        .map(|path| path.as_str().unwrap_or_else(|| panic!("runtime path")))
        .filter(|path| {
            *path == "crates/zeno-fcis-synthesis/src/finite/ir.rs"
                || path.starts_with("crates/zeno-fcis-synthesis/src/finite/ir/")
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(
        listed, expected,
        "registered evaluator omits or adds owned-program runtime sources"
    );
}

#[test]
fn receipt_source_list_equals_harness_closure_and_runtime_sources() {
    let expected = expected();
    let bound = refinement_sources();
    for key in ["harness_include_closure", "runtime_sources"] {
        assert_eq!(
            bound[key], expected[key],
            "incomplete or stale {key}; regenerate the manifest after final source edits"
        );
    }
    assert!(
        bound["harness_include_closure"]["crates/zeno-fcis-cli/src/symbolic/unsupported.rs"]
            .is_string()
    );
    assert!(
        bound["harness_include_closure"]["crates/zeno-fcis-synthesis/src/finite/ir/program.rs"]
            .is_string()
    );
}

#[test]
#[ignore = "explicit source-manifest generator; run after final source edits"]
fn regenerate_scalar_refinement_source_manifest() {
    let sources = expected();
    let mut output = String::from(
        "// Generated by certificates::source_tests::regenerate_scalar_refinement_source_manifest.\n",
    );
    for (key, name) in [
        ("harness_include_closure", "HARNESS_INCLUDE_SOURCES"),
        ("runtime_sources", "RUNTIME_REFINEMENT_SOURCES"),
    ] {
        output.push_str(&format!("pub(super) const {name}: &[(&str, &str)] = &[\n"));
        for (path, hash) in sources[key]
            .as_object()
            .unwrap_or_else(|| panic!("source map"))
        {
            output.push_str(&format!(
                "    ({path:?}, {:?}),\n",
                hash.as_str().unwrap_or_else(|| panic!("source hash"))
            ));
        }
        output.push_str("];\n");
    }
    let destination = root()
        .join("crates/zeno-fcis-cli/src/transform_command/certificates/refinement_sources.rs");
    std::fs::write(&destination, output).unwrap_or_else(|error| panic!("manifest write: {error}"));
    let formatted = std::process::Command::new("rustfmt")
        .args(["+1.97.1", "--edition", "2024"])
        .arg(&destination)
        .status()
        .unwrap_or_else(|error| panic!("format source manifest: {error}"));
    assert!(formatted.success());
    eprintln!("generated source manifest: {destination:?}");
}
