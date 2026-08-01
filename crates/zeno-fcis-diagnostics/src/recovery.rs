//! Exact, effect-closed recovery-word diagnostics.
//!
//! The tree owns complete snapshot commitments and ordered events. It rejects
//! discontinuous durable histories even when adjacent observations share the
//! same coarse PRE or POST class. The value is diagnostic evidence only.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError, Hash32};
use zeno_fcis_value::{AsciiText, TextError};

/// Canonical ICRWT profile version.
pub const ICRWT_VERSION: u16 = 3;

/// Maximum bytes in one recovery word identifier.
pub const MAX_RECOVERY_WORD_ID_BYTES: usize = 64;

/// Maximum bytes in one recovery event identifier.
pub const MAX_RECOVERY_EVENT_ID_BYTES: usize = 64;

/// Maximum bytes in one recovery action label.
pub const MAX_RECOVERY_ACTION_BYTES: usize = 64;

/// Exact bounded recovery-word identifier.
pub type RecoveryWordId = AsciiText<MAX_RECOVERY_WORD_ID_BYTES>;

/// Exact bounded recovery-event identifier.
pub type RecoveryEventId = AsciiText<MAX_RECOVERY_EVENT_ID_BYTES>;

/// Exact bounded recovery-action label.
pub type RecoveryAction = AsciiText<MAX_RECOVERY_ACTION_BYTES>;

/// Coarse observation class retained beside each complete snapshot commitment.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecoveryObservation {
    /// Exact pre-transition durable state.
    Pre,
    /// Exact post-transition durable state.
    Post,
    /// An impermissible mixture of pre- and post-transition durable facts.
    Mixed,
    /// Client knowledge does not yet determine PRE or POST.
    Indeterminate,
}

impl RecoveryObservation {
    const fn tag(self) -> u8 {
        match self {
            Self::Pre => 0,
            Self::Post => 1,
            Self::Mixed => 2,
            Self::Indeterminate => 3,
        }
    }
}

/// Complete commitment tuple used by the recovery diagnostic.
///
/// Upstream code remains responsible for proving that the durable-layout root
/// commits every required row. This type only prevents the diagnostic from
/// silently comparing the coarse observation class alone.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecoverySnapshotCommitment {
    observation: RecoveryObservation,
    semantic_root: Hash32,
    durable_layout_root: Hash32,
    authority_root: Hash32,
}

impl RecoverySnapshotCommitment {
    /// Constructs one exact snapshot commitment tuple.
    #[must_use]
    pub const fn new(
        observation: RecoveryObservation,
        semantic_root: Hash32,
        durable_layout_root: Hash32,
        authority_root: Hash32,
    ) -> Self {
        Self {
            observation,
            semantic_root,
            durable_layout_root,
            authority_root,
        }
    }

    /// Returns the coarse recovery observation.
    #[must_use]
    pub const fn observation(self) -> RecoveryObservation {
        self.observation
    }

    /// Returns the semantic-state commitment.
    #[must_use]
    pub const fn semantic_root(self) -> Hash32 {
        self.semantic_root
    }

    /// Returns the complete canonical durable-layout commitment.
    #[must_use]
    pub const fn durable_layout_root(self) -> Hash32 {
        self.durable_layout_root
    }

    /// Returns the authority-state commitment.
    #[must_use]
    pub const fn authority_root(self) -> Hash32 {
        self.authority_root
    }
}

impl CanonicalEncode for RecoverySnapshotCommitment {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(self.observation.tag());
        output.extend_from_slice(self.semantic_root.as_bytes());
        output.extend_from_slice(self.durable_layout_root.as_bytes());
        output.extend_from_slice(self.authority_root.as_bytes());
        Ok(())
    }
}

/// Whether an event is retained semantic progress or a structurally checked stutter.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecoveryEventKind {
    /// Complete snapshot equality and an empty effect set make the event erasable.
    Stutter,
    /// The event remains in the semantic recovery path.
    Progress,
}

impl RecoveryEventKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Stutter => 0,
            Self::Progress => 1,
        }
    }
}

/// One exact recovery intervention.
///
/// Safe public construction makes an effectful or snapshot-changing stutter
/// unrepresentable. Tree construction repeats the structural checks.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecoveryEvent {
    event_id: RecoveryEventId,
    action: RecoveryAction,
    kind: RecoveryEventKind,
    before_snapshot: RecoverySnapshotCommitment,
    after_snapshot: RecoverySnapshotCommitment,
    payload_root: Hash32,
    effects: Box<[Hash32]>,
}

impl RecoveryEvent {
    /// Constructs a structural stutter over one unchanged complete snapshot.
    pub fn try_stutter(
        event_id: &str,
        action: &str,
        snapshot: RecoverySnapshotCommitment,
        payload_root: Hash32,
    ) -> Result<Self, RecoveryError> {
        Ok(Self {
            event_id: parse_event_id(event_id)?,
            action: parse_action(action)?,
            kind: RecoveryEventKind::Stutter,
            before_snapshot: snapshot,
            after_snapshot: snapshot,
            payload_root,
            effects: Box::new([]),
        })
    }

