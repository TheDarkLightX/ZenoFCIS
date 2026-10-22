//! Deterministic stage/spawn seams; no global environment or timing races.
use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fcis-kbsnap-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        Self(path)
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Input {
        let target = self.0.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(target, bytes).unwrap_or_else(|e| panic!("{e}"));
        Input {
            path: path.into(),
            sha256: sha256_hex(bytes),
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn setup(d: &Directory) -> Recipe {
    let original = d.write(
        "data/original.json",
        br#"{"verdict":"equivalent","subject":"original"}"#,
    );
    let same = d.write(
        "data/same.json",
        br#"{"verdict":"equivalent","subject":"staged"}"#,
    );
    let changed = d.write(
        "data/changed.json",
        br#"{"verdict":"different","subject":"negative"}"#,
    );
    let mut r = super::tests::recipe();
    r.dependencies = vec![original.clone()];
    r.positive.arguments[3] = "data/original.json".into();
    r.positive.arguments[5] = "data/same.json".into();
    r.positive.inputs = vec![original.clone(), same];
    r.negative.arguments[3] = "data/original.json".into();
    r.negative.arguments[5] = "data/changed.json".into();
    r.negative.inputs = vec![original, changed];
    r
}
fn take(root: &Path, case: &Case, deps: &[Input]) -> Snapshot {
    Snapshot::new(root, case, deps).unwrap_or_else(|e| panic!("{e}"))
}
fn refused(result: Result<Snapshot, Value>, reason: &str) {
    match result {
        Ok(_) => panic!("expected {reason}"),
        Err(e) => assert_eq!(e["reason"], reason),
    }
}
fn checker(d: &Directory) -> PathBuf {
    // Emit the candidate bytes; a source edit changes observed content without
    // changing the asserted verdict, exactly as in the original finding.
    let script = d.0.join("checker");
    fs::write(&script, b"#!/bin/sh\ncat -- \"$6\"\n").unwrap_or_else(|e| panic!("{e}"));
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|e| panic!("{e}"));
    script
}
#[test]
fn kb_snapshot_root_edit_between_staging_and_spawn_cannot_change_receipt() {
    let d = Directory::new();
    let r = setup(&d);
    let executable = checker(&d);
    let positive = take(&d.0, &r.positive, &r.dependencies);
    let negative = take(&d.0, &r.negative, &r.dependencies);
    let digest = staged_dependencies(&positive, &negative, &r, Some(&r.checker_sha256))
        .unwrap_or_else(|e| panic!("{e}"));
    let source = d.0.join("data/same.json");
    let original = fs::read(&source).unwrap_or_else(|e| panic!("{e}"));
    let changed = br#"{"verdict":"equivalent","subject":"raced-source"}"#;
    let result = run_case(&positive, &r.positive, &executable, || {
        // This closure is the deterministic pause after staging and before spawn.
        fs::write(&source, changed).unwrap_or_else(|e| panic!("{e}"));
    })
    .unwrap_or_else(|e| panic!("{e}"));
    // Restore before the independent final live dependency check.
    fs::write(&source, &original).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        dependencies(&d.0, &r, Some(&r.checker_sha256)).unwrap_or_default(),
        digest
    );
    let negative_result =
        run_case(&negative, &r.negative, &executable, || {}).unwrap_or_else(|e| panic!("{e}"));
    let recipe_bytes = serde_json::to_vec(&r).unwrap_or_default();
    d.write("recipe.json", &recipe_bytes);
    let receipt = Receipt {
        schema: "zeno-fcis/kb-validation/1".into(),
        recipe: Input {
            path: "recipe.json".into(),
            sha256: sha256_hex(&recipe_bytes),
        },
        dependency_sha256: digest,
        last_validation: 1,
        positive: result,
        negative: negative_result,
    };
    let input = d.write(
        "receipt.json",
        &serde_json::to_vec(&receipt).unwrap_or_default(),
    );
    assert!(load(&d.0, &input, Some(&r.checker_sha256)).is_ok());
    assert_eq!(receipt.positive.output["subject"], "staged");
    let staged_path = positive.root().to_owned();
    assert_eq!(
        fs::metadata(&staged_path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or_default(),
        0o700
    );
    drop(positive);
    assert!(!staged_path.exists());
}
#[test]
fn kb_snapshot_mismatched_copied_input_refuses() {
    let d = Directory::new();
    let r = setup(&d);
    d.write("data/same.json", b"different bytes before copy");
    refused(
        Snapshot::new(&d.0, &r.positive, &r.dependencies),
        "kb-evidence-stale",
    );
}
#[test]
fn kb_snapshot_absolute_parent_and_symlink_paths_refuse() {
    let d = Directory::new();
    let r = setup(&d);
    for bad in [d.0.join("data/same.json"), PathBuf::from("../escape")] {
        let mut c = r.positive.clone();
        c.inputs[1].path = bad;
        refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-path");
    }
    symlink("same.json", d.0.join("data/link.json")).unwrap_or_else(|e| panic!("{e}"));
    let mut c = r.positive.clone();
    c.inputs[1].path = "data/link.json".into();
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-source");
    symlink("data", d.0.join("alias")).unwrap_or_else(|e| panic!("{e}"));
    c.inputs[1].path = "alias/same.json".into();
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-source");
    // Subject argv paths are independently confined, even with valid inputs.
    c = r.positive.clone();
    c.arguments[5] = d.0.join("data/same.json").to_string_lossy().into_owned();
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-path");
}
fn contract(d: &Directory) -> Case {
    let project = d.write("app/project.zeno", b"synthetic project");
    let policy = d.write("app/v2/policy.json", b"{}");
    Case {
        arguments: vec!["contract".into(), "review".into(), "app".into()],
        inputs: vec![project, policy],
        expected_exit: 0,
        assertion: Assertion {
            pointer: "/result".into(),
            equals: json!(true),
        },
    }
}
#[test]
fn kb_snapshot_full_contract_binds_undeclared_files_and_empty_directories() {
    let d = Directory::new();
    let c = contract(&d);
    d.write("app/extra/deep.txt", b"undeclared");
    fs::create_dir(d.0.join("app/empty")).unwrap_or_else(|e| panic!("{e}"));
    let first = take(&d.0, &c, &[]);
    assert_eq!(
        fs::read(first.root().join("app/extra/deep.txt")).unwrap_or_default(),
        b"undeclared"
    );
    assert!(first.root().join("app/empty").is_dir());
    d.write("app/extra/deep.txt", b"changed");
    let second = take(&d.0, &c, &[]);
    assert_ne!(first.manifest(), second.manifest());
    symlink("deep.txt", d.0.join("app/extra/link")).unwrap_or_else(|e| panic!("{e}"));
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-source");
}
#[test]
fn kb_snapshot_byte_and_entry_limits_refuse() {
    let d = Directory::new();
    let c = contract(&d);
    let large = fs::File::create(d.0.join("app/large")).unwrap_or_else(|e| panic!("{e}"));
    large
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap_or_else(|e| panic!("{e}"));
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-bound");
    fs::remove_file(d.0.join("app/large")).unwrap_or_else(|e| panic!("{e}"));
    // The total limit also covers several individually admissible files.
    for name in ["first", "second"] {
        let file = fs::File::create(d.0.join("app").join(name)).unwrap_or_else(|e| panic!("{e}"));
        file.set_len(32 * 1024 * 1024 + 1)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-bound");
    for name in ["first", "second"] {
        fs::remove_file(d.0.join("app").join(name)).unwrap_or_else(|e| panic!("{e}"));
    }
    for i in 0..4096 {
        d.write(&format!("app/files/{i}"), b"");
    }
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-bound");
}
#[test]
fn kb_snapshot_subject_binding_and_declared_input_limit_remain_required() {
    let d = Directory::new();
    let mut r = setup(&d);
    r.positive.inputs.pop();
    refused(
        Snapshot::new(&d.0, &r.positive, &[]),
        "kb-evidence-subject-unbound",
    );
    let mut c = contract(&d);
    c.inputs[1] = d.write("app/v2/policy.json", &vec![b' '; 1_048_577]);
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-input-bound");
}

#[test]
fn kb_snapshot_loaded_receipt_becomes_stale_when_undeclared_contract_file_changes() {
    let d = Directory::new();
    let mut r = setup(&d);
    r.positive = contract(&d);
    r.negative = r.positive.clone();
    r.negative.arguments.push("--gaps".into());
    r.negative.assertion.equals = json!(false);
    d.write("app/undeclared.txt", b"bound by complete subject");
    let digest = dependencies(&d.0, &r, Some(&r.checker_sha256)).unwrap_or_else(|e| panic!("{e}"));
    let output = |value: bool| {
        let output = json!({"result":value});
        CaseResult {
            exit: 0,
            output_sha256: sha256_hex(&serde_json::to_vec(&output).unwrap_or_default()),
            output,
        }
    };
    let recipe = d.write("recipe.json", &serde_json::to_vec(&r).unwrap_or_default());
    let receipt = Receipt {
        schema: "zeno-fcis/kb-validation/1".into(),
        recipe,
        dependency_sha256: digest,
        last_validation: 1,
        positive: output(true),
        negative: output(false),
    };
    let receipt = d.write(
        "receipt.json",
        &serde_json::to_vec(&receipt).unwrap_or_default(),
    );
    assert!(load(&d.0, &receipt, Some(&r.checker_sha256)).is_ok());
    d.write("app/undeclared.txt", b"changed after replay");
    assert_eq!(
        load(&d.0, &receipt, Some(&r.checker_sha256))
            .err()
            .map(|e| e["reason"].clone()),
        Some(json!("kb-evidence-replay-mismatch"))
    );
}

// Reviewer witnesses retained as named regressions.
#[test]
fn astra_implicit_parent_directories_bypass_entry_bound() {
    let d = Directory::new();
    let c = contract(&d);
    let dependency = d.write("dep/a/b/evidence", b"dependency");
    // Original witness: 4,096 manifest keys hid three uncounted parents.
    for i in 0..4091 {
        fs::create_dir(d.0.join(format!("app/empty-{i}"))).unwrap_or_else(|e| panic!("{e}"));
    }
    // The extra dep/a/b parents must count, even though they are not
    // independently declared inputs or recursively visited subject entries.
    refused(
        Snapshot::new(&d.0, &c, std::slice::from_ref(&dependency)),
        "kb-evidence-snapshot-bound",
    );
    // Equality is admissible: 4 contract entries + 4 dependency entries + 4088.
    for i in 4088..4091 {
        fs::remove_dir(d.0.join(format!("app/empty-{i}"))).unwrap_or_else(|e| panic!("{e}"));
    }
    let staged = take(&d.0, &c, &[dependency]);
    fn count(path: &Path) -> usize {
        fs::read_dir(path)
            .unwrap_or_else(|e| panic!("{e}"))
            .map(|e| {
                let p = e.unwrap_or_else(|e| panic!("{e}")).path();
                1 + if p.is_dir() { count(&p) } else { 0 }
            })
            .sum()
    }
    assert_eq!(staged.manifest().len(), 4096);
    assert_eq!(count(staged.root()), 4096);
    for parent in ["dep", "dep/a", "dep/a/b"] {
        assert_eq!(staged.manifest().get(parent), Some(&None));
    }
}

#[test]
fn astra_hardlink_and_intermediate_directory_swap_after_copy_are_isolated() {
    let d = Directory::new();
    let r = setup(&d);
    fs::hard_link(d.0.join("data/same.json"), d.0.join("alias")).unwrap_or_else(|e| panic!("{e}"));
    let staged = take(&d.0, &r.positive, &r.dependencies);
    let executable = checker(&d);
    let result = run_case(&staged, &r.positive, &executable, || {
        fs::rename(d.0.join("data"), d.0.join("renamed")).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(d.0.join("replacement")).unwrap_or_else(|e| panic!("{e}"));
        fs::write(
            d.0.join("replacement/same.json"),
            br#"{"verdict":"equivalent","subject":"replacement"}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        symlink("replacement", d.0.join("data")).unwrap_or_else(|e| panic!("{e}"));
        fs::write(
            d.0.join("alias"),
            br#"{"verdict":"equivalent","subject":"hardlink-mutated"}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(result.output["subject"], "staged");
}

#[test]
fn astra_non_utf8_and_depth_boundaries() {
    use std::os::unix::ffi::OsStrExt;
    let d = Directory::new();
    let c = contract(&d);
    let relative = Path::new("app").join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
    let non_utf8 = d.0.join(&relative);
    match fs::write(&non_utf8, b"data") {
        Ok(()) => {
            refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-path");
            fs::remove_file(non_utf8).unwrap_or_else(|e| panic!("{e}"));
        }
        // APFS rejects this fixture name before snapshot traversal can see it.
        // The same declared input must still refuse, at the source-open boundary.
        #[cfg(target_os = "macos")]
        Err(e) if e.raw_os_error() == Some(nix::libc::EILSEQ) => {
            let input = Input {
                path: relative,
                sha256: sha256_hex(b"data"),
            };
            refused(
                Snapshot::new(&d.0, &c, &[input]),
                "kb-evidence-snapshot-source",
            );
        }
        Err(e) => panic!("{e}"),
    }
    let nested = format!(
        "app/{}",
        std::iter::repeat_n("d", 126).collect::<Vec<_>>().join("/")
    );
    d.write(&format!("{nested}/at-depth-128"), b"ok");
    let staged = take(&d.0, &c, &[]);
    assert!(
        staged
            .root()
            .join(format!("{nested}/at-depth-128"))
            .exists()
    );
    d.write(&format!("{nested}/extra/at-depth-129"), b"refuse");
    refused(Snapshot::new(&d.0, &c, &[]), "kb-evidence-snapshot-bound");
}

#[test]
fn astra_old_dependency_identity_cannot_load_as_current() {
    let d = Directory::new();
    let r = setup(&d);
    let executable = checker(&d);
    let positive = take(&d.0, &r.positive, &r.dependencies);
    let negative = take(&d.0, &r.negative, &r.dependencies);
    let mut old: Vec<(String, String)> = r
        .dependencies
        .iter()
        .chain(&r.positive.inputs)
        .chain(&r.negative.inputs)
        .map(|i| (i.path.to_string_lossy().into_owned(), i.sha256.clone()))
        .collect();
    old.push(("checker".into(), r.checker_sha256.clone()));
    old.sort();
    old.dedup();
    let old_digest = sha256_hex(&serde_json::to_vec(&old).unwrap_or_else(|e| panic!("{e}")));
    let recipe = d.write(
        "recipe.json",
        &serde_json::to_vec(&r).unwrap_or_else(|e| panic!("{e}")),
    );
    let receipt = Receipt {
        schema: "zeno-fcis/kb-validation/1".into(),
        recipe,
        dependency_sha256: old_digest,
        last_validation: 1,
        positive: run_case(&positive, &r.positive, &executable, || {})
            .unwrap_or_else(|e| panic!("{e}")),
        negative: run_case(&negative, &r.negative, &executable, || {})
            .unwrap_or_else(|e| panic!("{e}")),
    };
    let input = d.write(
        "receipt.json",
        &serde_json::to_vec(&receipt).unwrap_or_else(|e| panic!("{e}")),
    );
    assert_eq!(
        load(&d.0, &input, Some(&r.checker_sha256))
            .err()
            .unwrap_or_else(|| panic!("expected refusal"))["reason"],
        "kb-evidence-replay-mismatch"
    );
}

#[test]
fn astra_receipt_inside_subject_invalidates_its_own_identity() {
    let d = Directory::new();
    let mut r = setup(&d);
    r.positive = contract(&d);
    r.negative = r.positive.clone();
    r.negative.arguments.push("--gaps".into());
    r.negative.assertion.equals = json!(false);
    let digest = dependencies(&d.0, &r, Some(&r.checker_sha256)).unwrap_or_else(|e| panic!("{e}"));
    let output = |value: bool| {
        let output = json!({"result":value});
        CaseResult {
            exit: 0,
            output_sha256: sha256_hex(
                &serde_json::to_vec(&output).unwrap_or_else(|e| panic!("{e}")),
            ),
            output,
        }
    };
    let mut recipe = d.write(
        "recipe.json",
        &serde_json::to_vec(&r).unwrap_or_else(|e| panic!("{e}")),
    );
    recipe.path = d.0.join("recipe.json");
    let receipt = Receipt {
        schema: "zeno-fcis/kb-validation/1".into(),
        recipe,
        dependency_sha256: digest,
        last_validation: 1,
        positive: output(true),
        negative: output(false),
    };
    let encoded = serde_json::to_vec(&receipt).unwrap_or_else(|e| panic!("{e}"));
    let outside = d.write("outside.json", &encoded);
    assert!(load(&d.0, &outside, Some(&r.checker_sha256)).is_ok());
    for destination in ["app/validation.json", "app/v2/validation.json"] {
        let out = d.0.join(destination);
        let (exit, result) = replay(&d.0.join("recipe.json"), &out);
        assert_eq!(exit, 1);
        assert_eq!(result["reason"], "kb-evidence-output-subject");
        assert!(!out.exists());
    }
    let inside = d.write("app/validation.json", &encoded);
    assert_eq!(
        load(&d.0, &inside, Some(&r.checker_sha256))
            .err()
            .unwrap_or_else(|| panic!("expected refusal"))["reason"],
        "kb-evidence-replay-mismatch"
    );
    eprintln!(
        "same receipt: outside=Validated; created inside subject=kb-evidence-replay-mismatch"
    );
}

// ---- Re-review 2 regression witnesses ----
fn rr2_count(path: &Path) -> usize {
    fs::read_dir(path)
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|e| {
            let p = e.unwrap_or_else(|e| panic!("{e}")).path();
            1 + if p.is_dir() { rr2_count(&p) } else { 0 }
        })
        .sum()
}
#[test]
fn rr2_bound_exact_4096_pass_4097_refuse_with_odd_shaped_shared_parents() {
    let d = Directory::new();
    let c = contract(&d); // app, app/project.zeno, app/v2, app/v2/policy.json
    let first = d.write("./dep//a/./b/evidence", b"one"); // dep, dep/a, dep/a/b, file
    let second = d.write("dep/a/c/other", b"two"); // dep/a/c, file (dep, dep/a shared)
    let deps = [first, second];
    for i in 0..4087 {
        fs::create_dir(d.0.join(format!("app/empty-{i}"))).unwrap_or_else(|e| panic!("{e}"));
    }
    // 4 + 4 + 2 + 4087 = 4097 physical entries.
    refused(Snapshot::new(&d.0, &c, &deps), "kb-evidence-snapshot-bound");
    fs::remove_dir(d.0.join("app/empty-4086")).unwrap_or_else(|e| panic!("{e}"));
    let staged = take(&d.0, &c, &deps);
    assert_eq!(staged.manifest().len(), 4096);
    assert_eq!(rr2_count(staged.root()), 4096);
    for parent in ["dep", "dep/a", "dep/a/b", "dep/a/c"] {
        assert_eq!(staged.manifest().get(parent), Some(&None), "{parent}");
    }
    eprintln!("rr2 bound: 4097 refused; 4096 admitted with manifest=physical=4096");
}
#[test]
fn rr2_parent_precreated_by_dependency_is_still_traversed() {
    let d = Directory::new();
    let mut c = contract(&d);
    c.inputs[1].path = "app//v2/./policy.json".into();
    let dep = d.write("app/x/y/dep.txt", b"dep");
    d.write("app/x/sibling.txt", b"sibling");
    d.write("app/x/y/z/deep.txt", b"deep");
    fs::create_dir(d.0.join("app/x/empty")).unwrap_or_else(|e| panic!("{e}"));
    let staged = take(&d.0, &c, &[dep]);
    let root = staged.root();
    assert_eq!(
        fs::read(root.join("app/x/sibling.txt")).unwrap_or_default(),
        b"sibling"
    );
    assert_eq!(
        fs::read(root.join("app/x/y/z/deep.txt")).unwrap_or_default(),
        b"deep"
    );
    assert!(root.join("app/x/empty").is_dir());
    assert_eq!(staged.manifest().len(), rr2_count(root));
    assert!(matches!(
        staged.manifest().get("app/x/y/dep.txt"),
        Some(Some(_))
    ));
    eprintln!(
        "rr2 traversal: {} entries, manifest=physical",
        staged.manifest().len()
    );
}
#[test]
fn rr2_transform_deep_parents_counted() {
    let d = Directory::new();
    let mut r = setup(&d);
    let deep = d.write(
        "deep/p/q/same.json",
        br#"{"verdict":"equivalent","subject":"deep"}"#,
    );
    r.positive.arguments[5] = "deep/p/q/same.json".into();
    r.positive.inputs[1] = deep;
    let staged = take(&d.0, &r.positive, &r.dependencies);
    assert_eq!(staged.manifest().len(), rr2_count(staged.root()));
    for parent in ["deep", "deep/p", "deep/p/q", "data"] {
        assert_eq!(staged.manifest().get(parent), Some(&None), "{parent}");
    }
    eprintln!(
        "rr2 transform: {} entries, manifest=physical",
        staged.manifest().len()
    );
}
#[test]
fn rr2_out_aliases_inside_subject_refuse_before_replay() {
    let d = Directory::new();
    let mut r = setup(&d);
    r.positive = contract(&d);
    r.negative = r.positive.clone();
    r.negative.arguments.push("--gaps".into());
    r.negative.assertion.equals = json!(false);
    d.write("recipe.json", &serde_json::to_vec(&r).unwrap_or_default());
    let mut odd = r.clone();
    odd.positive.arguments[2] = "./app/".into();
    odd.negative.arguments[2] = "./app/".into();
    d.write(
        "recipe-odd.json",
        &serde_json::to_vec(&odd).unwrap_or_default(),
    );
    fs::create_dir(d.0.join("outside")).unwrap_or_else(|e| panic!("{e}"));
    fs::create_dir(d.0.join("receipts")).unwrap_or_else(|e| panic!("{e}"));
    symlink("../app/v2", d.0.join("outside/link")).unwrap_or_else(|e| panic!("{e}"));
    symlink("app", d.0.join("applink")).unwrap_or_else(|e| panic!("{e}"));
    symlink("l2", d.0.join("outside/l1")).unwrap_or_else(|e| panic!("{e}"));
    symlink("../applink", d.0.join("outside/l2")).unwrap_or_else(|e| panic!("{e}"));
    let cases = [
        (
            "recipe.json",
            "outside/link/validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe.json",
            "receipts/../app/validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe.json",
            "receipts/../app/v2/../validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe.json",
            "applink/v2/validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe.json",
            "outside/l1/validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe.json",
            "app/newdir/validation.json",
            "kb-evidence-output-parent",
        ),
        (
            "recipe.json",
            "app/v2/newdir/../validation.json",
            "kb-evidence-output-parent",
        ),
        (
            "recipe-odd.json",
            "app/validation.json",
            "kb-evidence-output-subject",
        ),
        (
            "recipe-odd.json",
            "outside/link/validation.json",
            "kb-evidence-output-subject",
        ),
    ];
    for (recipe, destination, reason) in cases {
        let out = d.0.join(destination);
        let (exit, result) = replay(&d.0.join(recipe), &out);
        eprintln!(
            "rr2 out {recipe} {destination}: exit={exit} reason={}",
            result["reason"]
        );
        assert_eq!(exit, 1, "{destination}");
        assert_eq!(result["reason"], reason, "{destination}");
        assert!(!d.0.join("app/validation.json").exists());
        assert!(!d.0.join("app/v2/validation.json").exists());
        assert!(!d.0.join("app/newdir").exists());
    }
}
