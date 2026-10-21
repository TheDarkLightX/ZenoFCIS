/// The checkout this CLI was built from, two levels above this crate's
/// manifest as in this repository's `crates/zeno-fcis-cli`, unless the build
/// set `ZENO_FCIS_BUILD_TREE`. A path there names the tree; an empty value
/// records none, so that a release binary names no build directory and
/// `new` needs `--source`. The value is a constant, so a build without a
/// tree holds no build path at all.
pub(crate) const BUILD_TREE: Option<&str> = match option_env!("ZENO_FCIS_BUILD_TREE") {
    Some(tree) if tree.is_empty() => None,
    Some(tree) => Some(tree),
    None => Some(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
};
