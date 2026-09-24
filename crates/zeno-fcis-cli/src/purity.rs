//! Static check for ambient effects and nondeterminism in Rust decision code.
//!
//! Each file is parsed with `syn`. `use` and `extern crate` aliases are
//! resolved within the file, and every path, macro, item, and literal is
//! compared with a fixed rule table. Paths inside macro invocations are found
//! by scanning the macro's tokens. A directory containing `Cargo.toml` is
//! checked as a crate: every `.rs` file under `src/`, plus the structural
//! conditions that confine it (unconditional `no_std`, `forbid(unsafe_code)`,
//! no `extern crate std`, no source brought in from outside the checked
//! files, and a manifest, read completely, whose dependencies are all named
//! as the library's semantic crates).
//!
//! The check reports what it can see, and reports as unreadable what it could
//! not read, so an unread file never leaves a result clean. It does not
//! expand macros, follow calls into other files or dependencies, or know the
//! type of a method's receiver. A clean or confined result is Checked against
//! this rule table; it is never a proof of determinism.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, Spacing, Span, TokenStream, TokenTree};
use serde_json::{Value, json};
use syn::ext::IdentExt;
use syn::visit::{self, Visit};

pub(crate) const SCHEMA: &str = "zeno-fcis/purity-report/1";

/// Paths that reach an ambient effect or a source of nondeterminism, by rule.
/// A path matches a prefix when it equals it or continues it by whole
/// segments. `core::` and `alloc::` paths are compared as `std::` paths.
const PATH_RULES: &[(&str, &[&str])] = &[
    (
        "clock",
        &[
            "std::time",
            "chrono::Utc",
            "chrono::Local",
            "chrono::offset::Utc",
            "chrono::offset::Local",
            "time::OffsetDateTime::now_utc",
            "time::OffsetDateTime::now_local",
            "time::Instant",
            "quanta",
            "coarsetime",
        ],
    ),
    ("environment", &["std::env"]),
    ("filesystem", &["std::fs"]),
    ("io", &["std::io", "std::os"]),
    ("network", &["std::net"]),
    ("process", &["std::process"]),
    (
        "threads",
        &[
            "std::thread",
            "std::sync::mpsc",
            "std::sync::Barrier",
            "std::sync::Condvar",
            "rayon",
            "crossbeam",
            "tokio",
            "async_std",
            "futures",
        ],
    ),
    (
        "randomness",
        &[
            "rand",
            "getrandom",
            "fastrand",
            "oorandom",
            "std::hash::RandomState",
            "std::collections::hash_map::RandomState",
            "uuid::Uuid::new_v4",
            "uuid::Uuid::now_v7",
            "ahash::RandomState",
        ],
    ),
    (
        "hash-order",
        &[
            "std::collections::HashMap",
            "std::collections::HashSet",
            "std::collections::hash_map",
            "std::collections::hash_set",
            "hashbrown",
            "ahash::AHashMap",
            "ahash::AHashSet",
        ],
    ),
    (
        "shared-state",
        &[
            "std::sync::atomic",
            "std::sync::Mutex",
            "std::sync::RwLock",
            "std::sync::OnceLock",
            "std::sync::LazyLock",
            "std::sync::Once",
            "std::cell",
            "once_cell",
            "lazy_static",
            "parking_lot",
            "spin",
        ],
    ),
    ("address", &["std::ptr"]),
    ("foreign-code", &["libc", "std::ffi"]),
];

/// Paths whose results may change between toolchains or platforms.
const WARNING_PATH_RULES: &[(&str, &[&str])] = &[
    (
        "unstable-hash",
        &[
            "std::hash::DefaultHasher",
            "std::collections::hash_map::DefaultHasher",
        ],
    ),
    ("floating-point", &["f32", "f64"]),
];

/// Macros that perform an effect or keep state, by rule.
const MACRO_RULES: &[(&str, &[&str])] = &[
    ("io", &["print", "println", "eprint", "eprintln", "dbg"]),
    ("shared-state", &["thread_local", "lazy_static"]),
    ("unsafe", &["asm", "global_asm", "naked_asm"]),
];

/// Methods that expose an address, matched by name.
const ADDRESS_METHODS: &[&str] = &["addr", "expose_provenance", "expose_addr"];

/// Dependencies a confined decision crate may use: the library's semantic
/// crates, which `tools/check_assurance.py` checks for ambient effects.
pub(crate) const ALLOWED_DEPENDENCIES: &[&str] = &[
    "zeno-fcis-core",
    "zeno-fcis-value",
    "zeno-fcis-codec",
    "zeno-fcis-spec",
    "zeno-fcis-crypto",
    "zeno-fcis-schema",
    "zeno-fcis-project",
    "zeno-fcis-catalog",
    "zeno-fcis-transition",
    "zeno-fcis-laws",
    "zeno-fcis-authority",
    "zeno-fcis-security",
    "zeno-fcis-secret",
    "zeno-fcis-patch",
    "zeno-fcis-plan",
    "zeno-fcis-receipt",
    "zeno-fcis-shell",
    "zeno-fcis-compose",
    "zeno-fcis-domain",
    "zeno-fcis-composed-program",
    "zeno-fcis-refine",
    "zeno-fcis-profile-zenodex",
    "zeno-fcis-evidence",
    "zeno-fcis-authenticated",
    "zeno-fcis-authenticated-authority",
    "zeno-fcis-synthesis",
    "zeno-fcis-backend",
];

const NONCLAIMS: &[&str] = &[
    "a clean or confined result is checked against this rule table, not a proof of determinism",
    "macros are not expanded; paths inside macro invocations are found by scanning tokens",
    "code in files not listed, in generated sources, and in dependencies is not read",
    "in a crate, only src/ is read; build scripts, tests, examples, and benches are not",
    "a confined crate's dependencies are identified by name; their sources (registry, path, or git) are not checked",
    "method calls are matched by name only for addr, expose_provenance, and expose_addr",
    "a function converted to an integer, as in `decide as usize`, is not reported",
    "platform-dependent sizes such as usize and size_of are not reported",
];

/// How much one finding matters.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Severity {
    /// Makes the result `violations`.
    Error,
    /// Reported, but the result can still be clean or confined.
    Warning,
}

impl Severity {
    const fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// One matched rule at one source location.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Finding {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) rule: &'static str,
    pub(crate) severity: Severity,
    pub(crate) subject: String,
}

/// Structural conditions of one crate.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Structure {
    pub(crate) krate: String,
    pub(crate) no_std: bool,
    pub(crate) forbid_unsafe: bool,
    pub(crate) std_reentry: bool,
    pub(crate) unreviewed_dependencies: Vec<String>,
    /// Manifest lines whose effect on the dependency set was not established.
    pub(crate) unrecognized_manifest: Vec<String>,
    /// Places that bring in source the check does not read.
    pub(crate) external_sources: Vec<String>,
    /// Binary roots Cargo would build, which confinement does not cover.
    pub(crate) binary_targets: Vec<String>,
}

impl Structure {
    fn confined(&self) -> bool {
        self.no_std
            && self.forbid_unsafe
            && !self.std_reentry
            && self.unreviewed_dependencies.is_empty()
            && self.unrecognized_manifest.is_empty()
            && self.external_sources.is_empty()
            && self.binary_targets.is_empty()
    }
}

/// Complete result of one invocation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Report {
    pub(crate) files: Vec<String>,
    pub(crate) findings: Vec<Finding>,
    pub(crate) structures: Vec<Structure>,
    pub(crate) unreadable: Vec<(String, String)>,
}

