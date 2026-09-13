# Release privacy and CLI file handling

This patch follows the published `v1.0.0` source. It changes release hygiene and
CLI failure handling, without changing semantic encodings, contract acceptance,
authorization, or the Lean toolchain.

The publication audit found personal home-directory paths in retained review
records and exported Cargo metadata, plus a personal Git email in copied
workflow records. Public copies must use descriptive path and email
placeholders. Sanitized evidence remains an observation of the original run;
it is not a new run, and its content hashes must be refreshed. A recursive
release check covers source, package archives, nested bundles, and evidence.
It reports locations and categories without printing the matched private data.
An optional private marker file covers known owner identifiers without putting
them in the repository or command line. Packaging inherits the marker-file
setting; malformed or unreadable marker files fail the check.
Pattern checks cannot establish that every possible secret is absent.

The CLI audit reproduced a blocked open of a named pipe at a generated-artifact
path. On Unix, opening with nonblocking flags before checking the opened file's
type prevents that hang while preserving ordinary-file and symlink-to-file
behavior. Rejection still uses the existing JSON error and exit classes.

A write-size limit also reproduced an abandoned temporary artifact and a partial
new project file. One exclusive-create operation will own write, sync, and
failure cleanup. Replacement reuses that operation before rename. Existing
destination files remain intact if writing their replacement fails. This is
per-file cleanup, not a transaction over the entire generated directory.

Regression evidence must include regular files, named pipes, a failed write,
preservation of an existing destination, and retry after a failed first project
file. Existing CLI and acceptance checks cover unchanged successful behavior.

The signed V1 tag and immutable registry packages cannot be silently rewritten.
Historical removal and a subsequent patch release are separate publication
decisions from correcting the current source and public evidence copies.
