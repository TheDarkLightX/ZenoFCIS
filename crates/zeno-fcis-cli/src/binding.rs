//! The dependency binding `zeno-fcis new` writes into a generated Cargo
//! application. The application pins each ZenoFCIS package to this CLI's
//! version, and without the binding Cargo resolves those pins to the crates
//! published under the same versions, which are not this source.
//!
//! The binding is what `tools/check_generated_application.py` checks:
//! - a `[patch.crates-io]` path entry for every ZenoFCIS package the
//!   application needs, directly or through the normal and build
//!   dependencies of other ZenoFCIS packages;
//! - the source tree's `Cargo.lock`, so that Cargo keeps the tree's external
//!   versions when the application's first build adds it to the lock;
//! - the source tree's `rust-toolchain.toml`, when it has one.
//!
//! The source tree is the one `--source` names, else the tree this CLI was
//! built from while it still exists. It must be a Cargo workspace whose
//! members provide every needed package at this CLI's version.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The checkout this CLI was built from, two levels above this crate's
/// manifest as in this repository's `crates/zeno-fcis-cli`, unless the build
/// set `ZENO_FCIS_BUILD_TREE`. A path there names the tree; an empty value
/// records none, so that a release binary names no build directory and
/// `new` needs `--source`. The value is a constant, so a build without a
/// tree holds no build path at all.
const BUILD_TREE: Option<&str> = match option_env!("ZENO_FCIS_BUILD_TREE") {
    Some(tree) if tree.is_empty() => None,
    Some(tree) => Some(tree),
    None => Some(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
};
/// Largest manifest or lockfile read.
const FILE_LIMIT: u64 = 4 * 1024 * 1024;

/// The files a binding adds to a generated application.
#[derive(Debug)]
pub(crate) struct Binding {
    /// The source tree, canonical.
    pub(crate) tree: PathBuf,
    /// Appended to the generated `Cargo.toml`.
    pub(crate) patch: String,
    /// The source tree's `Cargo.lock`, byte for byte.
    pub(crate) lock: Vec<u8>,
    /// The source tree's `rust-toolchain.toml`, byte for byte, if any.
    pub(crate) toolchain: Option<Vec<u8>>,
}

/// Why no binding was made.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Refusal {
    /// No `--source` was given, and the tree this CLI was built from is
    /// gone or its build recorded none.
    NoTree(String),
    /// The source tree cannot bind the application.
    Unusable(String),
}

impl Refusal {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::NoTree(message) | Self::Unusable(message) => message,
        }
    }
}

