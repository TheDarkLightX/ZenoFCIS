//! Trusted adapter between shell-effects port data and the V2 Authority.
//!
//! A shell program carries [`PublicationData`]: the original inputs, the
//! Authority identity and the publication subject. That data grants nothing.
//! Before a commit, the trusted driver rebuilds the genuine publication from
//! the inputs through Authority and refuses data that names another
//! Authority or another subject, so replay and simulation can never mint a
//! publication. The V2 shell's own APIs are unchanged; [`production`]
//! mounts them behind the ports.

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_shell::effects::{codec, port::PublicationData};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, Publication, PublicationOutcome},
    v2_composition::{DescriptorData, Raw},
};

/// Durable reservation byte storage; restoration remains a pure shell function.
pub mod durable;
/// The production SQLite handler behind the shell-effects ports.
pub mod production;

/// A store's identity: SHA-256 of its original genesis envelope, the full
/// identity of its Authority and the delivery interpreter's identity. A
/// handle adds only its number. It names what the store was created from,
/// not a file: two stores created from the same genesis share it.
#[must_use]
pub fn store_identity(genesis: &[u8], authority: &[u8], interpreter: Hash32) -> Hash32 {
    let mut out = Vec::new();
    codec::bytes(&mut out, b"zeno-fcis/shell-effects/1 store");
    codec::bytes(&mut out, genesis);
    codec::bytes(&mut out, authority);
    codec::hash(&mut out, interpreter);
    RustCryptoSha256::hash(&out)
}

/// Why publication data did not bind to a genuine publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unbound {
    /// The data names another Authority.
    Identity,
    /// The Authority computes another subject for these inputs.
    Subject,
    /// The inputs do not produce a committing publication.
    NotCommitting,
}

/// The port data a program carries for a genuine publication.
#[must_use]
pub fn publication_data(publication: &Publication<'_>) -> PublicationData {
    let raw = publication.evaluation().raw();
    PublicationData {
        prestate: raw.state.to_vec(),
        command: raw.command.to_vec(),
        context: raw.context.to_vec(),
        identity: publication.identity().to_vec(),
        subject: publication.subject().to_vec(),
    }
}

/// Rebuilds the genuine publication from `data` through `authority`.
///
/// Refuses data whose Authority identity or subject differs from what the
/// Authority computes for the same inputs.
pub fn bind_publication<'a, 'p, D: DescriptorData<'p> + 'p>(
    authority: &'a Authority<'p, D>,
    data: &'a PublicationData,
) -> Result<Publication<'a>, Unbound> {
    if authority.identity() != data.identity.as_slice() {
        return Err(Unbound::Identity);
    }
    match authority.publish(Raw {
        state: &data.prestate,
        command: &data.command,
        context: &data.context,
    }) {
        PublicationOutcome::Commit(publication) if publication.subject() == data.subject => {
            Ok(publication)
        }
        PublicationOutcome::Commit(_) => Err(Unbound::Subject),
        _ => Err(Unbound::NotCommitting),
    }
}

/// The port data of an audited commit, rebuilt from its original inputs
/// through the Authority that published it.
pub fn audited_publication<'p, D: DescriptorData<'p> + 'p>(
    authority: &Authority<'p, D>,
    prestate: &[u8],
    command: &[u8],
    context: &[u8],
) -> Result<PublicationData, Unbound> {
    match authority.publish(Raw {
        state: prestate,
        command,
        context,
    }) {
        PublicationOutcome::Commit(publication) => Ok(publication_data(&publication)),
        _ => Err(Unbound::NotCommitting),
    }
}
