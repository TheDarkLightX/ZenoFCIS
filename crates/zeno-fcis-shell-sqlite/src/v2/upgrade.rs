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
//! - `program-successor`, magic `ZFCISV2-SUCCESSOR\0`: the shell itself
//!   established all five [`Premise`](crate::v2::upgrade::Premise)s of the
//!   plan's program succession from the two contracts' bound catalogs (see
//!   [`Successor::establish`](crate::v2::upgrade::Successor::establish)),
//!   including an exhaustive comparison of the two decision programs on every
//!   input tuple. Under those premises both contracts have the same reachable
//!   states, so the upgrade is admitted at any state without a genesis
//!   evaluation. The record binds the number of input tuples compared and,
//!   as provenance, the SHA-256 of the `transform` receipt of every adoption
//!   between the two versions as the lineage declares them.
//! - `genesis-admission`, magic `ZFCISV2-UPGRADE\0` (the format schema v10
//!   introduced): when any premise is missing, the new contract must admit
//!   the current state through the library's genesis evaluation, and the
//!   record binds that genesis publication. A generated contract's law 990
//!   admits only its declared genesis state, so for such contracts this kind
//!   is limited to stores at that state.
//!
//! Every record also binds the ordinal, the head's sequence, both full
//! identities, the state root and the chain tip it extends.

use std::fmt;
use zeno_fcis_codec::Hash32;

use zeno_fcis_synthesis::finite::{
    V2Resource, V2ScalarProgram, v2_authority as authority, v2_catalog::BoundCatalog,
    v2_composition::Descriptor, v2_laws as laws,
};

use super::equivalence::{self, Unestablished};
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

/// The identifier of generated decision-conformance law 991.
pub const DECISION_LAW: u32 = 991;

/// One premise of a program succession, numbered as in the plan of record.
/// When one is missing, the upgrade takes the genesis route instead and a
/// refusal names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Premise {
    /// 1. The canonical policies are identical except the decision program
    ///    and its Step limit: [`program_successor`].
    Policy,
    /// 2. Each contract declares law 991 as a decision-conformance law that
    ///    applies to every decision and not at genesis, and requires it.
    DecisionLaw,
    /// 3. The two decision programs are equal on every input tuple of their
    ///    declared domain, by the shell's own exhaustive comparison; carries
    ///    why the comparison did not establish it.
    Equivalence(Unestablished),
    /// 4. Neither Step limit binds: each covers one Step for every program
    ///    node and every law node.
    StepLimits,
    /// 5. No law observes Step usage.
    StepUsage,
}

impl fmt::Display for Premise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy => f.write_str(
                "premise 1: its policy differs from the store's contract in more than the decision program and its Step limit",
            ),
            Self::DecisionLaw => f.write_str(
                "premise 2: a contract does not declare and require decision-conformance law 991 on every decision",
            ),
            Self::Equivalence(Unestablished::Counterexample { ordinal }) => write!(
                f,
                "premise 3: the two decision programs differ on input tuple {ordinal} of their declared domain, counting from 0 in enumeration order"
            ),
            Self::Equivalence(Unestablished::DomainTooLarge {
                size: Some(size),
                cap,
            }) => write!(
                f,
                "premise 3: the decision programs' input domain has {size} tuples, more than the lineage's comparison cap of {cap}"
            ),
            Self::Equivalence(Unestablished::DomainTooLarge { size: None, cap }) => write!(
                f,
                "premise 3: the decision programs' input domain has more than 2^128 tuples, more than the lineage's comparison cap of {cap}"
            ),
            Self::Equivalence(Unestablished::InputAbi) => f.write_str(
                "premise 3: the two decision programs' input domains differ",
            ),
            Self::Equivalence(Unestablished::OutputAbi) => f.write_str(
                "premise 3: the two decision programs' output domains differ",
            ),
            Self::Equivalence(Unestablished::EmptyInputDomain { position }) => write!(
                f,
                "premise 3: input {position} of the decision programs has an empty domain"
            ),
            Self::Equivalence(Unestablished::CoverageMismatch { expected, visited }) => write!(
                f,
                "premise 3: the comparison of the decision programs visited {visited} tuples, not the {expected} of their domain"
            ),
            Self::StepLimits => f.write_str(
                "premise 4: a Step limit is below one Step per program node and law node, so it could refuse a decision",
            ),
            Self::StepUsage => f.write_str(
                "premise 5: a law reads the Step usage, which a program change alters",
            ),
        }
    }
}

/// Why an audit does not accept a stored program-successor record under the
/// lineage that opened the store. The store may be intact: the cause can be
/// what the application declares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Unsupported {
    /// The record is exact under the adoption receipt digests it stores, and
    /// those are not the ones the lineage declares for the versions it
    /// spans. A record that does not match its own stored digests is a
    /// damaged history instead.
    Receipts,
    /// The lineage's own catalogs do not establish the succession under its
    /// comparison cap; carries the missing premise.
    Premise(Premise),
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Receipts => f.write_str(
                "it names other adoption receipts than this application declares for those versions",
            ),
            Self::Premise(premise) => write!(
                f,
                "this application's contracts do not establish it ({premise})"
            ),
        }
    }
}