/// The binding of an application whose manifest is `manifest` to the source
/// tree `source`, or to the tree this CLI was built from.
///
/// # Errors
/// Refuses when there is no source tree, or when it does not provide every
/// needed package at this CLI's version and a `Cargo.lock`.
pub(crate) fn bind(manifest: &str, source: Option<&Path>) -> Result<Binding, Refusal> {
    let named = source.is_some();
    let no_tree = |reason: String| {
        Refusal::NoTree(format!(
            "{reason}; name a ZenoFCIS source tree of version {} with --source",
            env!("CARGO_PKG_VERSION")
        ))
    };
    let tree = match source.or(BUILD_TREE.map(Path::new)) {
        Some(tree) => fs::canonicalize(tree).map_err(|error| match source {
            Some(path) => Refusal::Unusable(format!("--source {}: {error}", path.display())),
            None => no_tree(format!(
                "the ZenoFCIS source tree this CLI was built from, {}, no longer exists",
                tree.display()
            )),
        })?,
        None => {
            return Err(no_tree(
                "this CLI was built without a source tree".to_owned(),
            ));
        }
    };
    let refuse = |reason: String| {
        let hint = if named {
            String::new()
        } else {
            "; name another with --source".to_owned()
        };
        Refusal::Unusable(format!(
            "the ZenoFCIS source tree {} cannot bind the application: {reason}{hint}",
            tree.display()
        ))
    };
    let workspace = read_manifest(&tree.join("Cargo.toml")).map_err(&refuse)?;
    if workspace.members.is_empty() {
        return Err(refuse(
            "its Cargo.toml lists no workspace members".to_owned(),
        ));
    }
    let mut packages: BTreeMap<String, (PathBuf, Manifest)> = BTreeMap::new();
    for member in &workspace.members {
        let directory = fs::canonicalize(tree.join(member))
            .map_err(|error| refuse(format!("workspace member {member}: {error}")))?;
        let package = read_manifest(&directory.join("Cargo.toml")).map_err(&refuse)?;
        let name = package
            .name
            .clone()
            .ok_or_else(|| refuse(format!("workspace member {member} names no package")))?;
        packages.insert(name, (directory, package));
    }
    let application = parse(manifest).map_err(|reason| refuse(format!("Cargo.toml: {reason}")))?;
    let mut pending: Vec<&String> = application.dependencies.iter().collect();
    pending.extend(&application.dev_dependencies);
    let mut selected = BTreeSet::new();
    while let Some(name) = pending.pop() {
        let Some((_, package)) = packages.get(name) else {
            if name.starts_with("zeno-fcis") {
                return Err(refuse(format!("no workspace member is the package {name}")));
            }
            continue;
        };
        if selected.insert(name.clone()) {
            pending.extend(&package.dependencies);
        }
    }
    let wanted = env!("CARGO_PKG_VERSION");
    let mut patch = String::from("\n[patch.crates-io]\n");
    for name in &selected {
        let (directory, package) = &packages[name];
        let version = if package.inherits_version {
            workspace.workspace_version.as_deref()
        } else {
            package.version.as_deref()
        };
        if version != Some(wanted) {
            return Err(refuse(format!(
                "it provides {name} {}, and this CLI generates applications for {wanted}",
                version.unwrap_or("of no version")
            )));
        }
        let path = directory.to_str().ok_or_else(|| {
            refuse(format!(
                "the path {} is not UTF-8 text",
                directory.display()
            ))
        })?;
        let path = serde_json::to_string(path).map_err(|error| refuse(error.to_string()))?;
        patch.push_str(&format!("{name} = {{ path = {path} }}\n"));
    }
    let lock = read_bounded(&tree.join("Cargo.lock"))
        .map_err(|error| refuse(format!("Cargo.lock: {error}")))?;
    let toolchain = match read_bounded(&tree.join("rust-toolchain.toml")) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(refuse(format!("rust-toolchain.toml: {error}"))),
    };
    Ok(Binding {
        tree,
        patch,
        lock,
        toolchain,
    })
}

fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    let length = fs::metadata(path)?.len();
    if length > FILE_LIMIT {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("larger than {FILE_LIMIT} bytes"),
        ));
    }
    fs::read(path)
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let bytes = read_bounded(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    parse(&text).map_err(|reason| format!("{}: {reason}", path.display()))
}

/// What the binding reads from one Cargo manifest.
#[derive(Debug, Default, Eq, PartialEq)]
struct Manifest {
    /// `[package]` `name`.
    name: Option<String>,
    /// `[package]` `version`, unless it is inherited from the workspace.
    version: Option<String>,
    /// `version.workspace = true`.
    inherits_version: bool,
    /// The packages normal and build dependency tables name, renames applied.
    dependencies: BTreeSet<String>,
    /// The packages dev-dependency tables name.
    dev_dependencies: BTreeSet<String>,
    /// `[workspace]` `members`.
    members: Vec<String>,
    /// `[workspace.package]` `version`.
    workspace_version: Option<String>,
}

/// The dependency kind a table path declares, with the dependency it names
/// when it is a `[dependencies.NAME]` table.
fn dependency_table(table: &[String]) -> Option<(bool, Option<&str>)> {
    let rest = match table {
        [target, _, rest @ ..] if target == "target" => rest,
        rest => rest,
    };
    let (kind, name) = match rest {
        [kind] => (kind, None),
        [kind, name] => (kind, Some(name.as_str())),
        _ => return None,
    };
    match kind.as_str() {
        "dependencies" | "build-dependencies" => Some((false, name)),
        "dev-dependencies" => Some((true, name)),
        _ => None,
    }
}