impl Report {
    pub(crate) fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    pub(crate) fn status(&self) -> &'static str {
        if !self.unreadable.is_empty() {
            "unreadable"
        } else if self.errors() > 0 {
            "violations"
        } else if !self.structures.is_empty() && self.structures.iter().all(Structure::confined) {
            "confined"
        } else {
            "clean"
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let findings: Vec<Value> = self
            .findings
            .iter()
            .map(|finding| {
                json!({
                    "file": finding.file, "line": finding.line, "column": finding.column,
                    "rule": finding.rule, "severity": finding.severity.name(),
                    "subject": finding.subject,
                })
            })
            .collect();
        let structures: Vec<Value> = self
            .structures
            .iter()
            .map(|structure| {
                json!({
                    "crate": structure.krate, "no_std": structure.no_std,
                    "forbid_unsafe": structure.forbid_unsafe,
                    "std_reentry": structure.std_reentry,
                    "unreviewed_dependencies": structure.unreviewed_dependencies,
                    "unrecognized_manifest": structure.unrecognized_manifest,
                    "external_sources": structure.external_sources,
                    "binary_targets": structure.binary_targets,
                    "confined": structure.confined(),
                })
            })
            .collect();
        let unreadable: Vec<Value> = self
            .unreadable
            .iter()
            .map(|(file, message)| json!({"file": file, "message": message}))
            .collect();
        json!({
            "schema": SCHEMA,
            "status": self.status(),
            "files_checked": self.files.len(),
            "errors": self.errors(),
            "warnings": self.findings.len() - self.errors(),
            "findings": findings,
            "crates": structures,
            "unreadable": unreadable,
            "nonclaims": NONCLAIMS,
        })
    }

    pub(crate) fn render(&self) -> String {
        let mut output = String::new();
        for (file, message) in &self.unreadable {
            output.push_str(&format!("{file}: unreadable: {message}\n"));
        }
        for finding in &self.findings {
            output.push_str(&format!(
                "{}:{}:{}: {}[{}]: {}\n",
                finding.file,
                finding.line,
                finding.column,
                finding.severity.name(),
                finding.rule,
                finding.subject
            ));
        }
        for structure in &self.structures {
            let mut unmet = Vec::new();
            if !structure.no_std {
                unmet.push("no unconditional #![no_std]".to_string());
            }
            if !structure.forbid_unsafe {
                unmet.push("no #![forbid(unsafe_code)]".to_string());
            }
            if structure.std_reentry {
                unmet.push("extern crate std".to_string());
            }
            if !structure.unreviewed_dependencies.is_empty() {
                unmet.push(format!(
                    "dependencies outside the semantic crates: {}",
                    structure.unreviewed_dependencies.join(", ")
                ));
            }
            if !structure.unrecognized_manifest.is_empty() {
                unmet.push(format!(
                    "manifest not read completely: {}",
                    structure.unrecognized_manifest.join(", ")
                ));
            }
            if !structure.external_sources.is_empty() {
                unmet.push(format!(
                    "source outside the checked files: {}",
                    structure.external_sources.join(", ")
                ));
            }
            if !structure.binary_targets.is_empty() {
                unmet.push(format!(
                    "binary targets, which confinement does not cover: {}",
                    structure.binary_targets.join(", ")
                ));
            }
            if unmet.is_empty() {
                output.push_str(&format!("{}: confined\n", structure.krate));
            } else {
                output.push_str(&format!(
                    "{}: not confined: {}\n",
                    structure.krate,
                    unmet.join("; ")
                ));
            }
        }
        output.push_str(&format!(
            "purity: {} ({} errors, {} warnings) in {} files\n",
            self.status(),
            self.errors(),
            self.findings.len() - self.errors(),
            self.files.len()
        ));
        output
    }
}

/// Largest source or manifest file the check reads.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Checks every listed file, directory, or crate.
///
/// Anything the check cannot read is reported as unreadable, so a file it
/// did not read can never leave the result clean or confined.
pub(crate) fn check_paths(paths: &[PathBuf]) -> Report {
    let mut report = Report::default();
    for path in paths {
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            check_crate(path, &mut report);
        } else if path.is_dir() {
            for file in rust_files(path, &mut report) {
                check_file(&file, &mut report);
            }
        } else {
            check_file(path, &mut report);
        }
    }
    report.findings.sort();
    report.findings.dedup();
    report
}

/// Lists the Rust files under a directory without following symbolic links.
///
/// A directory or entry that cannot be listed, a link that leads to a
/// directory or names a Rust file, and a directory with no Rust files are
/// reported as unreadable. Cargo build directories, named `target` and marked
/// with `CACHEDIR.TAG`, are skipped.
fn rust_files(directory: &Path, report: &mut Report) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = match fs::read_dir(&next) {
            Ok(entries) => entries,
            Err(error) => {
                report
                    .unreadable
                    .push((next.display().to_string(), error.to_string()));
                continue;
            }
        };
        for entry in entries {
            let (path, kind) = match entry.and_then(|entry| Ok((entry.path(), entry.file_type()?)))
            {
                Ok(listed) => listed,
                Err(error) => {
                    report
                        .unreadable
                        .push((next.display().to_string(), error.to_string()));
                    continue;
                }
            };
            let rust = path.extension().is_some_and(|extension| extension == "rs");
            if kind.is_symlink() {
                if rust || path.is_dir() {
                    report.unreadable.push((
                        path.display().to_string(),
                        "symbolic link not followed; list its target instead".to_string(),
                    ));
                }
            } else if kind.is_dir() {
                let build = path.file_name().is_some_and(|name| name == "target")
                    && path.join("CACHEDIR.TAG").is_file();
                if !build {
                    pending.push(path);
                }
            } else if rust {
                files.push(path);
            }
        }
    }
    if files.is_empty() {
        report.unreadable.push((
            directory.display().to_string(),
            "no Rust source files".to_string(),
        ));
    }
    files.sort();
    files
}

/// Checks one file and returns its syntax tree and text, or reports it as
/// unreadable.
fn check_file(path: &Path, report: &mut Report) -> Option<(syn::File, String)> {
    let name = path.display().to_string();
    report.files.push(name.clone());
    let checked = read_bounded(path, MAX_FILE_BYTES).and_then(|source| {
        let (file, findings) = check_source(&name, &source)?;
        Ok((file, findings, source))
    });
    match checked {
        Ok((file, findings, source)) => {
            report.findings.extend(findings);
            Some((file, source))
        }
        Err(message) => {
            report.unreadable.push((name, message));
            None
        }
    }
}