/// Evidence that the shell established all five premises of a program
/// succession. Only [`Successor::establish`] constructs one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Premises {
    tuples: u64,
}

impl Premises {
    /// The number of input tuples on which the shell compared the two
    /// decision programs: their whole declared domain.
    pub fn tuples(self) -> u64 {
        self.tuples
    }

    /// The record's premises byte: bits 0 to 4 are premises 1 to 5, and all
    /// are set, because a record exists only when all five held.
    const BYTE: u8 = 0b1_1111;
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

/// The contract declares law 991 as a decision-conformance law on every
/// decision, not at genesis, and requires it. This is the generator's own
/// test for law 991 plus the required list; the law's predicate is opaque
/// here, so it does not show that the predicate encodes any case table.
fn requires_decision_law(descriptor: &Descriptor<'_>) -> bool {
    descriptor.required.contains(&DECISION_LAW)
        && descriptor.laws.iter().any(|law| {
            law.id == DECISION_LAW
                && law.kind == laws::Kind::DecisionConformance
                && law.scope == laws::Scope::Always
                && !law.genesis
        })
}

/// Establishes the five premises from `from` to `to` in the order 1, 2, 4,
/// 5, 3, and reports whether the decision programs were enumerated: false
/// when an earlier premise is missing, or when the two programs have
/// identical instructions and roots.
///
/// # Errors
/// `Error::Range` when a policy cannot be encoded.
pub(super) fn premises(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    max_input_tuples: u64,
) -> Result<(Result<Premises, Premise>, bool), Error> {
    let (old, new) = (from.descriptor(), to.descriptor());
    let missing = if !program_successor(from, to)? {
        Some(Premise::Policy)
    } else if !requires_decision_law(old) || !requires_decision_law(new) {
        Some(Premise::DecisionLaw)
    } else if !steps_covered(old) || !steps_covered(new) {
        Some(Premise::StepLimits)
    } else if observes_step_usage(old) || observes_step_usage(new) {
        Some(Premise::StepUsage)
    } else {
        None
    };
    if let Some(missing) = missing {
        return Ok((Err(missing), false));
    }
    Ok(
        match equivalence::compare(&old.program, &new.program, max_input_tuples) {
            Ok(equal) => (
                Ok(Premises {
                    tuples: equal.tuples(),
                }),
                equal.enumerated(),
            ),
            Err(unestablished) => (
                Err(Premise::Equivalence(unestablished)),
                !matches!(
                    unestablished,
                    Unestablished::InputAbi
                        | Unestablished::OutputAbi
                        | Unestablished::EmptyInputDomain { .. }
                        | Unestablished::DomainTooLarge { .. }
                ),
            ),
        },
    )
}

/// A program succession the shell established: all five premises held. It
/// carries the evidence and the SHA-256 of the `transform` receipt of every
/// adoption from the old version to the new one, oldest first, as the
/// lineage declares them. Only [`Successor::establish`] constructs one.
#[derive(Clone, Copy, Debug)]
pub struct Successor<'a> {
    receipts: &'a [Hash32],
    premises: Premises,
}

impl<'a> Successor<'a> {
    /// Establishes every premise of a succession from `from` to `to`, from
    /// the two bound catalogs alone, in the order 1, 2, 4, 5, 3: the policy
    /// comparison, law 991 in both, both Step limits, no Step observation in
    /// either, and then the exhaustive comparison of the two decision
    /// programs through the library evaluator on every input tuple, if their
    /// domain has at most `max_input_tuples` tuples; two programs with
    /// identical instructions and roots are equal without enumeration.
    /// `Ok(Err(premise))` names the first premise that is missing. This
    /// computes afresh; a [`Lineage`](crate::v2::Lineage) remembers each
    /// pair's outcome for its upgrades and audits.
    ///
    /// `receipts` are provenance only: the shell replays no receipt.
    ///
    /// # Errors
    /// `Error::Lineage` without a receipt, and `Error::Range` when a policy
    /// cannot be encoded.
    pub fn establish(
        from: &BoundCatalog<'_>,
        to: &BoundCatalog<'_>,
        receipts: &'a [Hash32],
        max_input_tuples: u64,
    ) -> Result<Result<Self, Premise>, Error> {
        if receipts.is_empty() {
            return Err(Error::Lineage);
        }
        let (established, _) = premises(from, to, max_input_tuples)?;
        Ok(established.map(|premises| Self::held(receipts, premises)))
    }

    /// The succession whose premises an earlier [`Successor::establish`],
    /// or a lineage's memo of one, established for these two versions.
    pub(super) fn held(receipts: &'a [Hash32], premises: Premises) -> Self {
        Self { receipts, premises }
    }