    /// Constructs retained progress and canonicalizes its exact effect identities.
    pub fn try_progress(
        event_id: &str,
        action: &str,
        before_snapshot: RecoverySnapshotCommitment,
        after_snapshot: RecoverySnapshotCommitment,
        payload_root: Hash32,
        mut effects: Vec<Hash32>,
    ) -> Result<Self, RecoveryError> {
        effects.sort_unstable();
        if effects.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RecoveryError::DuplicateEffectIdentity);
        }
        let _ = u32::try_from(effects.len()).map_err(|_| RecoveryError::LengthOverflow)?;
        Ok(Self {
            event_id: parse_event_id(event_id)?,
            action: parse_action(action)?,
            kind: RecoveryEventKind::Progress,
            before_snapshot,
            after_snapshot,
            payload_root,
            effects: effects.into_boxed_slice(),
        })
    }

    /// Returns the exact event identifier.
    #[must_use]
    pub fn event_id(&self) -> &str {
        self.event_id.as_str()
    }

    /// Returns the intervention action label.
    #[must_use]
    pub fn action(&self) -> &str {
        self.action.as_str()
    }

    /// Returns the event kind.
    #[must_use]
    pub const fn kind(&self) -> RecoveryEventKind {
        self.kind
    }

    /// Returns the exact preceding snapshot commitment.
    #[must_use]
    pub const fn before_snapshot(&self) -> RecoverySnapshotCommitment {
        self.before_snapshot
    }

    /// Returns the exact successor snapshot commitment.
    #[must_use]
    pub const fn after_snapshot(&self) -> RecoverySnapshotCommitment {
        self.after_snapshot
    }

    /// Returns the payload commitment.
    #[must_use]
    pub const fn payload_root(&self) -> Hash32 {
        self.payload_root
    }

    /// Returns the canonical exact effect identities.
    #[must_use]
    pub fn effects(&self) -> &[Hash32] {
        &self.effects
    }

    fn is_structurally_valid(&self) -> bool {
        !self.event_id().is_empty()
            && !self.action().is_empty()
            && self.effects.windows(2).all(|pair| pair[0] < pair[1])
            && (self.kind != RecoveryEventKind::Stutter
                || (self.before_snapshot == self.after_snapshot && self.effects.is_empty()))
    }
}

impl CanonicalEncode for RecoveryEvent {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, self.event_id().as_bytes())?;
        put_blob(output, self.action().as_bytes())?;
        output.push(self.kind.tag());
        put_encoded(output, &self.before_snapshot)?;
        put_encoded(output, &self.after_snapshot)?;
        output.extend_from_slice(self.payload_root.as_bytes());
        put_length(output, self.effects.len())?;
        for effect in self.effects() {
            output.extend_from_slice(effect.as_bytes());
        }
        Ok(())
    }
}

/// One exact initial snapshot followed by an ordered intervention word.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecoveryWord {
    word_id: RecoveryWordId,
    initial_snapshot: RecoverySnapshotCommitment,
    events: Box<[RecoveryEvent]>,
}

impl RecoveryWord {
    /// Owns one exact recovery word and rejects duplicate event identities.
    pub fn try_new(
        word_id_value: &str,
        initial_snapshot: RecoverySnapshotCommitment,
        events: Vec<RecoveryEvent>,
    ) -> Result<Self, RecoveryError> {
        let word_id = parse_word_id(word_id_value)?;
        let _ = u32::try_from(events.len()).map_err(|_| RecoveryError::LengthOverflow)?;
        for (index, event) in events.iter().enumerate() {
            if events[..index]
                .iter()
                .any(|prior| prior.event_id == event.event_id)
            {
                return Err(RecoveryError::DuplicateEventId(
                    event.event_id().to_string(),
                ));
            }
        }
        Ok(Self {
            word_id,
            initial_snapshot,
            events: events.into_boxed_slice(),
        })
    }

    /// Returns the word identifier.
    #[must_use]
    pub fn word_id(&self) -> &str {
        self.word_id.as_str()
    }

    /// Returns the exact initial snapshot commitment.
    #[must_use]
    pub const fn initial_snapshot(&self) -> RecoverySnapshotCommitment {
        self.initial_snapshot
    }

    /// Returns the exact ordered events.
    #[must_use]
    pub fn events(&self) -> &[RecoveryEvent] {
        &self.events
    }
}

impl CanonicalEncode for RecoveryWord {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, self.word_id().as_bytes())?;
        put_encoded(output, &self.initial_snapshot)?;
        put_length(output, self.events.len())?;
        for event in self.events() {
            put_encoded(output, event)?;
        }
        Ok(())
    }
}