/// Reads one regular file of at most `limit` bytes as UTF-8.
fn read_bounded(path: &Path, limit: u64) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".to_string());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(limit.saturating_add(1)).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > limit) {
        return Err(format!("larger than {limit} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())
}

/// Checks one source text; the name labels its findings.
pub(crate) fn check_source(name: &str, source: &str) -> Result<(syn::File, Vec<Finding>), String> {
    let file = syn::parse_file(source).map_err(|error| {
        let start = error.span().start();
        format!("{}:{}: {error}", start.line, start.column + 1)
    })?;
    let mut aliases = Aliases::default();
    aliases.visit_file(&file);
    let mut scanner = Scanner {
        file: name,
        aliases: aliases.names,
        findings: Vec::new(),
    };
    for entry in &aliases.entries {
        scanner.check_import(entry);
    }
    scanner.visit_file(&file);
    Ok((file, scanner.findings))
}

fn check_crate(directory: &Path, report: &mut Report) {
    let mut structure = Structure {
        krate: directory.display().to_string(),
        ..Structure::default()
    };
    let manifest = directory.join("Cargo.toml");
    match read_bounded(&manifest, MAX_FILE_BYTES) {
        Ok(text) => {
            let dependencies = manifest_dependencies(&text);
            structure.unrecognized_manifest = dependencies.unrecognized;
            for dependency in dependencies.names {
                if let Some(rule) = crate_rule(&dependency) {
                    report.findings.push(Finding {
                        file: manifest.display().to_string(),
                        line: 0,
                        column: 0,
                        rule,
                        severity: Severity::Error,
                        subject: format!("dependency {dependency}"),
                    });
                } else if !ALLOWED_DEPENDENCIES.contains(&dependency.as_str()) {
                    structure.unreviewed_dependencies.push(dependency);
                }
            }
        }
        Err(message) => report
            .unreadable
            .push((manifest.display().to_string(), message)),
    }
    let source = directory.join("src");
    // Confinement is a claim about the library, so its root must be
    // `src/lib.rs` and the package must build no binary: Cargo builds
    // `src/main.rs` and every file in `src/bin/` as one.
    for binary in ["main.rs", "bin"] {
        let path = source.join(binary);
        if fs::symlink_metadata(&path).is_ok() {
            structure.binary_targets.push(path.display().to_string());
        }
    }
    let root = source.join("lib.rs");
    for file in rust_files(&source, report) {
        let Some((parsed, text)) = check_file(&file, report) else {
            continue;
        };
        match text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .parse::<TokenStream>()
        {
            Ok(tokens) => scan_escapes(&file.display().to_string(), tokens, &mut structure),
            Err(error) => structure
                .external_sources
                .push(format!("{}: {error}", file.display())),
        }
        if file == root {
            structure.no_std = parsed.attrs.iter().any(|attribute| {
                attribute.path().is_ident("no_std") && matches!(attribute.meta, syn::Meta::Path(_))
            });
            structure.forbid_unsafe = parsed.attrs.iter().any(forbids_unsafe_code);
        }
    }
    report.structures.push(structure);
}

/// Records what in one crate file reaches past the checked source: `extern
/// crate std`, the `include` macro under any name, and a `path` attribute,
/// which can place a module outside `src/`. Macro bodies are scanned too, so a
/// macro that could expand to any of them counts.
fn scan_escapes(file: &str, tokens: TokenStream, structure: &mut Structure) {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(ident) if ident.unraw() == "include" => {
                structure
                    .external_sources
                    .push(located(file, ident.span(), "`include`"));
            }
            TokenTree::Ident(ident) if ident == "extern" && names_crate_std(&tokens, index) => {
                structure.std_reentry = true;
            }
            TokenTree::Group(group) => {
                let attribute = group.delimiter() == Delimiter::Bracket
                    && index > 0
                    && matches!(&tokens[index - 1], TokenTree::Punct(punct) if matches!(punct.as_char(), '#' | '!'));
                if attribute && assigns_path(group.stream()) {
                    structure.external_sources.push(located(
                        file,
                        group.span(),
                        "`path` attribute",
                    ));
                }
                scan_escapes(file, group.stream(), structure);
            }
            _ => {}
        }
    }
}

/// Returns whether `extern` at `index` begins `extern crate std`.
fn names_crate_std(tokens: &[TokenTree], index: usize) -> bool {
    matches!(
        (tokens.get(index + 1), tokens.get(index + 2)),
        (Some(TokenTree::Ident(keyword)), Some(TokenTree::Ident(name)))
            if keyword == "crate" && name.unraw() == "std"
    )
}

/// Returns whether attribute tokens assign `path` at any depth, as in
/// `#[path = "x.rs"]` or `#[cfg_attr(unix, path = "x.rs")]`.
fn assigns_path(tokens: TokenStream) -> bool {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Ident(ident) => {
            ident == "path"
                && matches!(tokens.get(index + 1), Some(TokenTree::Punct(punct))
                    if punct.as_char() == '=' && punct.spacing() == Spacing::Alone)
        }
        TokenTree::Group(group) => assigns_path(group.stream()),
        _ => false,
    })
}

fn located(file: &str, span: Span, subject: &str) -> String {
    let start = span.start();
    format!("{file}:{}:{}: {subject}", start.line, start.column + 1)
}

fn forbids_unsafe_code(attribute: &syn::Attribute) -> bool {
    if !attribute.path().is_ident("forbid") {
        return false;
    }
    let mut found = false;
    let _ = attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("unsafe_code") {
            found = true;
        }
        Ok(())
    });
    found
}

/// Dependency names read from a manifest, and the lines whose effect on the
/// dependency set was not established.
///
/// The reader accepts the common forms of `[dependencies]`,
/// `[dependencies.NAME]`, and their `target` variants. Names in headers and
/// keys are compared without quotes or spaces, so `"path"` is `path`. It
/// reports instead of interpreting:
/// - a rename, workspace inheritance, or a source patch;
/// - a library `path` or `autolib`, and any binary target;
/// - an escape sequence;
/// - any other form that could add a dependency or change the targets.
///
/// Any reported line keeps the crate from being confined, so text read after
/// one cannot produce a confined result.
#[derive(Debug, Default, Eq, PartialEq)]
struct ManifestDependencies {
    names: Vec<String>,
    unrecognized: Vec<String>,
}

/// The kind of manifest table a line belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Table {
    /// A table that declares no runtime dependency, such as `[package]`.
    Other,
    /// `[dependencies]` or `[target.X.dependencies]`: each key names one.
    Dependencies,
    /// `[dependencies.NAME]` or its target variant: each key describes NAME.
    Dependency,
    /// `[lib]`.
    Library,
    /// A table already reported; its lines are not read.
    Unrecognized,
}

