//! The contract a template's demo crate fulfils.

use crate::render::Names;
use serde_json::{Map, Value as Json};
use zeno_fcis_schema::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope};
use zeno_fcis_value::Value;

/// The template's genuine library-owned V2 authority.
pub type Authority<'p> = zeno_fcis_authority::Authority<'p>;

/// A generated application, as its demo crate presents it.
///
/// Each method is a thin call into the application crate, so that the demo
/// runs the template exactly as `zeno-fcis new` writes it. The only code a
/// demo crate writes on its own is [`parse`](Self::parse), the mapping from
/// the page's request to the template's typed command and context.
pub trait Application: 'static {
    /// The template's name, as `zeno-fcis new --template` takes it. Browser
    /// state, publication and replay digests use `example/<NAME>/...` domains.
    /// These local identifiers do not authenticate the supplied context.
    const NAME: &'static str;
    /// The generated command type.
    type Command;
    /// The generated context type.
    type Context;

    /// Borrow the template's checked authority while its contract and
    /// descriptor remain alive. This host closure manages ownership;
    /// the library evaluates the declared decisions and laws.
    ///
    /// # Errors
    ///
    /// The template's own refusal to build its authority, rendered as text.
    fn with_authority<R>(f: impl FnOnce(&Authority<'_>) -> R) -> Result<R, String>;

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