/// Reads the manifest keys the binding needs. The parser accepts the TOML
/// that Cargo manifests use: tables and arrays of tables, dotted and quoted
/// keys, strings, arrays, inline tables and other scalars, which it skips.
/// Multi-line strings are refused, since no read key holds one.
fn parse(text: &str) -> Result<Manifest, String> {
    let mut manifest = Manifest::default();
    for (table, entries) in Parser::new(text).document()? {
        let text_of = |value: &Value| match value {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        };
        if let Some((dev, named)) = dependency_table(&table) {
            let mut names: BTreeMap<String, String> = BTreeMap::new();
            if let Some(name) = named {
                names.insert(name.to_owned(), name.to_owned());
            }
            for (key, value) in &entries {
                let (declared, field) = match (named, key.as_slice()) {
                    (Some(name), [field]) => (name.to_owned(), Some(field.as_str())),
                    (None, [name]) => (name.clone(), None),
                    (None, [name, field]) => (name.clone(), Some(field.as_str())),
                    _ => return Err(format!("dependency key `{}` is not read", key.join("."))),
                };
                let renamed = match (field, value) {
                    (Some("package"), value) => text_of(value),
                    (None, Value::Table(fields)) => fields
                        .iter()
                        .find(|(field, _)| field.as_slice() == ["package"])
                        .and_then(|(_, value)| text_of(value)),
                    _ => None,
                };
                let entry = names.entry(declared.clone()).or_insert(declared);
                if let Some(renamed) = renamed {
                    *entry = renamed;
                }
            }
            let into = if dev {
                &mut manifest.dev_dependencies
            } else {
                &mut manifest.dependencies
            };
            into.extend(names.into_values());
            continue;
        }
        for (key, value) in &entries {
            match (table.as_slice(), key.as_slice()) {
                ([package], [field]) if package == "package" && field == "name" => {
                    manifest.name = text_of(value);
                }
                ([package], [field]) if package == "package" && field == "version" => {
                    manifest.version = text_of(value);
                }
                ([package], [field, inherited])
                    if package == "package" && field == "version" && inherited == "workspace" =>
                {
                    manifest.inherits_version = *value == Value::Other("true".to_owned());
                }
                ([workspace], [field]) if workspace == "workspace" && field == "members" => {
                    let Value::Array(members) = value else {
                        return Err("workspace members must be an array".to_owned());
                    };
                    manifest.members = members
                        .iter()
                        .map(|member| {
                            text_of(member)
                                .ok_or_else(|| "workspace members must be strings".to_owned())
                        })
                        .collect::<Result<_, _>>()?;
                    if let Some(glob) = manifest
                        .members
                        .iter()
                        .find(|member| member.contains(['*', '?', '[']))
                    {
                        return Err(format!(
                            "workspace member `{glob}` is a pattern; list each member"
                        ));
                    }
                }
                ([workspace, package], [field])
                    if workspace == "workspace" && package == "package" && field == "version" =>
                {
                    manifest.workspace_version = text_of(value);
                }
                _ => {}
            }
        }
    }
    Ok(manifest)
}

/// A manifest value, as far as the binding reads it.
#[derive(Debug, Eq, PartialEq)]
enum Value {
    Text(String),
    Array(Vec<Value>),
    Table(Vec<(Vec<String>, Value)>),
    /// A boolean, number or date, as written.
    Other(String),
}

/// Every table in document order, with its key-value entries. The root
/// table has the empty path.
type Document = Vec<(Vec<String>, Vec<(Vec<String>, Value)>)>;

struct Parser<'t> {
    text: &'t [u8],
    at: usize,
}

impl<'t> Parser<'t> {
    fn new(text: &'t str) -> Self {
        Self {
            text: text.strip_prefix('\u{feff}').unwrap_or(text).as_bytes(),
            at: 0,
        }
    }