fn manifest_dependencies(manifest: &str) -> ManifestDependencies {
    // Cargo accepts a leading byte-order mark, so it must not hide the first
    // header.
    let manifest = manifest.strip_prefix('\u{feff}').unwrap_or(manifest);
    let mut names = BTreeSet::new();
    let mut unrecognized = Vec::new();
    let mut table = Table::Other;
    let mut open_string: Option<&str> = None;
    for (index, raw) in manifest.lines().enumerate() {
        let line = raw.trim();
        let mut refuse = |reason: &str| unrecognized.push(format!("line {}: {reason}", index + 1));
        if let Some(delimiter) = open_string {
            if line.contains('\\') {
                refuse("escape sequence in a multi-line string");
            } else if line.matches(delimiter).count() == 1 {
                open_string = None;
            } else if line.contains(delimiter) {
                refuse("multi-line string not recognized");
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = match read_header(line) {
                Ok((kind, name)) => {
                    names.extend(name);
                    kind
                }
                Err(reason) => {
                    refuse(reason);
                    Table::Unrecognized
                }
            };
            continue;
        }
        if let Some(delimiter) = ["\"\"\"", "'''"].into_iter().find(|d| line.contains(d)) {
            // The key before the string is still checked below.
            let rest = line.replacen(delimiter, "", 1);
            if table != Table::Other || rest.contains(['"', '\'', '\\']) {
                refuse("multi-line string not recognized");
            } else {
                open_string = Some(delimiter);
            }
        }
        if line.contains('\\') && matches!(table, Table::Dependencies | Table::Dependency) {
            refuse("escape sequence in a dependency declaration");
            continue;
        }
        // A line without `=` continues an array and cannot add a key.
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let inline_table = value.starts_with('{');
        let bare_key = bare(key);
        let first = bare_key.split('.').next().unwrap_or_default();
        match table {
            Table::Dependencies => {
                let (name, field) = key
                    .split_once('.')
                    .map_or((key, None), |(name, field)| (name, Some(field)));
                match dependency_name(name) {
                    Some(name) => {
                        names.insert(name);
                    }
                    None => refuse("dependency name not recognized"),
                }
                if field.is_some_and(changes_dependency_identity)
                    || assigns_key(value, "package")
                    || assigns_key(value, "workspace")
                {
                    refuse("dependency renamed or inherited from the workspace");
                } else if inline_table && !value.contains('}') {
                    refuse("multi-line inline table");
                }
            }
            Table::Dependency => {
                if changes_dependency_identity(key) {
                    refuse("dependency renamed or inherited from the workspace");
                }
            }
            Table::Library => {
                if key.contains('\\') {
                    refuse("escape sequence in a library key");
                } else if first == "path" {
                    refuse("library root set by path");
                }
            }
            Table::Other => {
                if key.contains('\\') || (inline_table && value.contains('\\')) {
                    refuse("escape sequence in a key or inline table");
                } else if let Some(reason) = changes_targets_or_sources(first) {
                    refuse(reason);
                } else if names_runtime_dependencies(&bare_key)
                    || (inline_table && names_runtime_dependencies(value))
                {
                    refuse("dependency declared outside a dependency table");
                }
            }
            Table::Unrecognized => {}
        }
    }
    if open_string.is_some() {
        unrecognized.push("unterminated multi-line string".to_string());
    }
    ManifestDependencies {
        names: names.into_iter().collect(),
        unrecognized,
    }
}

/// Reads one table header. A single-dependency table also returns its
/// dependency's name. An error says why the header is refused.
fn read_header(line: &str) -> Result<(Table, Option<String>), &'static str> {
    const UNRECOGNIZED: &str = "table header not recognized";
    let array = line.starts_with("[[");
    let split = if array {
        line.strip_prefix("[[")
            .and_then(|inner| inner.split_once("]]"))
    } else {
        line.strip_prefix('[')
            .and_then(|inner| inner.split_once(']'))
    };
    let (inner, rest) = split.ok_or(UNRECOGNIZED)?;
    let rest = rest.trim();
    if !(rest.is_empty() || rest.starts_with('#')) || inner.contains('\\') {
        return Err(UNRECOGNIZED);
    }
    let header = bare(inner);
    if header == "lib" && !array {
        return Ok((Table::Library, None));
    }
    if let Some(reason) = changes_targets_or_sources(header.split('.').next().unwrap_or_default()) {
        return Err(reason);
    }
    if array {
        // Tests, examples, and benches are not part of the library.
        return if names_runtime_dependencies(&header) {
            Err(UNRECOGNIZED)
        } else {
            Ok((Table::Other, None))
        };
    }
    if header == "dependencies" {
        return Ok((Table::Dependencies, None));
    }
    // Workspace declarations are not dependencies; a member that inherits
    // one is refused where it does.
    if header == "workspace.dependencies" || header.starts_with("workspace.dependencies.") {
        return Ok((Table::Other, None));
    }
    let dependency_table = header.strip_prefix("dependencies.").or_else(|| {
        let target = header.strip_prefix("target.")?;
        let (_, after) = target.rsplit_once(".dependencies")?;
        if after.is_empty() {
            Some("")
        } else {
            after.strip_prefix('.')
        }
    });
    match dependency_table {
        Some("") => Ok((Table::Dependencies, None)),
        Some(name) => Ok((
            Table::Dependency,
            Some(dependency_name(name).ok_or(UNRECOGNIZED)?),
        )),
        None if names_runtime_dependencies(&header) => Err(UNRECOGNIZED),
        None => Ok((Table::Other, None)),
    }
}

/// Returns a header or key without quotes or spaces, for comparing names.
fn bare(text: &str) -> String {
    text.chars()
        .filter(|character| !matches!(character, '"' | '\'') && !character.is_whitespace())
        .collect()
}

/// Returns a dependency key without quotes, if it is a plain crate name.
fn dependency_name(key: &str) -> Option<String> {
    let key = key.trim();
    let name = ['"', '\'']
        .into_iter()
        .find_map(|quote| key.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(key);
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    plain.then(|| name.to_string())
}

/// Returns whether a dependency field changes which crate is used.
fn changes_dependency_identity(field: &str) -> bool {
    matches!(
        field.trim().trim_matches(['"', '\'']),
        "package" | "workspace"
    )
}

/// Returns whether an inline table assigns `key`, as in `{ package = "x" }`.
fn assigns_key(value: &str, key: &str) -> bool {
    value.match_indices(key).any(|(start, _)| {
        let bounded = value[..start].chars().next_back().is_none_or(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        });
        let after = &value[start + key.len()..];
        let after = after.strip_prefix(['"', '\'']).unwrap_or(after);
        bounded && after.trim_start().starts_with('=')
    })
}

/// Returns whether text mentions `dependencies` other than as
/// `dev-dependencies` or `build-dependencies`.
fn names_runtime_dependencies(text: &str) -> bool {
    text.match_indices("dependencies").any(|(start, _)| {
        let before = &text[..start];
        !(before.ends_with("dev-") || before.ends_with("build-"))
    })
}

/// Returns why a table or key named `first`, outside `[lib]` itself, changes
/// the package's targets or where its dependencies come from.
fn changes_targets_or_sources(first: &str) -> Option<&'static str> {
    match first {
        "lib" | "autolib" => Some("library target changed"),
        "bin" => Some("binary target, which confinement does not cover"),
        "patch" | "replace" => Some("dependency sources replaced"),
        _ => None,
    }
}

/// Returns the rule that a whole dependency or crate root falls under.
fn crate_rule(name: &str) -> Option<&'static str> {
    let root = name.replace('-', "_");
    PATH_RULES
        .iter()
        .find(|(_, prefixes)| prefixes.contains(&root.as_str()))
        .map(|(rule, _)| *rule)
}

/// One `use` or `extern crate` entry.
struct Import {
    path: Vec<String>,
    glob: bool,
    span: Span,
}

/// Collects every import and its local name, in every scope of one file.
#[derive(Default)]
struct Aliases {
    names: BTreeMap<String, Vec<String>>,
    entries: Vec<Import>,
}

impl Aliases {
    fn walk(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.walk(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let (local, path) = if name.ident == "self" {
                    (prefix.last().cloned().unwrap_or_default(), prefix.clone())
                } else {
                    let mut path = prefix.clone();
                    path.push(name.ident.to_string());
                    (name.ident.to_string(), path)
                };
                self.add(local, path, name.ident.span());
            }
            syn::UseTree::Rename(rename) => {
                let mut path = prefix.clone();
                if rename.ident != "self" {
                    path.push(rename.ident.to_string());
                }
                self.add(rename.rename.to_string(), path, rename.ident.span());
            }
            syn::UseTree::Glob(glob) => self.entries.push(Import {
                path: prefix.clone(),
                glob: true,
                span: glob.star_token.spans[0],
            }),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.walk(item, prefix);
                }
            }
        }
    }

    fn add(&mut self, local: String, path: Vec<String>, span: Span) {
        if !path.is_empty() && local != "_" {
            self.names.insert(local, path.clone());
        }
        self.entries.push(Import {
            path,
            glob: false,
            span,
        });
    }
}

impl<'ast> Visit<'ast> for Aliases {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.walk(&item.tree, &mut Vec::new());
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let path = vec![item.ident.to_string()];
        let local = item
            .rename
            .as_ref()
            .map_or_else(|| item.ident.to_string(), |(_, rename)| rename.to_string());
        self.add(local, path, item.ident.span());
    }
}

struct Scanner<'a> {
    file: &'a str,
    aliases: BTreeMap<String, Vec<String>>,
    findings: Vec<Finding>,
}

