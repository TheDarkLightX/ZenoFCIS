//! The upgrade decision as a pure function of facts the shell established:
//! no SQLite, no I/O, no clock. The shell gathers the facts inside one
//! immediate transaction, persists the plan this module returns, and
//! otherwise writes nothing.
//!
//! An upgrade moves a store from the contract its current history segment
//! was published under to a later contract of its lineage with the same state
//! schema. The record names how the new contract admits the current state;
//! its magic bytes are the kind tag:
//!
//! - `program-successor`, magic `ZFCISV2-SUCCESSOR\0`: the new contract's
//!   complete canonical policy, with its decision program's instructions and
//!   roots and its Step limit taken from the old contract, is byte for byte the
//!   old policy ([`program_successor`](crate::v2::upgrade::program_successor)).
//!   Every law, branch, schema and other limit is therefore the old contract's,
//!   and the state at the head was admitted under those laws, so the upgrade is
//!   admitted at any state without a genesis evaluation. The record also binds
//!   which further [`Premises`](crate::v2::upgrade::Premises) held and the
//!   SHA-256 of the `transform` receipt of every adoption between the two
//!   versions. With every premise, including the ones generation checks, the
//!   two versions have the same reachable states.
//! - `genesis-admission`, magic `ZFCISV2-UPGRADE\0` (the format schema v10
//!   introduced): any other contract must admit the current state through the
//!   library's genesis evaluation, and the record binds that genesis
//!   publication. A generated contract's law 990 admits only its declared
//!   genesis state, so for such contracts this kind is limited to stores at
//!   that state.
//!
//! Every record also binds the ordinal, the head's sequence, both full
//! identities, the state root and the chain tip it extends.

use zeno_fcis_codec::Hash32;
use zeno_fcis_synthesis::finite::{
    V2Resource, V2ScalarProgram, v2_authority as authority, v2_catalog::BoundCatalog,
    v2_composition::Descriptor, v2_laws as laws,
};

use super::{Error, hash};

/// How an upgrade record shows that the new contract admits the store's
/// state. Both magics, like a certificate's `ZFCISV2-CERT\0`, first differ
/// at byte 8, so all three hash under `V2_CHAIN` without aliasing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Kind {
    /// The new contract differs from the old one only in its decision
    /// program and Step limit; see [`program_successor`].
    ProgramSuccessor,
    /// The new contract's genesis evaluation admitted the current state.
    GenesisAdmission,
}

impl Kind {
    /// The kind's name in reports: `program-successor` or `genesis-admission`.
    pub fn tag(self) -> &'static str {
        match self {
            Self::ProgramSuccessor => "program-successor",
            Self::GenesisAdmission => "genesis-admission",
        }
    }

    fn magic(self) -> &'static [u8] {
        match self {
            Self::ProgramSuccessor => b"ZFCISV2-SUCCESSOR\0",
            Self::GenesisAdmission => b"ZFCISV2-UPGRADE\0",
        }
    }

    /// The kind a stored record claims by its magic; the shell then checks
    /// that claim in full.
    pub(super) fn of_record(record: &[u8]) -> Option<Self> {
        [Self::ProgramSuccessor, Self::GenesisAdmission]
            .into_iter()
            .find(|kind| record.starts_with(kind.magic()))
    }
}

/// Whether `to` succeeds `from` by its decision program alone: `to`'s
/// complete canonical policy, with its program's instructions and roots and
/// its Step limit replaced by `from`'s, is byte for byte `from`'s policy.
///
/// The library's own serializer computes the policy, over the original
/// schema, framing, channel links and every descriptor field, so equality
/// means that everything else is `from`'s: the state, command and context
/// schemas, input bindings, output types, the decision output, branches,
/// reasons, channels, laws and their required IDs, every other limit, and the
/// program's input and output domains.
///
/// # Errors
/// `Error::Range` when the policy cannot be encoded.
pub fn program_successor(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>) -> Result<bool, Error> {
    let (old, new) = (from.descriptor(), to.descriptor());
    let substituted = Descriptor {
        state: new.state,
        command: new.command,
        context: new.context,
        program: V2ScalarProgram {
            inputs: new.program.inputs,
            outputs: new.program.outputs,
            nodes: old.program.nodes,
            roots: old.program.roots,
        },
        bindings: new.bindings,
        output_types: new.output_types,
        decision_output: new.decision_output,
        branches: new.branches,
        reasons: new.reasons,
        channels: new.channels,
        laws: new.laws,
        required: new.required,
        limits: new
            .limits
            .with_limit(V2Resource::Step, old.limits.limit(V2Resource::Step)),
    };
    let policy = authority::policy_bytes(
        &substituted,
        to.original_schema(),
        to.framing(),
        to.channel_roots(),
    )
    .ok_or(Error::Range)?;
    Ok(policy == from.original_contract())
}