    fn error(&self, reason: &str) -> String {
        let line = self.text[..self.at.min(self.text.len())]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1;
        format!("line {line}: {reason}")
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    /// Skips spaces and tabs, comments, and newlines when `newlines` is set.
    fn skip(&mut self, newlines: bool) {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' => self.at += 1,
                b'\r' | b'\n' if newlines => self.at += 1,
                b'#' => {
                    while self.peek().is_some_and(|byte| byte != b'\n') {
                        self.at += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", char::from(byte))))
        }
    }

    /// The end of a line: only a comment may follow.
    fn end_of_line(&mut self) -> Result<(), String> {
        self.skip(false);
        match self.peek() {
            None | Some(b'\n') => Ok(()),
            Some(b'\r') if self.text.get(self.at + 1) == Some(&b'\n') => Ok(()),
            _ => Err(self.error("unexpected text after the value")),
        }
    }

    fn document(mut self) -> Result<Document, String> {
        let mut tables: Document = vec![(Vec::new(), Vec::new())];
        loop {
            self.skip(true);
            match self.peek() {
                None => return Ok(tables),
                Some(b'[') => {
                    self.at += 1;
                    let array = self.peek() == Some(b'[');
                    if array {
                        self.at += 1;
                    }
                    self.skip(false);
                    let path = self.key()?;
                    self.skip(false);
                    self.expect(b']')?;
                    if array {
                        self.expect(b']')?;
                    }
                    self.end_of_line()?;
                    tables.push((path, Vec::new()));
                }
                Some(_) => {
                    let entry = self.entry()?;
                    self.end_of_line()?;
                    if let Some((_, entries)) = tables.last_mut() {
                        entries.push(entry);
                    }
                }
            }
        }
    }

    fn entry(&mut self) -> Result<(Vec<String>, Value), String> {
        let key = self.key()?;
        self.skip(false);
        self.expect(b'=')?;
        self.skip(false);
        Ok((key, self.value()?))
    }

    /// A dotted key of bare and quoted parts.
    fn key(&mut self) -> Result<Vec<String>, String> {
        let mut parts = Vec::new();
        loop {
            self.skip(false);
            let part = match self.peek() {
                Some(quote @ (b'"' | b'\'')) => self.string(quote)?,
                _ => {
                    let start = self.at;
                    while self.peek().is_some_and(|byte| {
                        byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
                    }) {
                        self.at += 1;
                    }
                    if start == self.at {
                        return Err(self.error("expected a key"));
                    }
                    String::from_utf8_lossy(&self.text[start..self.at]).into_owned()
                }
            };
            parts.push(part);
            self.skip(false);
            if self.peek() == Some(b'.') {
                self.at += 1;
            } else {
                return Ok(parts);
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(quote @ (b'"' | b'\'')) => Ok(Value::Text(self.string(quote)?)),
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip(true);
                    if self.peek() == Some(b']') {
                        self.at += 1;
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value()?);
                    self.skip(true);
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {}
                        _ => return Err(self.error("expected `,` or `]` in an array")),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut fields = Vec::new();
                loop {
                    self.skip(false);
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                        return Ok(Value::Table(fields));
                    }
                    fields.push(self.entry()?);
                    self.skip(false);
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {}
                        _ => return Err(self.error("expected `,` or `}` in an inline table")),
                    }
                }
            }
            Some(_) => {
                let start = self.at;
                while self.peek().is_some_and(|byte| {
                    !matches!(
                        byte,
                        b',' | b']' | b'}' | b'#' | b'\r' | b'\n' | b' ' | b'\t'
                    )
                }) {
                    self.at += 1;
                }
                if start == self.at {
                    return Err(self.error("expected a value"));
                }
                Ok(Value::Other(
                    String::from_utf8_lossy(&self.text[start..self.at]).into_owned(),
                ))
            }
            None => Err(self.error("expected a value")),
        }
    }