/// Canonical recovery-word rejection class.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecoveryDefectKind {
    /// The same complete word was supplied more than once.
    DuplicateWord,
    /// One word identifier names distinct content.
    WordIdCollision,
    /// One word repeats an event identity.
    DuplicateEventId,
    /// Adjacent events do not share one exact snapshot commitment.
    ChainMismatch,
    /// A purported stutter changes a snapshot or carries effects.
    InvalidStutter,
    /// Effect identities are duplicate or out of canonical order.
    NonCanonicalEffectSet,
    /// A prefix exposes a mixed durable observation.
    MixedPrefix,
    /// An exact POST snapshot is followed by an exact PRE snapshot.
    PostToPreRegression,
    /// The terminal observation is neither PRE nor POST.
    TerminalNotClosed,
}

impl RecoveryDefectKind {
    const fn tag(self) -> u8 {
        match self {
            Self::DuplicateWord => 0,
            Self::WordIdCollision => 1,
            Self::DuplicateEventId => 2,
            Self::ChainMismatch => 3,
            Self::InvalidStutter => 4,
            Self::NonCanonicalEffectSet => 5,
            Self::MixedPrefix => 6,
            Self::PostToPreRegression => 7,
            Self::TerminalNotClosed => 8,
        }
    }

    const fn prefix_precedence(self) -> u8 {
        match self {
            Self::DuplicateEventId => 0,
            Self::ChainMismatch => 1,
            Self::InvalidStutter => 2,
            Self::NonCanonicalEffectSet => 3,
            Self::MixedPrefix => 4,
            Self::PostToPreRegression => 5,
            Self::TerminalNotClosed => 6,
            Self::DuplicateWord | Self::WordIdCollision => 7,
        }
    }
}

/// Canonical globally earliest recovery defect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBadPrefixWitness {
    kind: RecoveryDefectKind,
    word_id: RecoveryWordId,
    prefix_length: u32,
    event_id: Option<RecoveryEventId>,
    expected: Option<RecoveryObservation>,
    actual: Option<RecoveryObservation>,
    expected_snapshot: Option<RecoverySnapshotCommitment>,
    actual_snapshot: Option<RecoverySnapshotCommitment>,
}

impl RecoveryBadPrefixWitness {
    /// Returns the defect class.
    #[must_use]
    pub const fn kind(&self) -> RecoveryDefectKind {
        self.kind
    }

    /// Returns the affected word identity.
    #[must_use]
    pub fn word_id(&self) -> &str {
        self.word_id.as_str()
    }

    /// Returns the number of events in the first bad prefix.
    #[must_use]
    pub const fn prefix_length(&self) -> u32 {
        self.prefix_length
    }

    /// Returns the event identity at the bad prefix, when one exists.
    #[must_use]
    pub fn event_id(&self) -> Option<&str> {
        self.event_id.as_ref().map(AsciiText::as_str)
    }

    /// Returns the expected coarse observation, when relevant.
    #[must_use]
    pub const fn expected(&self) -> Option<RecoveryObservation> {
        self.expected
    }

    /// Returns the actual coarse observation, when relevant.
    #[must_use]
    pub const fn actual(&self) -> Option<RecoveryObservation> {
        self.actual
    }

    /// Returns the expected exact snapshot commitment for a chain mismatch.
    #[must_use]
    pub const fn expected_snapshot(&self) -> Option<RecoverySnapshotCommitment> {
        self.expected_snapshot
    }

    /// Returns the actual exact snapshot commitment for a chain mismatch.
    #[must_use]
    pub const fn actual_snapshot(&self) -> Option<RecoverySnapshotCommitment> {
        self.actual_snapshot
    }
}

impl CanonicalEncode for RecoveryBadPrefixWitness {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(self.kind.tag());
        put_blob(output, self.word_id().as_bytes())?;
        output.extend_from_slice(&self.prefix_length.to_be_bytes());
        put_optional_text(output, self.event_id.as_ref())?;
        put_optional_observation(output, self.expected);
        put_optional_observation(output, self.actual);
        put_optional_snapshot(output, self.expected_snapshot)?;
        put_optional_snapshot(output, self.actual_snapshot)?;
        Ok(())
    }
}

/// Structural prefix identity containing its exact initial snapshot and event prefix.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecoveryPrefixKey {
    initial_snapshot: RecoverySnapshotCommitment,
    event_prefix: Box<[RecoveryEvent]>,
}

impl RecoveryPrefixKey {
    /// Returns the exact initial snapshot commitment.
    #[must_use]
    pub const fn initial_snapshot(&self) -> RecoverySnapshotCommitment {
        self.initial_snapshot
    }

    /// Returns the exact ordered event prefix.
    #[must_use]
    pub fn event_prefix(&self) -> &[RecoveryEvent] {
        &self.event_prefix
    }
}