impl Scanner<'_> {
    fn report(&mut self, span: Span, rule: &'static str, severity: Severity, subject: String) {
        let start = span.start();
        self.findings.push(Finding {
            file: self.file.to_string(),
            line: start.line,
            column: start.column + 1,
            rule,
            severity,
            subject,
        });
    }

    fn check_import(&mut self, import: &Import) {
        let path = normalize(&import.path);
        let joined = path.join("::");
        if import.glob {
            // A glob imports every denied item under its module.
            let mut rules = BTreeSet::new();
            for (rule, prefixes) in PATH_RULES {
                if prefixes.iter().any(|prefix| {
                    matches_prefix(&joined, prefix) || prefix.starts_with(&format!("{joined}::"))
                }) {
                    rules.insert(*rule);
                }
            }
            for rule in rules {
                self.report(import.span, rule, Severity::Error, format!("{joined}::*"));
            }
            return;
        }
        self.match_rules(&path, import.span, true);
    }

    /// Reports every rule a resolved path matches. A code path of one
    /// segment, such as a variable named `rand`, is a local name, so only
    /// imports (`explicit`) and longer paths match crate roots.
    fn match_rules(&mut self, path: &[String], span: Span, explicit: bool) {
        let joined = path.join("::");
        for (rule, prefixes) in PATH_RULES {
            if (explicit || path.len() > 1)
                && prefixes
                    .iter()
                    .any(|prefix| matches_prefix(&joined, prefix))
            {
                self.report(span, rule, Severity::Error, joined.clone());
            }
        }
        for (rule, prefixes) in WARNING_PATH_RULES {
            if prefixes
                .iter()
                .any(|prefix| matches_prefix(&joined, prefix))
            {
                self.report(span, rule, Severity::Warning, joined.clone());
            }
        }
    }

    fn check_code_path(&mut self, segments: &[String], absolute: bool, span: Span) {
        let Some(first) = segments.first() else {
            return;
        };
        let resolved = match self.aliases.get(first) {
            Some(target) if !absolute => {
                let mut full = target.clone();
                full.extend_from_slice(&segments[1..]);
                full
            }
            _ => segments.to_vec(),
        };
        if matches!(resolved[0].as_str(), "crate" | "self" | "super" | "Self") {
            return;
        }
        self.match_rules(&normalize(&resolved), span, false);
    }

    /// Finds paths and address formats inside a macro's unparsed tokens.
    fn scan_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            match &tokens[index] {
                TokenTree::Group(group) => self.scan_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    let text = literal.to_string();
                    if text.starts_with('"') && address_format(&text) {
                        self.report(
                            literal.span(),
                            "address",
                            Severity::Error,
                            "pointer formatting {:p}".to_string(),
                        );
                    }
                }
                TokenTree::Ident(ident) => {
                    let absolute = index >= 2 && is_path_separator(&tokens, index - 2);
                    let mut segments = vec![ident.to_string()];
                    let mut next = index + 1;
                    while is_path_separator(&tokens, next) {
                        let Some(TokenTree::Ident(segment)) = tokens.get(next + 2) else {
                            break;
                        };
                        segments.push(segment.to_string());
                        next += 3;
                    }
                    if segments.len() > 1 || self.aliases.contains_key(&segments[0]) {
                        self.check_code_path(&segments, absolute, ident.span());
                    }
                    index = next;
                    continue;
                }
                TokenTree::Punct(_) => {}
            }
            index += 1;
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if let Some(first) = path.segments.first() {
            self.check_code_path(&segments, path.leading_colon.is_some(), first.ident.span());
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(last) = mac.path.segments.last() {
            let name = last.ident.to_string();
            for (rule, names) in MACRO_RULES {
                if names.contains(&name.as_str()) {
                    self.report(last.ident.span(), rule, Severity::Error, format!("{name}!"));
                }
            }
        }
        // A qualified macro path, such as `core::ptr::addr_of`, is checked
        // like any other path; a bare name was matched above.
        if mac.path.segments.len() > 1 {
            let segments: Vec<String> = mac
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            if let Some(first) = mac.path.segments.first() {
                self.check_code_path(
                    &segments,
                    mac.path.leading_colon.is_some(),
                    first.ident.span(),
                );
            }
        }
        self.scan_tokens(mac.tokens.clone());
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.report(
                item.ident.span(),
                "shared-state",
                Severity::Error,
                format!("static mut {}", item.ident),
            );
        }
        visit::visit_item_static(self, item);
    }

    fn visit_expr_unsafe(&mut self, expr: &'ast syn::ExprUnsafe) {
        self.report(
            expr.unsafe_token.span,
            "unsafe",
            Severity::Error,
            "unsafe block".to_string(),
        );
        visit::visit_expr_unsafe(self, expr);
    }

    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if let Some(token) = &signature.unsafety {
            self.report(
                token.span,
                "unsafe",
                Severity::Error,
                format!("unsafe fn {}", signature.ident),
            );
        }
        visit::visit_signature(self, signature);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if let Some(token) = &item.unsafety {
            self.report(
                token.span,
                "unsafe",
                Severity::Error,
                "unsafe impl".to_string(),
            );
        }
        visit::visit_item_impl(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if let Some(token) = &item.unsafety {
            self.report(
                token.span,
                "unsafe",
                Severity::Error,
                format!("unsafe trait {}", item.ident),
            );
        }
        visit::visit_item_trait(self, item);
    }

    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.report(
            item.abi.extern_token.span,
            "foreign-code",
            Severity::Error,
            "extern block".to_string(),
        );
        visit::visit_item_foreign_mod(self, item);
    }

    fn visit_type_ptr(&mut self, pointer: &'ast syn::TypePtr) {
        self.report(
            pointer.star_token.spans[0],
            "address",
            Severity::Error,
            "raw pointer type".to_string(),
        );
        visit::visit_type_ptr(self, pointer);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        if ADDRESS_METHODS.contains(&name.as_str()) {
            self.report(
                call.method.span(),
                "address",
                Severity::Error,
                format!(".{name}()"),
            );
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_lit_float(&mut self, literal: &'ast syn::LitFloat) {
        self.report(
            literal.span(),
            "floating-point",
            Severity::Warning,
            literal.base10_digits().to_string(),
        );
    }
}

fn is_path_separator(tokens: &[TokenTree], index: usize) -> bool {
    matches!(
        (tokens.get(index), tokens.get(index + 1)),
        (Some(TokenTree::Punct(first)), Some(TokenTree::Punct(second)))
            if first.as_char() == ':' && second.as_char() == ':'
    )
}

/// Treats `core::` and `alloc::` paths as the `std::` paths they re-export.
fn normalize(path: &[String]) -> Vec<String> {
    let mut normalized = path.to_vec();
    if let Some(first) = normalized.first_mut()
        && (first == "core" || first == "alloc")
    {
        *first = "std".to_string();
    }
    normalized
}

fn matches_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with("::"))
}

