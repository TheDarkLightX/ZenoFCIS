//! The upgrade decision as a pure function of facts the shell established:
//! no SQLite, no I/O, no clock. The shell gathers the facts inside one
//! immediate transaction, persists the plan this module returns, and
//! otherwise writes nothing.
//!
//! An upgrade moves a store from the contract its current history segment
//! was published under to another contract with the same state schema. The
//! new contract must admit the current state through the library's genesis
//! evaluation, and the record binds both identities, that genesis
//! publication, the state root and the chain tip it extends.

use zeno_fcis_codec::Hash32;

use super::{Error, hash};

/// Magic bytes of an upgrade record, disjoint from a certificate's
/// `ZFCISV2-CERT\0`, so both hash under `V2_CHAIN` without aliasing.
const MAGIC: &[u8] = b"ZFCISV2-UPGRADE\0";

/// The new contract's genesis publication over the current state, as the
/// library produced it.
#[derive(Clone, Copy, Debug)]
pub struct Genesis<'a> {
    /// The complete sealed subject.
    pub subject: &'a [u8],
    /// The poststate the publication carries; genesis repeats its input.
    pub poststate: &'a [u8],
}

/// What the shell established about the store and the two contracts.
#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    /// One more than the upgrades the store already holds.
    pub ordinal: u64,
    /// Commit position of the validated head.
    pub sequence: u64,
    /// Root of the state at the head.
    pub root: Hash32,
    /// Hash chain tip at the head, which the record extends.
    pub previous_chain: Hash32,
    /// Identity the store's current segment was published under.
    pub from_identity: &'a [u8],
    /// Exact canonical schema bytes of that contract.
    pub from_schema: &'a [u8],
    /// Identity of the contract to upgrade to.
    pub identity: &'a [u8],
    /// Exact canonical schema bytes of that contract.
    pub schema: &'a [u8],
    /// The state at the head, as the store holds it.
    pub state: &'a [u8],
    /// The new contract's genesis publication over that state, when the
    /// library admitted one.
    pub genesis: Option<Genesis<'a>>,
}

/// Why an upgrade is refused. A refusal writes nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// The two contracts' canonical state schemas differ; changing the
    /// state's shape is a data migration, which this upgrade never performs.
    StateSchema,
    /// The contract to upgrade to is the one the store already runs.
    SameContract,
    /// The new contract's genesis laws do not admit the current state, or
    /// the admitted publication is not over it.
    Genesis,
}

/// A decided upgrade: the canonical record and the chain link it adds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    record: Vec<u8>,
    chain: Hash32,
}

impl Plan {
    /// Canonical record bytes; the chain link is their `V2_CHAIN` hash.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    /// The hash chain tip after the upgrade.
    pub fn chain(&self) -> Hash32 {
        self.chain
    }
}

/// Decides one upgrade. Refusal order: state schema, same contract, genesis.
///
/// # Errors
/// Returns the refusal, or a length that cannot be framed.
pub fn decide(facts: &Facts<'_>) -> Result<Plan, Error> {
    if facts.from_schema != facts.schema {
        return Err(Error::Upgrade(Refusal::StateSchema));
    }
    if facts.from_identity == facts.identity {
        return Err(Error::Upgrade(Refusal::SameContract));
    }
    let genesis = facts.genesis.ok_or(Error::Upgrade(Refusal::Genesis))?;
    if genesis.poststate != facts.state {
        return Err(Error::Upgrade(Refusal::Genesis));
    }
    let record = record_bytes(
        facts.ordinal,
        facts.sequence,
        facts.from_identity,
        facts.identity,
        genesis.subject,
        facts.root,
        facts.previous_chain,
    )?;
    let chain = hash(zeno_fcis_codec::domains::V2_CHAIN, &record)?;
    Ok(Plan { record, chain })
}

/// The canonical upgrade record: the magic, the ordinal and head sequence,
/// then the framed from-identity, identity, complete genesis publication,
/// state root and previous chain tip.
pub(super) fn record_bytes(
    ordinal: u64,
    sequence: u64,
    from_identity: &[u8],
    identity: &[u8],
    genesis: &[u8],
    root: Hash32,
    previous_chain: Hash32,
) -> Result<Vec<u8>, Error> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    for part in [
        from_identity,
        identity,
        genesis,
        root.as_bytes(),
        previous_chain.as_bytes(),
    ] {
        bytes.extend_from_slice(
            &u64::try_from(part.len())
                .map_err(|_| Error::Range)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(part);
    }
    Ok(bytes)
}