impl CanonicalEncode for RecoveryPrefixKey {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_encoded(output, &self.initial_snapshot)?;
        put_length(output, self.event_prefix.len())?;
        for event in self.event_prefix() {
            put_encoded(output, event)?;
        }
        Ok(())
    }
}

/// One canonical node in the exact recovery-prefix trie.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryTrieNode {
    key: RecoveryPrefixKey,
    snapshot: RecoverySnapshotCommitment,
    child_events: Box<[RecoveryEvent]>,
    terminal_word_ids: Box<[RecoveryWordId]>,
}

impl RecoveryTrieNode {
    /// Returns the exact structural prefix key.
    #[must_use]
    pub const fn key(&self) -> &RecoveryPrefixKey {
        &self.key
    }

    /// Returns the exact snapshot reached by this prefix.
    #[must_use]
    pub const fn snapshot(&self) -> RecoverySnapshotCommitment {
        self.snapshot
    }

    /// Returns canonically ordered outgoing events.
    #[must_use]
    pub fn child_events(&self) -> &[RecoveryEvent] {
        &self.child_events
    }

    /// Returns canonically ordered words ending at this node.
    pub fn terminal_word_ids(&self) -> impl Iterator<Item = &str> {
        self.terminal_word_ids.iter().map(AsciiText::as_str)
    }
}

impl CanonicalEncode for RecoveryTrieNode {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_encoded(output, &self.key)?;
        put_encoded(output, &self.snapshot)?;
        put_length(output, self.child_events.len())?;
        for event in self.child_events() {
            put_encoded(output, event)?;
        }
        put_length(output, self.terminal_word_ids.len())?;
        for word_id in &self.terminal_word_ids {
            put_blob(output, word_id.as_str().as_bytes())?;
        }
        Ok(())
    }
}

/// Canonical exact recovery-word trie.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryWordTree {
    words: Box<[RecoveryWord]>,
    nodes: Box<[RecoveryTrieNode]>,
}

impl RecoveryWordTree {
    /// Returns canonically ordered exact recovery words.
    #[must_use]
    pub fn words(&self) -> &[RecoveryWord] {
        &self.words
    }

    /// Returns canonically ordered exact prefix nodes.
    #[must_use]
    pub fn nodes(&self) -> &[RecoveryTrieNode] {
        &self.nodes
    }

    /// Rebuilds all derived nodes and compares the complete value.
    #[must_use]
    pub fn verify(&self) -> bool {
        build_recovery_word_tree(self.words.to_vec()).as_ref() == Ok(self)
    }

    /// Returns the terminal class for one word after complete reconstruction.
    pub fn terminal_class(
        &self,
        word_id_value: &str,
    ) -> Result<Option<RecoveryObservation>, RecoveryError> {
        if !self.verify() {
            return Err(RecoveryError::InvalidTree);
        }
        Ok(self
            .words
            .iter()
            .find(|word| word.word_id() == word_id_value)
            .map(terminal_observation))
    }
}

impl CanonicalEncode for RecoveryWordTree {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&ICRWT_VERSION.to_be_bytes());
        put_length(output, self.words.len())?;
        for word in self.words() {
            put_encoded(output, word)?;
        }
        put_length(output, self.nodes.len())?;
        for node in self.nodes() {
            put_encoded(output, node)?;
        }
        Ok(())
    }
}

/// Recovery value construction or invariant failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryError {
    /// A word identifier is empty.
    EmptyWordId,
    /// A word identifier violates the bounded ASCII profile.
    InvalidWordId(TextError),
    /// An event identifier is empty.
    EmptyEventId,
    /// An event identifier violates the bounded ASCII profile.
    InvalidEventId(TextError),
    /// An action label is empty.
    EmptyAction,
    /// An action label violates the bounded ASCII profile.
    InvalidAction(TextError),
    /// One word repeats an event identity.
    DuplicateEventId(String),
    /// One progress event repeats an effect identity.
    DuplicateEffectIdentity,
    /// A collection cannot be represented by the canonical length field.
    LengthOverflow,
    /// A derived recovery tree failed complete reconstruction.
    InvalidTree,
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyWordId => formatter.write_str("recovery word identifier is empty"),
            Self::InvalidWordId(error) => error.fmt(formatter),
            Self::EmptyEventId => formatter.write_str("recovery event identifier is empty"),
            Self::InvalidEventId(error) => error.fmt(formatter),
            Self::EmptyAction => formatter.write_str("recovery action is empty"),
            Self::InvalidAction(error) => error.fmt(formatter),
            Self::DuplicateEventId(event_id) => {
                write!(formatter, "duplicate recovery event identifier {event_id}")
            }
            Self::DuplicateEffectIdentity => {
                formatter.write_str("duplicate recovery effect identity")
            }
            Self::LengthOverflow => formatter.write_str("recovery collection length overflow"),
            Self::InvalidTree => formatter.write_str("recovery tree failed reconstruction"),
        }
    }
}