/// Recognizes a `{:p}` or `{name:p}` style pointer format in a string literal.
fn address_format(literal: &str) -> bool {
    let mut rest = literal;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(escaped) = after.strip_prefix('{') {
            rest = escaped;
            continue;
        }
        let Some(end) = after.find('}') else {
            return false;
        };
        let spec = &after[..end];
        if let Some((_, format)) = spec.split_once(':')
            && format.ends_with('p')
        {
            return true;
        }
        rest = &after[end + 1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(name: &str, source: &str) -> Vec<Finding> {
        check_source(name, source)
            .map(|(_, findings)| findings)
            .unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    fn rules(source: &str) -> Vec<(&'static str, Severity)> {
        findings("test.rs", source)
            .into_iter()
            .map(|finding| (finding.rule, finding.severity))
            .collect()
    }

    fn errors(source: &str) -> BTreeSet<&'static str> {
        rules(source)
            .into_iter()
            .filter(|(_, severity)| *severity == Severity::Error)
            .map(|(rule, _)| rule)
            .collect()
    }

    fn set(names: &[&'static str]) -> BTreeSet<&'static str> {
        names.iter().copied().collect()
    }

    #[test]
    fn pure_decision_code_has_no_findings() {
        let source = r#"
            use std::collections::BTreeMap;
            use alloc::vec::Vec;
            pub fn decide(count: i64, limit: i64, rand: i64) -> Option<i64> {
                let mut totals = BTreeMap::new();
                totals.insert(1, count);
                let time = count + rand;
                if time < limit { Some(time) } else { None }
            }
        "#;
        assert!(rules(source).is_empty(), "{:?}", rules(source));
    }

    #[test]
    fn every_rule_family_is_detected() {
        let cases: &[(&str, &[&'static str])] = &[
            (
                "fn f() { let _ = std::time::SystemTime::now(); }",
                &["clock"],
            ),
            ("fn f() { let _ = std::env::var(\"X\"); }", &["environment"]),
            ("fn f() { let _ = std::fs::read(\"x\"); }", &["filesystem"]),
            ("fn f() { let _ = std::io::stdin(); }", &["io"]),
            (
                "fn f() { let _ = std::net::TcpStream::connect(\"x\"); }",
                &["network"],
            ),
            ("fn f() { let _ = std::process::id(); }", &["process"]),
            ("fn f() { std::thread::spawn(|| ()); }", &["threads"]),
            ("fn f() { let _: u8 = rand::random(); }", &["randomness"]),
            (
                "fn f() { let _ = std::collections::HashMap::<u8, u8>::new(); }",
                &["hash-order"],
            ),
            (
                "static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);",
                &["shared-state"],
            ),
            ("static mut C: u64 = 0;", &["shared-state"]),
            ("thread_local! { static C: u8 = 0; }", &["shared-state"]),
            ("fn f() { unsafe {} }", &["unsafe"]),
            ("unsafe fn f() {}", &["unsafe"]),
            ("fn f(x: *const u8) {}", &["address"]),
            (
                "fn f(x: &u8) -> String { format!(\"{:p}\", x) }",
                &["address"],
            ),
            (
                "fn f(x: &u8) -> String { format!(\"{x:p}\") }",
                &["address"],
            ),
            (
                "fn f(x: &u8) -> usize { (x as *const u8).addr() }",
                &["address"],
            ),
            (
                "fn f(x: &u8) -> usize { core::ptr::addr_of!(*x) as usize }",
                &["address"],
            ),
            ("extern \"C\" { fn g(); }", &["foreign-code"]),
            ("fn f() { println!(\"x\"); }", &["io"]),
        ];
        for (source, expected) in cases {
            assert_eq!(errors(source), set(expected), "{source}");
        }
    }

    #[test]
    fn aliases_groups_globs_and_absolute_paths_are_resolved() {
        let cases: &[(&str, &[&'static str])] = &[
            (
                "use std::time::SystemTime as Clock; fn f() { let _ = Clock::now(); }",
                &["clock"],
            ),
            (
                "use std::{env, fs}; fn f() {}",
                &["environment", "filesystem"],
            ),
            ("use std::io::{self, Read}; fn f() {}", &["io"]),
            // The glob also brings in `hash_map::RandomState`.
            (
                "use std::collections::*; fn f() {}",
                &["hash-order", "randomness"],
            ),
            (
                "fn f() { let _ = ::std::env::var(\"X\"); }",
                &["environment"],
            ),
            (
                "use core::sync::atomic::AtomicU32; fn f() {}",
                &["shared-state"],
            ),
            (
                "use core::cell::Cell; struct P { calls: Cell<u32> }",
                &["shared-state"],
            ),
            (
                "extern crate rand as dice; fn f() -> u8 { dice::random() }",
                &["randomness"],
            ),
            ("mod clock { pub use std::time::Instant; }", &["clock"]),
            // The imports below name harmless modules, so only resolving the
            // later paths through them finds the denied items.
            (
                "use std::collections; fn f() { let _ = collections::HashMap::<u8, u8>::new(); }",
                &["hash-order"],
            ),
            (
                "use std::sync as s; static C: s::atomic::AtomicU64 = s::atomic::AtomicU64::new(0);",
                &["shared-state"],
            ),
            (
                "use std::collections; fn f() { let _ = vec![collections::HashSet::<u8>::new()]; }",
                &["hash-order"],
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(errors(source), set(expected), "{source}");
        }
    }

    #[test]
    fn paths_inside_macro_invocations_are_checked() {
        let source = "fn f() -> String { format!(\"{:?}\", std::time::SystemTime::now()) }";
        assert_eq!(errors(source), set(&["clock"]));
        let aliased = "use std::env as e; fn f() { assert!(e::var(\"X\").is_ok()); }";
        assert_eq!(errors(aliased), set(&["environment"]));
    }

    #[test]
    fn floating_point_is_a_warning_and_escaped_braces_are_not_formats() {
        let source = "fn f(x: f64) -> f64 { x * 2.0 }";
        assert!(errors(source).is_empty());
        assert!(rules(source).contains(&("floating-point", Severity::Warning)));
        assert!(!address_format("\"{{:p}}\""));
        assert!(address_format("\"{0:#p}\""));
        assert!(!address_format("\"{:?} {x}\""));
    }

    #[test]
    fn findings_carry_their_line_and_column() {
        let findings = findings(
            "a.rs",
            "fn f() {}\nfn g() { let _ = std::env::var(\"X\"); }\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!((findings[0].line, findings[0].column), (2, 18));
    }

    #[test]
    fn unparseable_source_is_reported_not_passed() {
        assert!(check_source("bad.rs", "fn f( {").is_err());
    }

    #[test]
    fn manifest_dependencies_are_read_from_every_dependency_table() {
        let manifest = r#"
            [package]
            name = "decisions"
            [dependencies]
            zeno-fcis-core = "=1.1.0"
            rand = "0.9"
            [dependencies.zeno-fcis-value]
            version = "=1.1.0"
            [target.'cfg(unix)'.dependencies]
            libc = "0.2"
            [dev-dependencies]
            proptest = "1"
        "#;
        let read = manifest_dependencies(manifest);
        assert_eq!(read.unrecognized, Vec::<String>::new());
        assert_eq!(
            read.names,
            ["libc", "rand", "zeno-fcis-core", "zeno-fcis-value"]
        );
        assert_eq!(crate_rule("rand"), Some("randomness"));
        assert_eq!(crate_rule("async-std"), Some("threads"));
        assert_eq!(crate_rule("zeno-fcis-core"), None);
    }

    #[test]
    fn manifest_forms_that_could_hide_a_dependency_or_add_a_target_are_refused() {
        let refused = [
            "[dependencies]\nzeno-fcis-value = { package = \"evil-clock\", version = \"1\" }",
            "[dependencies]\nzeno-fcis-value = { \"package\" = \"evil-clock\" }",
            "[dependencies]\nzeno-fcis-value = { \"pack\\u0061ge\" = \"evil-clock\" }",
            "[dependencies]\nzeno-fcis-value.package = \"evil-clock\"",
            "[dependencies]\nzeno-fcis-value = { workspace = true }",
            "[dependencies]\nzeno-fcis-value.workspace = true",
            "[dependencies]\nzeno-fcis-value = {\n  package = \"evil-clock\",\n}",
            "[dependencies.zeno-fcis-value]\npackage = \"evil-clock\"",
            "[dependencies.zeno-fcis-value]\nversion = \"\"\"\n1\"\"\"",
            "[target.'cfg(unix)'.dependencies.zeno-fcis-value]\npackage = \"evil-clock\"",
            "dependencies.evil-clock = \"1\"",
            "dependencies.evil-clock = \"\"\"\n1\"\"\"",
            "[target.'cfg(unix)']\ndependencies = { evil-clock = \"1\" }",
            "[target]\n'cfg(unix)' = { dependencies = { evil-clock = \"1\" } }",
            "[patch.crates-io]\nzeno-fcis-value = { path = \"../evil\" }",
            "[\"patch\".\"crates-io\"]\nzeno-fcis-value = { path = \"../evil\" }",
            "\"patch\".crates-io.zeno-fcis-value.path = \"../evil\"",
            "[lib]\npath = \"src/other.rs\"",
            // Found by an independent review of 3e056c8: a quoted key.
            "[lib]\n\"path\" = \"outside.rs\"",
            "[lib]\n'path' = \"outside.rs\"",
            "[\"lib\"]\npath = \"outside.rs\"",
            "lib.path = \"outside.rs\"",
            "lib = { path = \"outside.rs\" }",
            "[package]\nautolib = false",
            // Found by the same review: a binary target outside `src/`.
            "[[bin]]\nname = \"helper\"\npath = \"outside.rs\"",
            "[[\"bin\"]]\nname = \"helper\"",
            "bin = [{ name = \"helper\", path = \"outside.rs\" }]",
            // Cargo accepts a leading byte-order mark before the first header.
            "\u{feff}[lib]\npath = \"outside.rs\"",
            "[package]\ndescription = \"\"\"\nno end",
        ];
        for manifest in refused {
            assert!(
                !manifest_dependencies(manifest).unrecognized.is_empty(),
                "accepted: {manifest}"
            );
        }
        let recognized: [(&str, &[&str]); 8] = [
            (
                "[dependencies] # runtime\nevil-clock = \"1\"",
                &["evil-clock"],
            ),
            (
                "[target.'cfg(unix)'.dependencies.evil-clock]\nversion = \"1\"",
                &["evil-clock"],
            ),
            (
                "[package]\ndescription = \"\"\"\n[dependencies]\nnot-real = \"1\"\n\"\"\"\n[dependencies]\nzeno-fcis-core = \"1\"",
                &["zeno-fcis-core"],
            ),
            (
                "[dependencies.zeno-fcis-value]\nversion = \"1\"\nfeatures = [\n  \"std\",\n]",
                &["zeno-fcis-value"],
            ),
            (
                "[dev-dependencies]\nproptest = \"1\"\n[build-dependencies]\ncc = \"1\"\n[target.'cfg(unix)'.dev-dependencies]\nlibc = \"0.2\"",
                &[],
            ),
            // A quoted header names the same table as a bare one.
            ("[\"dependencies\"]\nevil-clock = \"1\"", &["evil-clock"]),
            ("[workspace.dependencies]\nrand = \"0.9\"", &[]),
            (
                "[lib]\nname = \"decisions\"\n[[test]]\nname = \"laws\"\npath = \"tests/laws.rs\"",
                &[],
            ),
        ];
        for (manifest, names) in recognized {
            let read = manifest_dependencies(manifest);
            assert_eq!(read.unrecognized, Vec::<String>::new(), "{manifest}");
            assert_eq!(read.names, names, "{manifest}");
        }
    }

    /// A scratch directory for one test, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("zeno-purity-{label}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap_or_else(|error| panic!("{error}"));
            Self(root)
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
            }
            fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        }

        fn check(&self) -> Report {
            check_paths(std::slice::from_ref(&self.0))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const CLOCK: &str = "pub fn now() -> bool { std::time::SystemTime::now().elapsed().is_ok() }\n";
    const PURE: &str = "pub fn pure(x: u8) -> u8 { x }\n";

    #[cfg(unix)]
    #[test]
    fn unreadable_directories_and_links_are_reported_not_skipped() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let scratch = Scratch::new("walk");
        scratch.write("ok.rs", PURE);
        scratch.write("hidden/clock.rs", CLOCK);
        let hidden = scratch.0.join("hidden");
        let mode = |bits| {
            fs::set_permissions(&hidden, fs::Permissions::from_mode(bits))
                .unwrap_or_else(|error| panic!("{error}"));
        };
        mode(0o000);
        // A privileged user can still list the directory; nothing to observe.
        if fs::read_dir(&hidden).is_err() {
            let report = scratch.check();
            assert_eq!(report.status(), "unreadable", "{}", report.render());
        }
        mode(0o755);

        symlink("..", hidden.join("up")).unwrap_or_else(|error| panic!("{error}"));
        let looped = scratch.check();
        assert_eq!(looped.status(), "unreadable", "{}", looped.render());
        assert_eq!(looped.files.len(), 2, "{:?}", looped.files);
        assert!(looped.unreadable[0].1.contains("symbolic link"));
        fs::remove_file(hidden.join("up")).unwrap_or_else(|error| panic!("{error}"));

        // A link to a file that is not Rust source is not reported.
        symlink("ok.rs", scratch.0.join("notes")).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(scratch.check().status(), "violations");
    }

    #[cfg(unix)]
    #[test]
    fn missing_empty_and_special_inputs_are_unreadable() {
        let scratch = Scratch::new("empty");
        let empty = scratch.check();
        assert_eq!(empty.status(), "unreadable");
        assert_eq!(empty.unreadable[0].1, "no Rust source files");
        let device = check_paths(&[PathBuf::from("/dev/null")]);
        assert_eq!(device.unreadable[0].1, "not a regular file");
        let missing = check_paths(&[scratch.0.join("absent.rs")]);
        assert_eq!(missing.status(), "unreadable");
        scratch.write("five.rs", "12345");
        let five = scratch.0.join("five.rs");
        assert!(read_bounded(&five, 4).is_err_and(|error| error.contains("larger than 4")));
        assert_eq!(read_bounded(&five, 5).as_deref(), Ok("12345"));
    }

    #[test]
    fn cargo_build_directories_are_skipped_but_modules_named_target_are_not() {
        let scratch = Scratch::new("target");
        scratch.write("ok.rs", PURE);
        scratch.write(
            "target/CACHEDIR.TAG",
            "Signature: 8a477f597d28d172789f06886806bc55\n",
        );
        scratch.write("target/debug/generated.rs", CLOCK);
        assert_eq!(scratch.check().status(), "clean");
        fs::remove_file(scratch.0.join("target/CACHEDIR.TAG"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(scratch.check().status(), "violations");
    }

    #[test]
    fn source_from_outside_the_checked_files_prevents_confinement() {
        let scratch = Scratch::new("escapes");
        scratch.write(
            "Cargo.toml",
            "[package]\nname = \"d\"\n[dependencies]\nzeno-fcis-core = \"=1.1.0\"\n",
        );
        let header = "#![no_std]\n#![forbid(unsafe_code)]\n";
        let with = |body: &str| {
            scratch.write("src/lib.rs", &format!("{header}{body}"));
            scratch.check()
        };
        for body in [
            "include!(\"../../outside.rs\");\n",
            "use core::include as grab;\ngrab!(\"../../outside.rs\");\n",
            "macro_rules! grab { ($file:literal) => { include!($file); } }\n",
            "#[path = \"../../outside.rs\"]\nmod outside;\n",
            "#[cfg_attr(unix, path = \"../../outside.rs\")]\nmod outside;\n",
        ] {
            let report = with(body);
            assert_eq!(report.status(), "clean", "{body}: {}", report.render());
            assert!(!report.structures[0].external_sources.is_empty(), "{body}");
        }
        let nested = with("mod inner { extern crate std; }\n");
        assert_eq!(nested.status(), "clean", "{}", nested.render());
        assert!(nested.structures[0].std_reentry);
        for body in [
            "pub const DATA: &str = include_str!(\"data.txt\");\n",
            "pub fn f() -> u8 { let path = 1; path }\n",
        ] {
            let report = with(body);
            assert_eq!(report.status(), "confined", "{body}: {}", report.render());
        }
        scratch.write(
            "Cargo.toml",
            "[package]\nname = \"d\"\n[dependencies]\nzeno-fcis-core = { package = \"evil-clock\", version = \"1\" }\n",
        );
        let renamed = with(PURE);
        assert_eq!(renamed.status(), "clean", "{}", renamed.render());
        assert!(!renamed.structures[0].unrecognized_manifest.is_empty());
    }

    /// The three packages an independent review of 3e056c8 reported as
    /// falsely confined, each with a `no_std` library in `src/lib.rs`.
    #[test]
    fn only_a_library_only_package_can_be_confined() {
        let scratch = Scratch::new("targets");
        let package = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
        scratch.write("Cargo.toml", package);
        scratch.write(
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\npub fn answer() -> u8 { 42 }\n",
        );
        scratch.write("outside.rs", "fn main() {}\n");
        assert_eq!(scratch.check().status(), "confined");

        // A binary built from `src/main.rs` that uses `std`.
        scratch.write(
            "src/main.rs",
            "fn main() { let _ = std::string::String::new(); }\n",
        );
        let two_roots = scratch.check();
        assert_eq!(two_roots.status(), "clean", "{}", two_roots.render());
        assert_eq!(two_roots.structures[0].binary_targets.len(), 1);
        fs::remove_file(scratch.0.join("src/main.rs")).unwrap_or_else(|error| panic!("{error}"));

        scratch.write("src/bin/tool.rs", "fn main() {}\n");
        assert_eq!(scratch.check().status(), "clean");
        fs::remove_dir_all(scratch.0.join("src/bin")).unwrap_or_else(|error| panic!("{error}"));

        for manifest in [
            "[lib]\n\"path\" = \"outside.rs\"\n",
            "[[bin]]\nname = \"helper\"\npath = \"outside.rs\"\n",
        ] {
            scratch.write("Cargo.toml", &format!("{package}{manifest}"));
            let report = scratch.check();
            assert_eq!(report.status(), "clean", "{manifest}: {}", report.render());
            assert!(!report.structures[0].unrecognized_manifest.is_empty());
        }
    }

    #[test]
    fn escaped_library_keys_prevent_confinement() {
        let scratch = Scratch::new("escaped-library-key");
        let package = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
        scratch.write(
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\npub fn answer() -> u8 { 42 }\n",
        );
        scratch.write("outside.rs", "fn main() {}\n");

        // Ordinary quoted library keys remain supported.
        scratch.write(
            "Cargo.toml",
            &format!("{package}[lib]\n\"name\" = \"decisions\"\n"),
        );
        assert_eq!(scratch.check().status(), "confined");

        // Cargo decodes both escapes to `path`, selecting the unchecked file.
        for header in ["[lib]", r#"["lib"]"#] {
            for key in [r#""pa\u0074h""#, r#""\U00000070ath""#] {
                let manifest = format!("{package}{header}\n{key} = \"outside.rs\"\n");
                scratch.write("Cargo.toml", &manifest);
                let report = scratch.check();
                assert_eq!(report.status(), "clean", "{manifest}: {}", report.render());
                assert!(!report.structures[0].confined());
                assert!(
                    report.structures[0]
                        .unrecognized_manifest
                        .iter()
                        .any(|reason| reason.contains("escape sequence")),
                    "{manifest}: {}",
                    report.render()
                );
            }
        }
    }

    #[test]
    fn a_confined_crate_needs_every_structural_condition() {
        let root = std::env::temp_dir().join(format!("zeno-purity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
            }
            fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        };
        let check = || check_paths(std::slice::from_ref(&root));
        write(
            "Cargo.toml",
            "[package]\nname = \"d\"\n[dependencies]\nzeno-fcis-core = \"=1.1.0\"\n",
        );
        write(
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\npub fn f(x: u8) -> u8 { x }\n",
        );
        let confined = check();
        assert_eq!(confined.status(), "confined", "{}", confined.render());

        write(
            "src/lib.rs",
            "#![cfg_attr(not(feature = \"std\"), no_std)]\n#![forbid(unsafe_code)]\npub fn f(x: u8) -> u8 { x }\n",
        );
        assert_eq!(check().status(), "clean");

        write(
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\nextern crate std;\npub fn f(x: u8) -> u8 { x }\n",
        );
        assert_eq!(check().status(), "clean");

        write(
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\npub fn f(x: u8) -> u8 { x }\n",
        );
        write(
            "Cargo.toml",
            "[package]\nname = \"d\"\n[dependencies]\nserde = \"1\"\n",
        );
        let unreviewed = check();
        assert_eq!(unreviewed.status(), "clean");
        assert_eq!(unreviewed.structures[0].unreviewed_dependencies, ["serde"]);

        write(
            "Cargo.toml",
            "[package]\nname = \"d\"\n[dependencies]\nrand = \"0.9\"\n",
        );
        assert_eq!(check().status(), "violations");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_allowlist_matches_the_repository_semantic_crates() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/check_assurance.py"
        );
        let script = fs::read_to_string(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let start = script
            .find("SEMANTIC_CRATES = (")
            .unwrap_or_else(|| panic!("no SEMANTIC_CRATES in {path}"));
        let end = start
            + script[start..]
                .find(')')
                .unwrap_or_else(|| panic!("unterminated SEMANTIC_CRATES in {path}"));
        let listed: BTreeSet<&str> = script[start..end]
            .lines()
            .filter_map(|line| line.trim().strip_prefix('"')?.split('"').next())
            .collect();
        let allowed: BTreeSet<&str> = ALLOWED_DEPENDENCIES.iter().copied().collect();
        assert_eq!(listed, allowed);
    }

    #[test]
    fn the_example_template_decision_code_is_clean() {
        for (name, source) in [
            (
                "account-lockout/program.rs",
                include_str!("../templates/account-lockout/src/program.rs"),
            ),
            (
                "account-lockout/laws.rs",
                include_str!("../templates/account-lockout/src/laws.rs"),
            ),
            (
                "order-fulfillment/program.rs",
                include_str!("../templates/order-fulfillment/src/program.rs"),
            ),
            (
                "order-fulfillment/laws.rs",
                include_str!("../templates/order-fulfillment/src/laws.rs"),
            ),
            (
                "inventory-reservation/program.rs",
                include_str!("../templates/inventory-reservation/src/program.rs"),
            ),
            (
                "inventory-reservation/laws.rs",
                include_str!("../templates/inventory-reservation/src/laws.rs"),
            ),
            (
                "inventory-reservation/transition.rs",
                include_str!("../templates/inventory-reservation/synthesized/transition.rs"),
            ),
        ] {
            let findings = findings(name, source);
            assert!(findings.is_empty(), "{name}: {findings:?}");
        }
    }

    #[test]
    fn the_durable_counter_decision_code_is_clean() {
        for (name, source) in [
            (
                "program.rs",
                include_str!("../templates/durable-counter/src/program.rs"),
            ),
            (
                "laws.rs",
                include_str!("../templates/durable-counter/src/laws.rs"),
            ),
            (
                "transition.rs",
                include_str!("../templates/durable-counter/synthesized/transition.rs"),
            ),
        ] {
            let findings = findings(name, source);
            let errors: Vec<_> = findings
                .iter()
                .filter(|finding| finding.severity == Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
    }
}