/// The premises of the reachable-state guarantee that the shell checks for a
/// program succession, beyond the policy comparison that defines it.
///
/// A program successor and the version it supersedes have the same reachable
/// states when their policies differ only in the decision program and Step
/// limit, the identical laws pin every committed decision to the case table
/// (generated law 991), the adoption receipts are F3 `Equivalent`, neither
/// Step limit binds, and no law observes Step usage. Generation checks law
/// 991 and replays the receipts; this records the rest. Steps are charged
/// only for program instruction attempts and law nodes, and a program change
/// alters no other meter reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Premises {
    /// Each version's Step limit is at least one Step for every program node
    /// and every law node, the most one evaluation charges, so neither limit
    /// refuses a decision.
    pub step_limits_never_bind: bool,
    /// No law observes Step usage, the one meter reading a program change can
    /// alter.
    pub step_usage_unobserved: bool,
}

impl Premises {
    fn of(from: &Descriptor<'_>, to: &Descriptor<'_>) -> Self {
        Self {
            step_limits_never_bind: steps_covered(from) && steps_covered(to),
            step_usage_unobserved: !observes_step_usage(from) && !observes_step_usage(to),
        }
    }

    /// Every premise held.
    pub fn all(self) -> bool {
        self.step_limits_never_bind && self.step_usage_unobserved
    }

    /// The record's premises byte: bit 0 the policy comparison, which holds
    /// for every program successor, bit 1 `step_limits_never_bind`, bit 2
    /// `step_usage_unobserved`.
    fn byte(self) -> u8 {
        1 | u8::from(self.step_limits_never_bind) << 1 | u8::from(self.step_usage_unobserved) << 2
    }
}

/// The Step limit covers one Step for each program node and each law node.
fn steps_covered(descriptor: &Descriptor<'_>) -> bool {
    let program = descriptor.program.nodes.len() as u128;
    let laws: u128 = descriptor
        .laws
        .iter()
        .map(|law| law.program.nodes.len() as u128)
        .sum();
    program + laws <= u128::from(descriptor.limits.limit(V2Resource::Step))
}

/// Some law reads the Step usage.
fn observes_step_usage(descriptor: &Descriptor<'_>) -> bool {
    let step = laws::Observation::Usage(V2Resource::Step);
    descriptor.laws.iter().any(|law| {
        law.program.nodes.iter().any(|node| match *node {
            laws::Op::Observe(observation) | laws::Op::ObserveWhen(_, observation, _) => {
                observation == step
            }
            _ => false,
        })
    })
}

/// A program succession the shell established, with the premises it
/// checked and the SHA-256 of the `transform` receipt of every adoption from
/// the old version to the new one, oldest first. Only
/// [`Successor::establish`] constructs one, so a value shows that
/// [`program_successor`] held.
#[derive(Clone, Copy, Debug)]
pub struct Successor<'a> {
    receipts: &'a [Hash32],
    premises: Premises,
}

impl<'a> Successor<'a> {
    /// Compares the two catalogs' policies; `None` when `to` changes more
    /// than the decision program and its Step limit.
    ///
    /// # Errors
    /// `Error::Lineage` without a receipt, and `Error::Range` when a policy
    /// cannot be encoded.
    pub fn establish(
        from: &BoundCatalog<'_>,
        to: &BoundCatalog<'_>,
        receipts: &'a [Hash32],
    ) -> Result<Option<Self>, Error> {
        if receipts.is_empty() {
            return Err(Error::Lineage);
        }
        let premises = Premises::of(from.descriptor(), to.descriptor());
        Ok(program_successor(from, to)?.then_some(Self { receipts, premises }))
    }