impl core::error::Error for RecoveryError {}

#[derive(Clone)]
struct NodeBuilder {
    key: RecoveryPrefixKey,
    snapshot: RecoverySnapshotCommitment,
    child_events: Vec<RecoveryEvent>,
    terminal_word_ids: Vec<RecoveryWordId>,
}

/// Builds a canonical recovery trie or returns one canonical earliest defect.
pub fn build_recovery_word_tree(
    mut words: Vec<RecoveryWord>,
) -> Result<RecoveryWordTree, Box<RecoveryBadPrefixWitness>> {
    words.sort_by(|left, right| left.word_id.cmp(&right.word_id));
    let mut selected: Option<RecoveryBadPrefixWitness> = None;
    let mut group_start = 0;
    while group_start < words.len() {
        let mut group_end = group_start + 1;
        while group_end < words.len() && words[group_end].word_id == words[group_start].word_id {
            group_end += 1;
        }
        if group_end - group_start > 1 {
            let first = &words[group_start];
            let duplicate = words[group_start + 1..group_end]
                .iter()
                .all(|word| word == first);
            select_earlier_witness(
                &mut selected,
                structural_witness(
                    if duplicate {
                        RecoveryDefectKind::DuplicateWord
                    } else {
                        RecoveryDefectKind::WordIdCollision
                    },
                    first,
                ),
            );
        }
        group_start = group_end;
    }

    for word in &words {
        for defect in validate_word(word) {
            select_earlier_witness(&mut selected, defect);
        }
    }
    if let Some(defect) = selected {
        return Err(Box::new(defect));
    }

    let nodes = build_nodes(&words);
    Ok(RecoveryWordTree {
        words: words.into_boxed_slice(),
        nodes: nodes.into_boxed_slice(),
    })
}

fn validate_word(word: &RecoveryWord) -> Vec<RecoveryBadPrefixWitness> {
    let mut defects = Vec::new();
    let mut current_snapshot = word.initial_snapshot;
    let mut current = current_snapshot.observation;
    let mut seen_event_ids: Vec<&str> = Vec::new();
    if current == RecoveryObservation::Mixed {
        defects.push(witness(
            RecoveryDefectKind::MixedPrefix,
            word,
            0,
            None,
            None,
            Some(current),
            None,
            None,
        ));
    }
    for (index, event) in word.events.iter().enumerate() {
        let prefix_length = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if seen_event_ids.contains(&event.event_id()) {
            defects.push(witness(
                RecoveryDefectKind::DuplicateEventId,
                word,
                prefix_length,
                Some(&event.event_id),
                None,
                None,
                None,
                None,
            ));
        }
        seen_event_ids.push(event.event_id());
        if event.before_snapshot != current_snapshot {
            defects.push(witness(
                RecoveryDefectKind::ChainMismatch,
                word,
                prefix_length,
                Some(&event.event_id),
                Some(current),
                Some(event.before_snapshot.observation),
                Some(current_snapshot),
                Some(event.before_snapshot),
            ));
            current_snapshot = event.after_snapshot;
            current = event.after_snapshot.observation;
            continue;
        }
        if !event.effects.windows(2).all(|pair| pair[0] < pair[1]) {
            defects.push(witness(
                RecoveryDefectKind::NonCanonicalEffectSet,
                word,
                prefix_length,
                Some(&event.event_id),
                None,
                None,
                None,
                None,
            ));
        }
        if !event.is_structurally_valid() {
            defects.push(witness(
                RecoveryDefectKind::InvalidStutter,
                word,
                prefix_length,
                Some(&event.event_id),
                Some(event.before_snapshot.observation),
                Some(event.after_snapshot.observation),
                Some(event.before_snapshot),
                Some(event.after_snapshot),
            ));
        }
        if event.after_snapshot.observation == RecoveryObservation::Mixed {
            defects.push(witness(
                RecoveryDefectKind::MixedPrefix,
                word,
                prefix_length,
                Some(&event.event_id),
                None,
                Some(RecoveryObservation::Mixed),
                None,
                Some(event.after_snapshot),
            ));
        }
        if current == RecoveryObservation::Post
            && event.after_snapshot.observation == RecoveryObservation::Pre
        {
            defects.push(witness(
                RecoveryDefectKind::PostToPreRegression,
                word,
                prefix_length,
                Some(&event.event_id),
                Some(RecoveryObservation::Post),
                Some(RecoveryObservation::Pre),
                Some(current_snapshot),
                Some(event.after_snapshot),
            ));
        }
        current_snapshot = event.after_snapshot;
        current = event.after_snapshot.observation;
    }
    if !matches!(
        current,
        RecoveryObservation::Pre | RecoveryObservation::Post
    ) {
        defects.push(witness(
            RecoveryDefectKind::TerminalNotClosed,
            word,
            u32::try_from(word.events.len()).unwrap_or(u32::MAX),
            word.events.last().map(|event| &event.event_id),
            None,
            Some(current),
            None,
            Some(current_snapshot),
        ));
    }
    defects
}

