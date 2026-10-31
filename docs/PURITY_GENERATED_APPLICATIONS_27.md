# Purity check on generated applications (design, F27-30)

Status: design only, not implemented in 2.7.0. Nothing here changes `purity` yet.

## The journey that fails today

Every template README tells the user of a generated application to run

```sh
zeno-fcis purity src/program.rs src/laws.rs
```

inside the application directory, and the acceptance scenario `determinism`
runs `purity` on the in-tree templates, which have no `Cargo.toml`. A
generated application does have one: `new --contract` writes a
`[patch.crates-io]` table with one `name = { path = "..." }` entry for every
ZenoFCIS package the application's manifest depends on, pointing into the
source tree the CLI was built from (`crates/zeno-fcis-cli/src/binding.rs`).
The purity manifest reader refuses every `[patch]` table (see the
`manifest_forms_that_could_hide_a_dependency_or_add_a_target_are_refused`
unit test in `crates/zeno-fcis-cli/src/purity.rs`), because a patch can put
other code under a trusted dependency's name. So `zeno-fcis purity .` on a
generated application always exits 3 or 1 with an unrecognized manifest, and
the file-list form the README gives skips the crate-level confinement check.
The journey is untested in acceptance, which is why the gap stayed open.

## Trust rule

A `[patch]` entry is admitted only when it is exactly the binding this CLI
would write for its own build tree. Everything else about `[patch]` stays
refused. In full:

1. The entry's table is `[patch.crates-io]`; no other patch target.
2. The entry's key is a ZenoFCIS package name that the binding selects for
   this application (`binding.rs` computes that set from the application's
   dependencies); unknown names refuse.
3. The entry has exactly one field, `path`, a plain string; `package`,
   `version`, `git`, `branch`, `rev`, `features`, `default-features`,
   `optional`, inline tables with more fields, dotted keys and multi-line
   strings refuse, as they do for `[dependencies]` today.
4. The path is the package directory the current build tree binds for that
   name, compared after canonicalisation of both sides; a path into another
   tree, a symbolic link, or a relative path refuses.
5. The package at that path declares that name in its `[package]` table and a
   library target whose root is `src/lib.rs`; its version is the CLI's
   version, as the binding already requires.
6. The build tree is the one `ZENO_FCIS_BUILD_TREE` names, or the compiled-in
   default while it exists; when neither is available, every `[patch]`
   refuses as today. The tree's identity is reported, not assumed.

What the rule does not do: it does not read, hash or check the patched
package's sources beyond the manifest fields above. Those sources are the
library this CLI was built from; the claim is "this application's
dependencies resolve to the same ZenoFCIS packages this CLI binds", not
"those packages are pure". Confinement of the library crates remains the
library's own purity and proof evidence.

## Behaviour

- `zeno-fcis purity APP_DIR` on a generated application reads `Cargo.toml`,
  applies the rule to each `[patch.crates-io]` entry, then runs the existing
  confinement check with the patched names treated as the semantic crates
  they name. The report gains a `patch_binding` object: the build tree, each
  admitted entry (name, path) and each refused entry with its reason. Status
  stays `clean`/`confined`, `violations` or `unreadable` as documented.
- A refused patch entry is `violations` with rule `manifest-patch-refused`,
  not `unreadable`: the manifest was read completely and the entry is the
  finding.
- The resolved route (`check_resolved_paths`, which the mandatory V2
  inspection in `main.rs` runs) uses the same admission, so that inspection
  and the user-facing command agree on which dependencies are in scope.
- The template READMEs change to `zeno-fcis purity .` with the expected
  status, and keep the file-list form as the fallback for a tree without the
  binding.

## Acceptance

Add to the `determinism` scenario in `tools/atdd.py`:

1. Build one application from a template with `new APP --contract DIR
   --source .` into scratch; run `zeno-fcis purity APP --format json`;
   require status `confined` (or `clean` when the template's crate is not
   confinable; record which) and `patch_binding.admitted` naming every
   ZenoFCIS dependency.
2. Planted controls, each expected `violations` with `manifest-patch-refused`:
   a patch entry whose path points at a copy of the package outside the
   build tree; an entry with an extra `package = "..."` field; an entry under
   `[patch."https://example.invalid/index"]`; a `[patch.crates-io]` entry for
   a name the binding does not select.
3. `ZENO_FCIS_BUILD_TREE` unset and no compiled-in tree: the same application
   refuses every entry, as 2.7.0 does.
4. The resolved route on the same application reports the same
   `patch_binding`.

Unit tests in `purity.rs` cover rule items 1 to 5 one at a time on manifest
text, beside the existing refused forms.

## Scope and what stays open

The result is a static check on the listed sources and a manifest-level
binding check; it is not a proof of determinism and does not verify the
patched sources. `RUSTFLAGS`, `.cargo/config.toml` and `--extern` overrides
stay outside its scope, as the existing report states. Cross-tree patches
(an application bound to a different ZenoFCIS checkout than the CLI) remain
refused by design; rebuild the application against the CLI's tree.