    /// The adoption receipts' SHA-256 digests, oldest first.
    pub fn receipts(&self) -> &'a [Hash32] {
        self.receipts
    }

    /// The premises the shell checked.
    pub fn premises(&self) -> Premises {
        self.premises
    }
}

/// The new contract's genesis publication over the current state, as the
/// library produced it.
#[derive(Clone, Copy, Debug)]
pub struct Genesis<'a> {
    /// The complete sealed subject.
    pub subject: &'a [u8],
    /// The poststate the publication carries; genesis repeats its input.
    pub poststate: &'a [u8],
}

/// How the new contract admits the current state.
#[derive(Clone, Copy, Debug)]
pub enum Admission<'a> {
    /// A program succession: no genesis evaluation is needed.
    Successor(Successor<'a>),
    /// The library's genesis publication over the state.
    Genesis(Genesis<'a>),
    /// The library's genesis evaluation did not commit the state; `Some`
    /// carries its technical refusal, such as the law that refused.
    Refused(Option<authority::Refusal>),
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
    /// How the new contract admits that state.
    pub admission: Admission<'a>,
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
    /// The new contract is not a program successor, and its genesis
    /// evaluation does not admit the current state, or admits another one.
    /// `Some` carries the library's refusal; for a generated contract it is
    /// usually law 990, which admits only the declared genesis state.
    Genesis(Option<authority::Refusal>),
}

/// A decided upgrade: its kind, the canonical record and the chain link it
/// adds, and the premises and receipt digests a program succession binds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    kind: Kind,
    record: Vec<u8>,
    chain: Hash32,
    premises: Option<Premises>,
    evidence: Vec<u8>,
}

impl Plan {
    /// How the new contract admitted the state.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Canonical record bytes; the chain link is their `V2_CHAIN` hash.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    /// The hash chain tip after the upgrade.
    pub fn chain(&self) -> Hash32 {
        self.chain
    }

    /// The premises a program succession recorded; `None` for a genesis
    /// admission.
    pub fn premises(&self) -> Option<Premises> {
        self.premises
    }

    /// The bound receipt digests, 32 bytes each, oldest first; empty for a
    /// genesis admission.
    pub fn evidence(&self) -> &[u8] {
        &self.evidence
    }
}

/// Decides one upgrade. Refusal order: state schema, same contract, then
/// the admission. A program succession is preferred over a genesis
/// admission whenever the shell established one.
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
    let (kind, admitted, premises, evidence) = match facts.admission {
        Admission::Successor(successor) => {
            let evidence = evidence_bytes(successor.receipts);
            (
                Kind::ProgramSuccessor,
                successor_admission(successor.premises, &evidence),
                Some(successor.premises),
                evidence,
            )
        }
        Admission::Genesis(genesis) if genesis.poststate == facts.state => (
            Kind::GenesisAdmission,
            genesis.subject.to_vec(),
            None,
            Vec::new(),
        ),
        Admission::Genesis(_) => return Err(Error::Upgrade(Refusal::Genesis(None))),
        Admission::Refused(reason) => return Err(Error::Upgrade(Refusal::Genesis(reason))),
    };
    let record = record_bytes(
        kind,
        facts.ordinal,
        facts.sequence,
        facts.from_identity,
        facts.identity,
        &admitted,
        facts.root,
        facts.previous_chain,
    )?;
    let chain = hash(zeno_fcis_codec::domains::V2_CHAIN, &record)?;
    Ok(Plan {
        kind,
        record,
        chain,
        premises,
        evidence,
    })
}

/// A program succession's admission as the record frames it: the premises
/// byte, then the receipt digests.
pub(super) fn successor_admission(premises: Premises, evidence: &[u8]) -> Vec<u8> {
    let mut admission = vec![premises.byte()];
    admission.extend_from_slice(evidence);
    admission
}