fn build_nodes(words: &[RecoveryWord]) -> Vec<RecoveryTrieNode> {
    let mut nodes: Vec<NodeBuilder> = Vec::new();
    for word in words {
        let mut prefix: Vec<RecoveryEvent> = Vec::new();
        let mut snapshot = word.initial_snapshot;
        let mut node_index = ensure_node(&mut nodes, word.initial_snapshot, &prefix, snapshot);
        for event in &word.events {
            if !nodes[node_index].child_events.contains(event) {
                nodes[node_index].child_events.push(event.clone());
            }
            prefix.push(event.clone());
            snapshot = event.after_snapshot;
            node_index = ensure_node(&mut nodes, word.initial_snapshot, &prefix, snapshot);
        }
        if !nodes[node_index].terminal_word_ids.contains(&word.word_id) {
            nodes[node_index]
                .terminal_word_ids
                .push(word.word_id.clone());
        }
    }
    let mut finished = nodes
        .into_iter()
        .map(|mut node| {
            node.child_events.sort();
            node.terminal_word_ids.sort();
            RecoveryTrieNode {
                key: node.key,
                snapshot: node.snapshot,
                child_events: node.child_events.into_boxed_slice(),
                terminal_word_ids: node.terminal_word_ids.into_boxed_slice(),
            }
        })
        .collect::<Vec<_>>();
    finished.sort_by(|left, right| left.key.cmp(&right.key));
    finished
}

fn ensure_node(
    nodes: &mut Vec<NodeBuilder>,
    initial_snapshot: RecoverySnapshotCommitment,
    prefix: &[RecoveryEvent],
    snapshot: RecoverySnapshotCommitment,
) -> usize {
    let key = RecoveryPrefixKey {
        initial_snapshot,
        event_prefix: prefix.to_vec().into_boxed_slice(),
    };
    if let Some(index) = nodes.iter().position(|node| node.key == key) {
        return index;
    }
    nodes.push(NodeBuilder {
        key,
        snapshot,
        child_events: Vec::new(),
        terminal_word_ids: Vec::new(),
    });
    nodes.len() - 1
}

fn terminal_observation(word: &RecoveryWord) -> RecoveryObservation {
    word.events
        .last()
        .map_or(word.initial_snapshot.observation, |event| {
            event.after_snapshot.observation
        })
}

fn compare_witness(left: &RecoveryBadPrefixWitness, right: &RecoveryBadPrefixWitness) -> Ordering {
    (
        left.prefix_length,
        left.kind.prefix_precedence(),
        &left.word_id,
        &left.event_id,
    )
        .cmp(&(
            right.prefix_length,
            right.kind.prefix_precedence(),
            &right.word_id,
            &right.event_id,
        ))
}

fn select_earlier_witness(
    selected: &mut Option<RecoveryBadPrefixWitness>,
    candidate: RecoveryBadPrefixWitness,
) {
    if selected
        .as_ref()
        .is_none_or(|current| compare_witness(&candidate, current) == Ordering::Less)
    {
        *selected = Some(candidate);
    }
}

fn structural_witness(kind: RecoveryDefectKind, word: &RecoveryWord) -> RecoveryBadPrefixWitness {
    witness(kind, word, 0, None, None, None, None, None)
}

#[allow(clippy::too_many_arguments)]
fn witness(
    kind: RecoveryDefectKind,
    word: &RecoveryWord,
    prefix_length: u32,
    event_id: Option<&RecoveryEventId>,
    expected: Option<RecoveryObservation>,
    actual: Option<RecoveryObservation>,
    expected_snapshot: Option<RecoverySnapshotCommitment>,
    actual_snapshot: Option<RecoverySnapshotCommitment>,
) -> RecoveryBadPrefixWitness {
    RecoveryBadPrefixWitness {
        kind,
        word_id: word.word_id.clone(),
        prefix_length,
        event_id: event_id.cloned(),
        expected,
        actual,
        expected_snapshot,
        actual_snapshot,
    }
}

fn parse_word_id(value: &str) -> Result<RecoveryWordId, RecoveryError> {
    if value.is_empty() {
        return Err(RecoveryError::EmptyWordId);
    }
    RecoveryWordId::try_from_string(value.to_string()).map_err(RecoveryError::InvalidWordId)
}

fn parse_event_id(value: &str) -> Result<RecoveryEventId, RecoveryError> {
    if value.is_empty() {
        return Err(RecoveryError::EmptyEventId);
    }
    RecoveryEventId::try_from_string(value.to_string()).map_err(RecoveryError::InvalidEventId)
}