    /// A basic (`"`) or literal (`'`) string on one line.
    fn string(&mut self, quote: u8) -> Result<String, String> {
        if self.text[self.at..].starts_with(&[quote; 3]) {
            return Err(self.error("multi-line strings are not read"));
        }
        self.at += 1;
        let mut bytes = Vec::new();
        loop {
            let byte = match self.peek() {
                None | Some(b'\n') => return Err(self.error("unterminated string")),
                Some(byte) => byte,
            };
            self.at += 1;
            match byte {
                _ if byte == quote => break,
                b'\\' if quote == b'"' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.at += 1;
                    match escaped {
                        b'"' | b'\\' => bytes.push(escaped),
                        b'n' => bytes.push(b'\n'),
                        b't' => bytes.push(b'\t'),
                        b'r' => bytes.push(b'\r'),
                        _ => return Err(self.error("escape sequence not read")),
                    }
                }
                _ => bytes.push(byte),
            }
        }
        String::from_utf8(bytes).map_err(|_| self.error("string is not UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn workspace_and_normalized_manifests_read_as_cargo_writes_them() {
        let workspace = parse(
            "[workspace]\nresolver = \"3\"\nmembers = [\n    \"crates/a\", # first\n    'crates/b',\n]\n\n\
             [workspace.package]\nversion = \"1.1.0\"\n[workspace.lints.clippy]\nall = { level = \"warn\", priority = -1 }\n",
        );
        assert_eq!(
            workspace,
            Ok(Manifest {
                members: vec!["crates/a".to_owned(), "crates/b".to_owned()],
                workspace_version: Some("1.1.0".to_owned()),
                ..Manifest::default()
            })
        );
        // The repository's form: inline tables and inherited fields.
        let member = parse(
            "[package]\nname = \"zeno-fcis-x\"\nversion.workspace = true\n\
             description = \"a # b\"\n[features]\nstd = [\n  \"zeno-fcis-y/std\",\n]\n\
             [dependencies]\nzeno-fcis-y = { version = \"=1.1.0\", path = \"../y\", features = [\"std\"] }\n\
             renamed = { package = \"zeno-fcis-z\", version = \"=1.1.0\" }\nserde = \"1\"\n\
             [target.'cfg(unix)'.dependencies]\nnix = { version = \"=0.30.1\" }\n\
             [build-dependencies]\nzeno-fcis-b = \"=1.1.0\"\n\
             [dev-dependencies]\nzeno-fcis-dev = \"=1.1.0\"\n[[bench]]\nname = \"speed\"\n",
        );
        let expected = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(
            member,
            Ok(Manifest {
                name: Some("zeno-fcis-x".to_owned()),
                inherits_version: true,
                dependencies: expected(&[
                    "nix",
                    "serde",
                    "zeno-fcis-b",
                    "zeno-fcis-y",
                    "zeno-fcis-z"
                ]),
                dev_dependencies: expected(&["zeno-fcis-dev"]),
                ..Manifest::default()
            })
        );
        // The form `cargo package` writes: one table per dependency.
        let normalized = parse(
            "# generated\n[package]\nname = \"zeno-fcis-shell\"\nversion = \"1.1.0\"\n\
             [dependencies.zeno-fcis-codec]\nversion = \"=1.1.0\"\ndefault-features = false\n\
             [dependencies.alias]\npackage = \"zeno-fcis-plan\"\nversion = \"=1.1.0\"\n\
             [target.\"cfg(unix)\".build-dependencies.zeno-fcis-value]\nversion = \"=1.1.0\"\n\
             [dev-dependencies.zeno-fcis-core]\nversion = \"=1.1.0\"\n[lints.clippy.all]\nlevel = \"warn\"\n",
        );
        assert_eq!(
            normalized,
            Ok(Manifest {
                name: Some("zeno-fcis-shell".to_owned()),
                version: Some("1.1.0".to_owned()),
                dependencies: expected(&["zeno-fcis-codec", "zeno-fcis-plan", "zeno-fcis-value"]),
                dev_dependencies: expected(&["zeno-fcis-core"]),
                ..Manifest::default()
            })
        );
        for (text, reason) in [
            (
                "a = \"\"\"\nx\n\"\"\"\n",
                "line 1: multi-line strings are not read",
            ),
            ("a = \"x\ny = 1\n", "line 1: unterminated string"),
            (
                "[workspace]\nmembers = [\"crates/*\"]\n",
                "workspace member `crates/*` is a pattern; list each member",
            ),
            (
                "[dependencies]\na.b.c = 1\n",
                "dependency key `a.b.c` is not read",
            ),
            ("a = 1 b\n", "line 1: unexpected text after the value"),
        ] {
            assert_eq!(parse(text).err().as_deref(), Some(reason), "{text}");
        }
    }

    #[test]
    fn the_build_tree_binds_each_needed_package_and_nothing_else() {
        let manifest = include_str!("../contract-app/Cargo.toml.in")
            .replace("{package}", "bound")
            .replace("{version}", env!("CARGO_PKG_VERSION"));
        let binding = bind(&manifest, None).unwrap_or_else(|refusal| panic!("{refusal:?}"));
        let repository = fs::canonicalize(repository()).unwrap_or_else(|error| panic!("{error}"));
        let entries: Vec<(&str, &str)> = binding
            .patch
            .lines()
            .skip(2)
            .map(|line| line.split_once(" = ").unwrap_or_else(|| panic!("{line}")))
            .collect();
        assert!(binding.patch.starts_with("\n[patch.crates-io]\n"));
        let names: Vec<&str> = entries.iter().map(|(name, _)| *name).collect();
        for needed in [
            "zeno-fcis-codec",
            "zeno-fcis-core",
            "zeno-fcis-shell-sqlite",
            "zeno-fcis-synthesis",
            "zeno-fcis-value",
        ] {
            assert!(names.contains(&needed), "{needed}");
        }
        // Neither this CLI nor a package only tests need is bound.
        for unneeded in [
            "zeno-fcis-cli",
            "zeno-fcis-generated-code-tests",
            "zeno-fcis",
        ] {
            assert!(!names.contains(&unneeded), "{unneeded}");
        }
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        for (name, path) in entries {
            let expected = repository.join("crates").join(name);
            assert_eq!(
                path,
                format!("{{ path = {:?} }}", expected.display().to_string()),
                "{name}"
            );
        }
        assert_eq!(
            binding.lock,
            fs::read(repository.join("Cargo.lock")).unwrap_or_else(|error| panic!("{error}"))
        );
        assert_eq!(
            binding.toolchain,
            fs::read(repository.join("rust-toolchain.toml")).ok()
        );
        // The same binding through --source.
        let named =
            bind(&manifest, Some(&repository)).unwrap_or_else(|refusal| panic!("{refusal:?}"));
        assert_eq!(named.patch, binding.patch);
    }

    #[test]
    fn a_source_tree_that_cannot_bind_the_application_is_refused() {
        let manifest = "[package]\nname = \"app\"\n[dependencies]\nzeno-fcis-core = \"=1.1.0\"\n";
        let scratch =
            std::env::temp_dir().join(format!("zeno-fcis-binding-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        let write = |path: &str, text: &str| {
            let path = scratch.join(path);
            fs::create_dir_all(path.parent().unwrap_or(&scratch))
                .unwrap_or_else(|error| panic!("{error}"));
            fs::write(path, text).unwrap_or_else(|error| panic!("{error}"));
        };
        let refusal = |source: &Path| {
            bind(manifest, Some(source))
                .err()
                .unwrap_or_else(|| panic!("{} must be refused", source.display()))
        };
        let missing = refusal(&scratch.join("absent"));
        assert!(
            matches!(&missing, Refusal::Unusable(message) if message.starts_with("--source ")),
            "{missing:?}"
        );
        write("Cargo.toml", "[workspace]\nmembers = [\"core\"]\n");
        write(
            "core/Cargo.toml",
            "[package]\nname = \"zeno-fcis-core\"\nversion = \"0.9.0\"\n",
        );
        let older = refusal(&scratch).message().to_owned();
        assert!(
            older.ends_with(&format!(
                "it provides zeno-fcis-core 0.9.0, and this CLI generates applications for {}",
                env!("CARGO_PKG_VERSION")
            )),
            "{older}"
        );
        write(
            "core/Cargo.toml",
            &format!(
                "[package]\nname = \"zeno-fcis-core\"\nversion = \"{}\"\n",
                env!("CARGO_PKG_VERSION")
            ),
        );
        assert!(refusal(&scratch).message().contains(": Cargo.lock: "));
        write("Cargo.lock", "version = 4\n");
        let bound = bind(manifest, Some(&scratch)).unwrap_or_else(|refusal| panic!("{refusal:?}"));
        assert_eq!(bound.lock, b"version = 4\n");
        assert_eq!(bound.toolchain, None);
        let other = "[package]\nname = \"app\"\n[dependencies]\nzeno-fcis-value = \"=1.1.0\"\n";
        assert!(bind(other, Some(&scratch)).err().is_some_and(|refusal| {
            refusal
                .message()
                .ends_with("no workspace member is the package zeno-fcis-value")
        }));
        let _ = fs::remove_dir_all(&scratch);
    }
}