/// The canonical upgrade record: the kind's magic, the ordinal and head
/// sequence, then the framed from-identity, identity, admission, state root
/// and previous chain tip. The admission is the complete genesis publication
/// for a genesis admission, and for a program successor the premises byte
/// followed by the concatenated receipt digests.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_bytes(
    kind: Kind,
    ordinal: u64,
    sequence: u64,
    from_identity: &[u8],
    identity: &[u8],
    admission: &[u8],
    root: Hash32,
    previous_chain: Hash32,
) -> Result<Vec<u8>, Error> {
    let mut bytes = kind.magic().to_vec();
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    for part in [
        from_identity,
        identity,
        admission,
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

/// Receipt digests as stored: 32 bytes each, oldest first.
pub(super) fn evidence_bytes(receipts: &[Hash32]) -> Vec<u8> {
    receipts
        .iter()
        .flat_map(|receipt| receipt.as_bytes().to_vec())
        .collect()
}

/// The receipt digests of stored evidence: at least one, 32 bytes each.
pub(super) fn evidence_receipts(evidence: &[u8]) -> Result<Vec<Hash32>, Error> {
    if evidence.is_empty() || !evidence.len().is_multiple_of(32) {
        return Err(Error::History);
    }
    evidence
        .chunks_exact(32)
        .map(|chunk| {
            chunk
                .try_into()
                .map(Hash32::new)
                .map_err(|_| Error::History)
        })
        .collect()
}

/// Which lineage member published each history segment. A store's segments
/// are the contracts it ran, oldest first; they must appear in the lineage in
/// that order, each later than the previous one. Each segment takes the
/// earliest member that fits, so a lineage that grows at its end assigns an
/// existing store's segments exactly as before. A segment under no lineage
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

    const RECEIPTS: [Hash32; 2] = [Hash32::new([7; 32]), Hash32::new([8; 32])];
    const HELD: Premises = Premises {
        step_limits_never_bind: true,
        step_usage_unobserved: true,
    };

    fn facts<'a>(admission: Admission<'a>) -> Facts<'a> {
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
            admission,
        }
    }

    const ADMITTED: Admission<'static> = Admission::Genesis(Genesis {
        subject: b"sealed genesis",
        poststate: b"state",
    });

    fn successor() -> Admission<'static> {
        Admission::Successor(Successor {
            receipts: &RECEIPTS,
            premises: HELD,
        })
    }

    fn expected(magic: &[u8], admission: &[u8]) -> Vec<u8> {
        let mut expected = magic.to_vec();
        expected.extend_from_slice(&1_u64.to_be_bytes());
        expected.extend_from_slice(&6_u64.to_be_bytes());
        for part in [
            &b"identity-1"[..],
            b"identity-2",
            admission,
            &[1; 32],
            &[2; 32],
        ] {
            expected.extend_from_slice(&(part.len() as u64).to_be_bytes());
            expected.extend_from_slice(part);
        }
        expected
    }

    #[test]
    fn the_record_binds_every_fact_and_the_chain_is_its_hash() {
        let plan = decide(&facts(ADMITTED)).unwrap_or_else(|error| panic!("{error}"));
        let expected = expected(b"ZFCISV2-UPGRADE\0", b"sealed genesis");
        assert_eq!(plan.kind(), Kind::GenesisAdmission);
        assert_eq!(plan.record(), expected);
        assert!(plan.evidence().is_empty());
        assert_eq!(
            plan.chain(),
            hash(zeno_fcis_codec::domains::V2_CHAIN, &expected)
                .unwrap_or_else(|error| panic!("{error}"))
        );
        // Deterministic, and every fact changes the link.
        assert_eq!(decide(&facts(ADMITTED)).ok(), Some(plan.clone()));
        let mut changed = facts(ADMITTED);
        changed.sequence = 7;
        assert_ne!(decide(&changed).ok(), Some(plan.clone()));
        let mut changed = facts(ADMITTED);
        changed.previous_chain = Hash32::new([3; 32]);
        assert_ne!(decide(&changed).ok(), Some(plan.clone()));
        let changed = facts(Admission::Genesis(Genesis {
            subject: b"other sealed genesis",
            poststate: b"state",
        }));
        assert_ne!(decide(&changed).ok(), Some(plan));
    }

    #[test]
    fn a_program_succession_binds_its_premises_and_receipts_under_its_own_magic() {
        let plan = decide(&facts(successor())).unwrap_or_else(|error| panic!("{error}"));
        let mut evidence = vec![7; 32];
        evidence.extend_from_slice(&[8; 32]);
        let mut admission = vec![0b111];
        admission.extend_from_slice(&evidence);
        assert_eq!(plan.kind(), Kind::ProgramSuccessor);
        assert_eq!(plan.kind().tag(), "program-successor");
        assert_eq!(plan.record(), expected(b"ZFCISV2-SUCCESSOR\0", &admission));
        assert_eq!(plan.premises(), Some(HELD));
        assert!(HELD.all());
        assert_eq!(plan.evidence(), evidence);
        assert_eq!(
            evidence_receipts(plan.evidence()).ok(),
            Some(RECEIPTS.to_vec())
        );
        assert_eq!(Kind::of_record(plan.record()), Some(Kind::ProgramSuccessor));
        let genesis = decide(&facts(ADMITTED)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            Kind::of_record(genesis.record()),
            Some(Kind::GenesisAdmission)
        );
        assert_eq!(genesis.premises(), None);
        assert_eq!(Kind::of_record(b"ZFCISV2-CERT\0"), None);
        // The two kinds never share a record or a link for the same facts.
        assert_ne!(plan.chain(), genesis.chain());
        // Each receipt, and its order, changes the record, as does each premise.
        let reversed = [RECEIPTS[1], RECEIPTS[0]];
        let swapped = decide(&facts(Admission::Successor(Successor {
            receipts: &reversed,
            premises: HELD,
        })))
        .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(swapped.chain(), plan.chain());
        for (premises, byte) in [
            (
                Premises {
                    step_limits_never_bind: false,
                    ..HELD
                },
                0b101,
            ),
            (
                Premises {
                    step_usage_unobserved: false,
                    ..HELD
                },
                0b011,
            ),
        ] {
            assert!(!premises.all());
            let failed = decide(&facts(Admission::Successor(Successor {
                receipts: &RECEIPTS,
                premises,
            })))
            .unwrap_or_else(|error| panic!("{error}"));
            let mut admission = vec![byte];
            admission.extend_from_slice(&evidence);
            assert_eq!(
                failed.record(),
                expected(b"ZFCISV2-SUCCESSOR\0", &admission)
            );
            assert_ne!(failed.chain(), plan.chain());
        }
        for malformed in [&[][..], &[7; 31], &[7; 33]] {
            assert!(matches!(evidence_receipts(malformed), Err(Error::History)));
        }
    }

    #[test]
    fn refusals_in_order_write_no_plan() {
        for admission in [ADMITTED, successor()] {
            let mut other_schema = facts(admission);
            other_schema.schema = b"other schema";
            assert!(matches!(
                decide(&other_schema),
                Err(Error::Upgrade(Refusal::StateSchema))
            ));
            let mut same = facts(admission);
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
        }
        let refusal = authority::Refusal::Encoding;
        assert!(matches!(
            decide(&facts(Admission::Refused(Some(refusal)))),
            Err(Error::Upgrade(Refusal::Genesis(Some(reason)))) if reason == refusal
        ));
        assert!(matches!(
            decide(&facts(Admission::Refused(None))),
            Err(Error::Upgrade(Refusal::Genesis(None)))
        ));
        assert!(matches!(
            decide(&facts(Admission::Genesis(Genesis {
                subject: b"sealed genesis",
                poststate: b"another state",
            }))),
            Err(Error::Upgrade(Refusal::Genesis(None)))
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
        // A repeated identity takes its earliest fitting position, which a
        // lineage that grows at its end never changes.
        let repeated: [&[u8]; 3] = [b"one", b"two", b"one"];
        assert_eq!(
            assign(&repeated, &[b"one", b"two", b"one"]).ok(),
            Some(vec![0, 1, 2])
        );
        assert_eq!(assign(&repeated, &[b"one"]).ok(), Some(vec![0]));
        let grown: [&[u8]; 4] = [b"one", b"two", b"one", b"four"];
        assert_eq!(assign(&grown, &[b"one"]).ok(), Some(vec![0]));
        assert_eq!(assign(&grown, &[b"one", b"four"]).ok(), Some(vec![0, 3]));
    }
}