fn parse_action(value: &str) -> Result<RecoveryAction, RecoveryError> {
    if value.is_empty() {
        return Err(RecoveryError::EmptyAction);
    }
    RecoveryAction::try_from_string(value.to_string()).map_err(RecoveryError::InvalidAction)
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_encoded<T: CanonicalEncode>(output: &mut Vec<u8>, value: &T) -> Result<(), EncodeError> {
    put_blob(output, &value.canonical_bytes()?)
}

fn put_optional_text<const MAX: usize>(
    output: &mut Vec<u8>,
    value: Option<&AsciiText<MAX>>,
) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_blob(output, value.as_str().as_bytes())?;
        }
    }
    Ok(())
}

fn put_optional_observation(output: &mut Vec<u8>, value: Option<RecoveryObservation>) {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            output.push(value.tag());
        }
    }
}

fn put_optional_snapshot(
    output: &mut Vec<u8>,
    value: Option<RecoverySnapshotCommitment>,
) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_encoded(output, &value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn hash(value: u8) -> Hash32 {
        Hash32::new([value; 32])
    }

    fn snapshot(observation: RecoveryObservation, value: u8) -> RecoverySnapshotCommitment {
        RecoverySnapshotCommitment::new(
            observation,
            hash(value),
            hash(value.wrapping_add(1)),
            hash(value.wrapping_add(2)),
        )
    }

    fn progress(
        event_id: &str,
        before: RecoverySnapshotCommitment,
        after: RecoverySnapshotCommitment,
    ) -> RecoveryEvent {
        RecoveryEvent::try_progress(event_id, "step", before, after, hash(200), Vec::new())
            .unwrap_or_else(|error| panic!("progress construction failed: {error}"))
    }

    fn word(
        word_id: &str,
        initial: RecoverySnapshotCommitment,
        events: Vec<RecoveryEvent>,
    ) -> RecoveryWord {
        RecoveryWord::try_new(word_id, initial, events)
            .unwrap_or_else(|error| panic!("word construction failed: {error}"))
    }

    #[test]
    fn same_class_snapshot_splice_is_rejected() {
        let initial = snapshot(RecoveryObservation::Pre, 1);
        let first_after = snapshot(RecoveryObservation::Pre, 10);
        let foreign_before = snapshot(RecoveryObservation::Pre, 20);
        let terminal = snapshot(RecoveryObservation::Post, 30);
        let result = build_recovery_word_tree(vec![word(
            "spliced",
            initial,
            vec![
                progress("first", initial, first_after),
                progress("second", foreign_before, terminal),
            ],
        )]);
        let Err(witness) = result else {
            panic!("snapshot splice was accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::ChainMismatch);
        assert_eq!(witness.prefix_length(), 2);
        assert_eq!(witness.expected_snapshot(), Some(first_after));
        assert_eq!(witness.actual_snapshot(), Some(foreign_before));
    }

    #[test]
    fn prefix_identity_binds_the_initial_snapshot() {
        let pre = word("pre", snapshot(RecoveryObservation::Pre, 1), Vec::new());
        let post = word("post", snapshot(RecoveryObservation::Post, 1), Vec::new());
        let tree = build_recovery_word_tree(vec![pre, post])
            .unwrap_or_else(|witness| panic!("tree rejected: {:?}", witness.kind()));
        assert_eq!(tree.nodes().len(), 2);
        assert_ne!(tree.nodes()[0].key(), tree.nodes()[1].key());
    }

    #[test]
    fn defect_precedence_is_applied_before_word_identity() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let mixed = snapshot(RecoveryObservation::Mixed, 2);
        let mixed_word = word("a-mixed", pre, vec![progress("mixed", pre, mixed)]);
        let foreign = snapshot(RecoveryObservation::Pre, 3);
        let post = snapshot(RecoveryObservation::Post, 4);
        let chain_word = word("z-chain", pre, vec![progress("chain", foreign, post)]);
        let Err(witness) = build_recovery_word_tree(vec![mixed_word, chain_word]) else {
            panic!("invalid words were accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::ChainMismatch);
        assert_eq!(witness.word_id(), "z-chain");
    }

    #[test]
    fn builder_revalidates_stutter_and_effect_closure() {
        let before = snapshot(RecoveryObservation::Pre, 1);
        let after = snapshot(RecoveryObservation::Pre, 2);
        let forged = RecoveryEvent {
            event_id: parse_event_id("forged").unwrap_or_else(|error| panic!("id failed: {error}")),
            action: parse_action("retry").unwrap_or_else(|error| panic!("action failed: {error}")),
            kind: RecoveryEventKind::Stutter,
            before_snapshot: before,
            after_snapshot: after,
            payload_root: hash(10),
            effects: vec![hash(11)].into_boxed_slice(),
        };
        let Err(witness) = build_recovery_word_tree(vec![word("forged", before, vec![forged])])
        else {
            panic!("invalid stutter was accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::InvalidStutter);
    }

    #[test]
    fn builder_revalidates_unique_event_identity() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let repeated = RecoveryEvent::try_stutter("same", "retry", pre, hash(2))
            .unwrap_or_else(|error| panic!("stutter failed: {error}"));
        let forged = RecoveryWord {
            word_id: parse_word_id("forged").unwrap_or_else(|error| panic!("id failed: {error}")),
            initial_snapshot: pre,
            events: vec![repeated.clone(), repeated].into_boxed_slice(),
        };
        let Err(witness) = build_recovery_word_tree(vec![forged]) else {
            panic!("duplicate event identity was accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::DuplicateEventId);
        assert_eq!(witness.prefix_length(), 2);
    }

    #[test]
    fn mixed_prefix_survives_later_repair() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let mixed = snapshot(RecoveryObservation::Mixed, 2);
        let post = snapshot(RecoveryObservation::Post, 3);
        let Err(witness) = build_recovery_word_tree(vec![word(
            "mixed-post",
            pre,
            vec![
                progress("mixed", pre, mixed),
                progress("repair", mixed, post),
            ],
        )]) else {
            panic!("mixed prefix was erased");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::MixedPrefix);
        assert_eq!(witness.prefix_length(), 1);
    }

    #[test]
    fn post_to_pre_regression_is_rejected() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let post = snapshot(RecoveryObservation::Post, 2);
        let Err(witness) = build_recovery_word_tree(vec![word(
            "regress",
            pre,
            vec![
                progress("commit", pre, post),
                progress("rollback", post, pre),
            ],
        )]) else {
            panic!("POST to PRE regression was accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::PostToPreRegression);
    }

    #[test]
    fn input_order_is_canonical_and_queries_reconstruct() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let post = snapshot(RecoveryObservation::Post, 2);
        let first_word = word("pre", pre, Vec::new());
        let second_word = word("post", pre, vec![progress("commit", pre, post)]);
        let first = build_recovery_word_tree(vec![first_word.clone(), second_word.clone()])
            .unwrap_or_else(|witness| panic!("tree failed: {:?}", witness.kind()));
        let second = build_recovery_word_tree(vec![second_word, first_word])
            .unwrap_or_else(|witness| panic!("tree failed: {:?}", witness.kind()));
        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert!(first.verify());
        assert_eq!(
            first.terminal_class("pre"),
            Ok(Some(RecoveryObservation::Pre))
        );
        assert_eq!(
            first.terminal_class("post"),
            Ok(Some(RecoveryObservation::Post))
        );
    }

    #[test]
    fn duplicate_words_and_word_id_collisions_are_distinct() {
        let pre = snapshot(RecoveryObservation::Pre, 1);
        let first = word("same", pre, Vec::new());
        let Err(duplicate) = build_recovery_word_tree(vec![first.clone(), first.clone()]) else {
            panic!("duplicate word was accepted");
        };
        assert_eq!(duplicate.kind(), RecoveryDefectKind::DuplicateWord);
        let changed = word(
            "same",
            pre,
            vec![
                RecoveryEvent::try_stutter("retry", "retry", pre, hash(2))
                    .unwrap_or_else(|error| panic!("stutter failed: {error}")),
            ],
        );
        let Err(collision) = build_recovery_word_tree(vec![first, changed]) else {
            panic!("word identity collision was accepted");
        };
        assert_eq!(collision.kind(), RecoveryDefectKind::WordIdCollision);
    }

    #[test]
    fn duplicate_word_does_not_hide_higher_priority_prefix_defect() {
        let mixed = word(
            "same-mixed",
            snapshot(RecoveryObservation::Mixed, 1),
            Vec::new(),
        );
        let Err(witness) = build_recovery_word_tree(vec![mixed.clone(), mixed]) else {
            panic!("invalid duplicate words were accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::MixedPrefix);
        assert_eq!(witness.prefix_length(), 0);
        assert_eq!(witness.word_id(), "same-mixed");
    }

    #[test]
    fn duplicate_group_does_not_hide_another_words_prefix_defect() {
        let duplicate = word(
            "a-duplicate",
            snapshot(RecoveryObservation::Pre, 1),
            Vec::new(),
        );
        let mixed = word(
            "z-mixed",
            snapshot(RecoveryObservation::Mixed, 2),
            Vec::new(),
        );
        let Err(witness) = build_recovery_word_tree(vec![duplicate.clone(), duplicate, mixed])
        else {
            panic!("invalid words were accepted");
        };
        assert_eq!(witness.kind(), RecoveryDefectKind::MixedPrefix);
        assert_eq!(witness.prefix_length(), 0);
        assert_eq!(witness.word_id(), "z-mixed");
    }
}