    /// The adoption receipts' SHA-256 digests, oldest first.
    pub fn receipts(&self) -> &'a [Hash32] {
        self.receipts
    }

    /// The evidence that every premise held.
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
    /// The premise named was missing, and this is the library's genesis
    /// publication over the state.
    Genesis(Premise, Genesis<'a>),
    /// The premise named was missing, and the library's genesis evaluation
    /// did not commit the state; `Some` carries its technical refusal, such
    /// as the law that refused.
    Refused(Premise, Option<authority::Refusal>),
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
    /// A premise of a program succession is missing, and the new contract's
    /// genesis evaluation does not admit the current state, or admits
    /// another one.
    Genesis {
        /// The first missing premise, in the order the shell checks them.
        missing: Premise,
        /// The library's refusal; for a generated contract it is usually law
        /// 990, which admits only the declared genesis state.
        refusal: Option<authority::Refusal>,
    },
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

    /// For a program succession, the evidence that every premise held;
    /// `None` for a genesis admission.
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
        Admission::Genesis(_, genesis) if genesis.poststate == facts.state => (
            Kind::GenesisAdmission,
            genesis.subject.to_vec(),
            None,
            Vec::new(),
        ),
        Admission::Genesis(missing, _) => {
            return Err(Error::Upgrade(Refusal::Genesis {
                missing,
                refusal: None,
            }));
        }
        Admission::Refused(missing, refusal) => {
            return Err(Error::Upgrade(Refusal::Genesis { missing, refusal }));
        }
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
/// byte, the number of input tuples compared as a big-endian `u64`, then the
/// receipt digests.
pub(super) fn successor_admission(premises: Premises, evidence: &[u8]) -> Vec<u8> {
    let mut admission = vec![Premises::BYTE];
    admission.extend_from_slice(&premises.tuples.to_be_bytes());
    admission.extend_from_slice(evidence);
    admission
}

/// The canonical upgrade record: the kind's magic, the ordinal and head
/// sequence, then the framed from-identity, identity, admission, state root
/// and previous chain tip. The admission is the complete genesis publication
/// for a genesis admission, and for a program successor the premises byte,
/// the number of input tuples compared and the concatenated receipt digests.
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
    const HELD: Premises = Premises { tuples: 1_296_000 };

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

    const ADMITTED: Admission<'static> = Admission::Genesis(
        Premise::Policy,
        Genesis {
            subject: b"sealed genesis",
            poststate: b"state",
        },
    );

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
        let changed = facts(Admission::Genesis(
            Premise::Policy,
            Genesis {
                subject: b"other sealed genesis",
                poststate: b"state",
            },
        ));
        assert_ne!(decide(&changed).ok(), Some(plan));
    }

    #[test]
    fn a_program_succession_binds_its_premises_and_receipts_under_its_own_magic() {
        let plan = decide(&facts(successor())).unwrap_or_else(|error| panic!("{error}"));
        let mut evidence = vec![7; 32];
        evidence.extend_from_slice(&[8; 32]);
        let mut admission = vec![0b1_1111];
        admission.extend_from_slice(&1_296_000_u64.to_be_bytes());
        admission.extend_from_slice(&evidence);
        assert_eq!(plan.kind(), Kind::ProgramSuccessor);
        assert_eq!(plan.kind().tag(), "program-successor");
        assert_eq!(plan.record(), expected(b"ZFCISV2-SUCCESSOR\0", &admission));
        assert_eq!(plan.premises(), Some(HELD));
        assert_eq!(plan.evidence(), evidence);
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
        // Each receipt, and its order, changes the record, as does the
        // number of tuples compared.
        let reversed = [RECEIPTS[1], RECEIPTS[0]];
        let swapped = decide(&facts(Admission::Successor(Successor {
            receipts: &reversed,
            premises: HELD,
        })))
        .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(swapped.chain(), plan.chain());
        let fewer = decide(&facts(Admission::Successor(Successor {
            receipts: &RECEIPTS,
            premises: Premises { tuples: 1 },
        })))
        .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(fewer.chain(), plan.chain());
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
        // A genesis refusal names the missing premise and the library's refusal.
        let refusal = authority::Refusal::Encoding;
        let missing = Premise::Equivalence(Unestablished::Counterexample { ordinal: 5 });
        assert!(matches!(
            decide(&facts(Admission::Refused(missing, Some(refusal)))),
            Err(Error::Upgrade(Refusal::Genesis { missing: named, refusal: Some(reason) }))
                if reason == refusal && named == missing
        ));
        assert!(matches!(
            decide(&facts(Admission::Refused(Premise::StepLimits, None))),
            Err(Error::Upgrade(Refusal::Genesis {
                missing: Premise::StepLimits,
                refusal: None
            }))
        ));
        assert!(matches!(
            decide(&facts(Admission::Genesis(
                Premise::DecisionLaw,
                Genesis {
                    subject: b"sealed genesis",
                    poststate: b"another state",
                }
            ))),
            Err(Error::Upgrade(Refusal::Genesis {
                missing: Premise::DecisionLaw,
                refusal: None
            }))
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
