//! The contract a template's demo crate fulfils.

use crate::render::Names;
use serde_json::{Map, Value as Json};
use std::fmt::Debug;
use zeno_fcis_authority::{
    AuthorizedShellState, CatalogAuthorizedTransition, CatalogCommitAuthority,
    CatalogTransitionProgram,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::ProjectLawEngine;
use zeno_fcis_schema::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope};
use zeno_fcis_value::Value;

/// The template's commit authority: the type its own `Authority` alias names.
pub type Authority<A> = CatalogCommitAuthority<
    RustCryptoSha256,
    <A as Application>::Program,
    <A as Application>::Laws,
    <A as Application>::Destination,
>;

/// The in-memory shell, pinned to the same provider, program, laws, and
/// destination type as the template's authority.
pub type Shell<A> = AuthorizedShellState<
    RustCryptoSha256,
    <A as Application>::Program,
    <A as Application>::Laws,
    <A as Application>::Destination,
>;

/// A decision the authority authorized to commit.
pub type Transition<A> = CatalogAuthorizedTransition<
    RustCryptoSha256,
    <A as Application>::Program,
    <A as Application>::Laws,
    <A as Application>::Destination,
>;

/// A generated application, as its demo crate presents it.
///
/// Each method is a thin call into the application crate, so that the demo
/// runs the template exactly as `zeno-fcis new` writes it. The only code a
/// demo crate writes on its own is [`parse`](Self::parse), the mapping from
/// the page's request to the template's typed command and context.
pub trait Application: 'static {
    /// The template's name, as `zeno-fcis new --template` takes it. The
    /// digests that identify the principal, the authentication evidence, and
    /// each replay are labelled `example/<NAME>/...`, as in the template's
    /// `invoke`.
    const NAME: &'static str;
    /// The template's program: the `P` of its `Authority`.
    type Program: CatalogTransitionProgram<RustCryptoSha256, Error: Debug>;
    /// The template's law checker: the `L` of its `Authority`.
    type Laws: ProjectLawEngine;
    /// The template's delivery adapter, the `I` of its `Authority`; nothing
    /// delivers in a page, and the authority holds it only as a marker.
    type Destination;
    /// The generated command type.
    type Command;
    /// The generated context type.
    type Context;

    /// The template's `authority()`.
    ///
    /// # Errors
    ///
    /// The template's own refusal to build its authority, rendered as text.
    fn authority() -> Result<Authority<Self>, String>;

    /// The names of the reasons, fields, variants, and channels of
    /// `project.zeno`, and of every law in the manifest.
    ///
    /// # Errors
    ///
    /// The template's own refusal to parse its project or build its manifest.
    fn names() -> Result<Names, String>;

    /// The exact genesis the template's `create` writes, admitted against
    /// the schema through the generated bindings.
    ///
    /// # Errors
    ///
    /// A genesis the schema refuses.
    fn genesis() -> Result<SchemaAdmittedEnvelope, String>;

    /// The shell's current state, read back through the generated bindings
    /// and re-admitted, as the template's `invoke` does with a snapshot.
    ///
    /// # Errors
    ///
    /// A state the bindings cannot read or the schema refuses.
    fn admit_state(state: Value) -> Result<SchemaAdmittedEnvelope, String>;

    /// The page's request: `command` names the command, and the other fields
    /// are the command's and the context's, in the README's words. Every
    /// field a command reads is required, and no other field is allowed.
    ///
    /// # Errors
    ///
    /// A field missing, mis-typed, out of its choices, or unexpected.
    fn parse(request: &Map<String, Json>) -> Result<(Self::Command, Self::Context), String>;

    /// Schema admission of a typed command and context through the
    /// generated bindings.
    ///
    /// # Errors
    ///
    /// A value the schema refuses.
    fn admit(
        command: &Self::Command,
        context: &Self::Context,
    ) -> Result<(SchemaAdmittedTypeEnvelope, SchemaAdmittedTypeEnvelope), String>;
}