/// Which lineage member published each history segment. A store's segments
/// are the contracts it ran, oldest first; they must appear in the lineage in
/// that order, each later than the previous one. A segment under no lineage
/// member, or out of order, is `Error::Identity`: the store's history cannot
/// be replayed by this lineage.
///
/// # Errors
/// Returns `Error::Identity` as described.
pub fn assign(lineage: &[&[u8]], segments: &[&[u8]]) -> Result<Vec<usize>, Error> {
    let mut positions = Vec::with_capacity(segments.len());
    let mut next = 0;
    for segment in segments {
        let found = lineage[next..]
            .iter()
            .position(|member| member == segment)
            .ok_or(Error::Identity)?;
        positions.push(next + found);
        next += found + 1;
    }
    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(genesis: Option<Genesis<'a>>) -> Facts<'a> {
        Facts {
            ordinal: 1,
            sequence: 6,
            root: Hash32::new([1; 32]),
            previous_chain: Hash32::new([2; 32]),
            from_identity: b"identity-1",
            from_schema: b"schema",
            identity: b"identity-2",
            schema: b"schema",
            state: b"state",
            genesis,
        }
    }

    const ADMITTED: Genesis<'static> = Genesis {
        subject: b"sealed genesis",
        poststate: b"state",
    };

    #[test]
    fn the_record_binds_every_fact_and_the_chain_is_its_hash() {
        let plan = decide(&facts(Some(ADMITTED))).unwrap_or_else(|error| panic!("{error}"));
        let mut expected = b"ZFCISV2-UPGRADE\0".to_vec();
        expected.extend_from_slice(&1_u64.to_be_bytes());
        expected.extend_from_slice(&6_u64.to_be_bytes());
        for part in [
            &b"identity-1"[..],
            b"identity-2",
            b"sealed genesis",
            &[1; 32],
            &[2; 32],
        ] {
            expected.extend_from_slice(&(part.len() as u64).to_be_bytes());
            expected.extend_from_slice(part);
        }
        assert_eq!(plan.record(), expected);
        assert_eq!(
            plan.chain(),
            hash(zeno_fcis_codec::domains::V2_CHAIN, &expected)
                .unwrap_or_else(|error| panic!("{error}"))
        );
        // Deterministic, and every fact changes the link.
        assert_eq!(decide(&facts(Some(ADMITTED))).ok(), Some(plan.clone()));
        let mut changed = facts(Some(ADMITTED));
        changed.sequence = 7;
        assert_ne!(decide(&changed).ok(), Some(plan.clone()));
        let mut changed = facts(Some(ADMITTED));
        changed.previous_chain = Hash32::new([3; 32]);
        assert_ne!(decide(&changed).ok(), Some(plan.clone()));
        let mut changed = facts(Some(Genesis {
            subject: b"other sealed genesis",
            poststate: b"state",
        }));
        changed.ordinal = 1;
        assert_ne!(decide(&changed).ok(), Some(plan));
    }

    #[test]
    fn refusals_in_order_write_no_plan() {
        let mut other_schema = facts(Some(ADMITTED));
        other_schema.schema = b"other schema";
        assert!(matches!(
            decide(&other_schema),
            Err(Error::Upgrade(Refusal::StateSchema))
        ));
        let mut same = facts(Some(ADMITTED));
        same.identity = b"identity-1";
        assert!(matches!(
            decide(&same),
            Err(Error::Upgrade(Refusal::SameContract))
        ));
        // Schema precedes identity: both wrong reports the schema.
        same.schema = b"other schema";
        assert!(matches!(
            decide(&same),
            Err(Error::Upgrade(Refusal::StateSchema))
        ));
        assert!(matches!(
            decide(&facts(None)),
            Err(Error::Upgrade(Refusal::Genesis))
        ));
        assert!(matches!(
            decide(&facts(Some(Genesis {
                subject: b"sealed genesis",
                poststate: b"another state",
            }))),
            Err(Error::Upgrade(Refusal::Genesis))
        ));
    }

    #[test]
    fn segments_must_follow_the_lineage_in_order() {
        let lineage: [&[u8]; 3] = [b"one", b"two", b"three"];
        let ok = |segments: &[&[u8]]| assign(&lineage, segments).ok();
        assert_eq!(ok(&[b"one"]), Some(vec![0]));
        assert_eq!(ok(&[b"one", b"two", b"three"]), Some(vec![0, 1, 2]));
        assert_eq!(ok(&[b"two"]), Some(vec![1]));
        // A store may skip a version it never ran.
        assert_eq!(ok(&[b"one", b"three"]), Some(vec![0, 2]));
        assert_eq!(ok(&[]), Some(vec![]));
        for segments in [
            &[b"four" as &[u8]][..],
            &[b"two", b"one"],
            &[b"one", b"one"],
            &[b"three", b"three"],
            &[b"one", b"two", b"three", b"one"],
        ] {
            assert!(
                matches!(assign(&lineage, segments), Err(Error::Identity)),
                "{segments:?}"
            );
        }
        let repeated: [&[u8]; 3] = [b"one", b"two", b"one"];
        assert_eq!(
            assign(&repeated, &[b"one", b"two", b"one"]).ok(),
            Some(vec![0, 1, 2])
        );
    }
}
