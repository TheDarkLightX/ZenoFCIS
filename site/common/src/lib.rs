//! What every demo module shares.
//!
//! A template's demo crate implements [`Application`] for the application
//! `zeno-fcis new` wrote: the types its authority is built over, its exact
//! genesis, and the mapping from the page's request to its typed command and
//! context. Everything else is here: the step over the library's in-memory
//! shell, which follows each template's `invoke` ([`demo`]); the JSON report,
//! with every value named as `project.zeno` names it ([`render`]); the reader
//! of the page's request fields ([`request`]); and the C ABI the page calls
//! ([`abi`]), which exchanges bounded scalar words and keeps all buffers private.
//!
//! The library types a demo crate names in its `Application` impl are
//! re-exported here, so a demo crate depends on its application and on this
//! crate alone.

pub mod abi;
pub mod application;
pub mod demo;
pub mod render;
pub mod request;

pub use application::{Application, Authority, Shell, Transition};
pub use demo::{Demo, DemoInstance, Refusal, Stage};
pub use render::Names;
pub use request::Request;
pub use serde_json::{self, Map, Value as Json};
pub use zeno_fcis_crypto::RustCryptoSha256;
pub use zeno_fcis_schema::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, ValidationLimits};
pub use zeno_fcis_value::Value;
