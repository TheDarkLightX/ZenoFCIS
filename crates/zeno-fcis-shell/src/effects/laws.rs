//! The registry's nine handler laws, checked on every handled request.
//!
//! Each law is stated here independently of the simulation's own code: it
//! compares the world before and after a request with the answer. Success
//! equations assume a fault-free, admitted call; the frames cover failures.
//! [`Laws::probe`] also drives requests no program makes, such as changed
//! bytes under a key or an unknown delivery ID, from every reached store.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::num::NonZeroU64;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::codec;
use super::driver::Binding;
use super::handler::{Handled, Stack};
use super::port::{
    Code, CommitMode, Delivery, EpochTick, Fault, HistoryEntry, OpId, Operation, Outcome,
    PublicationData, Purpose, Response, Sent, Snapshot, WaitError,
};
use super::simulation::{self, Durable, IDENTITY, Profile, Script, Simulation, World};
use crate::CommitStatus;

/// A law that failed, and how.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawFailure {
    /// The law's registry name.
    pub law: &'static str,
    /// What broke.
    pub detail: &'static str,
}

/// Law checks performed and the branches they witnessed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Laws {
    /// Requests checked.
    pub checks: u64,
    /// How often each law branch was witnessed.
    pub witnesses: BTreeMap<&'static str, u64>,
}

/// The branches every qualified stack must witness at least once.
pub const REQUIRED: &[&str] = &[
    "commit-once/key-present",
    "commit-once/replay",
    "commit-once/changed-refused",
    "commit-once/second-replays",
    "commit-atomic/applied",
    "commit-atomic/unchanged",
    "commit-atomic/unknown",
    "read-state/exact",
    "read-exact/history",
    "read-exact/outbox",
    "read-exact/repeat",
    "send-exact/sent",
    "send-exact/duplicate",
    "send-exact/lost",
    "acknowledge-exact/applied",
    "acknowledge-exact/already",
    "acknowledge-exact/unknown-id",
    "acknowledge-exact/mismatch",
    "reopen-audits/exact",
    "time-monotone/clock",
    "time-monotone/wake",
    "time-monotone/interrupted",
    "jitter-in-range/draw",
    "fault/full",
    "fault/clock-busy",
    "fault/clock-full",
    "fault/clock-io",
    "fault/random-busy",
    "fault/random-full",
    "fault/random-io",
    "fault/wait-busy",
    "fault/wait-full",
    "fault/wait-io",
];

fn fail(law: &'static str, detail: &'static str) -> Result<(), LawFailure> {
    Err(LawFailure { law, detail })
}

fn sha256(parts: &[&[u8]]) -> Hash32 {
    let mut out = Vec::new();
    for part in parts {
        codec::bytes(&mut out, part);
    }
    RustCryptoSha256::hash(&out)
}

fn head(durable: &Durable) -> Snapshot {
    Snapshot {
        version: durable.commits.len() as u64,
        identity: IDENTITY.to_vec(),
        state: durable.state.clone(),
        root: RustCryptoSha256::hash(&durable.state),
        chain: durable.chain,
    }
}

/// Whether `after` is `before` with exactly this one publication applied:
/// state, chain, history entry, replay key, receipt and both outbox rows.
fn applied(before: &Durable, after: &Durable, key: Hash32, data: &PublicationData) -> bool {
    expected_commit(before, key, data).as_ref() == Some(after)
}

// Independent specification of the one-byte application and complete outbox.
// This path never calls Simulation, decide, publication or deliveries.
fn expected_commit(before: &Durable, key: Hash32, data: &PublicationData) -> Option<Durable> {
    if data.prestate != before.state
        || data.identity != IDENTITY
        || data.context != [1]
        || data.command.len() != 1
        || data.subject
            != sha256(&[IDENTITY, &data.prestate, &data.command, &data.context]).as_bytes()
        || before.replays.contains(&key)
    {
        return None;
    }
    let next = before.state.first()?.checked_add(*data.command.first()?)?;
    let version = u64::try_from(before.commits.len()).ok()?.checked_add(1)?;
    let chain = sha256(&[before.chain.as_bytes(), &data.subject]);
    let mut expected = before.clone();
    expected.state = vec![next];
    expected.chain = chain;
    expected.commits.push(HistoryEntry {
        key,
        version,
        publication: data.clone(),
    });
    expected.replays.push(key);
    expected.receipts.push(chain);
    for ordinal in [0_u32, 1] {
        let payload = vec![next, u8::try_from(ordinal).ok()?];
        expected.outbox.push((
            Delivery {
                id: sha256(&[b"delivery", chain.as_bytes(), &ordinal.to_be_bytes()]),
                version,
                lane: 1,
                ordinal,
                destination: b"receiver".to_vec(),
                payload_sha256: RustCryptoSha256::hash(&payload),
                payload,
            },
            false,
        ));
    }
    Some(expected)
}

/// Indices whose acknowledged bit differs, or `None` if anything else in D did.
fn ack_changes(before: &Durable, after: &Durable) -> Option<Vec<usize>> {
    if before.state != after.state
        || before.chain != after.chain
        || before.commits != after.commits
        || before.receipts != after.receipts
        || before.replays != after.replays
        || before.outbox.len() != after.outbox.len()
    {
        return None;
    }
    let mut changed = Vec::new();
    for (index, (old, new)) in before.outbox.iter().zip(&after.outbox).enumerate() {
        if old.0 != new.0 {
            return None;
        }
        if old.1 != new.1 {
            changed.push(index);
        }
    }
    Some(changed)
}

impl Laws {
    fn saw(&mut self, branch: &'static str) {
        *self.witnesses.entry(branch).or_insert(0) += 1;
    }

    /// The required branches this run never witnessed.
    #[must_use]
    pub fn unwitnessed(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        for branch in REQUIRED {
            if !self.witnesses.contains_key(branch) {
                missing.push(*branch);
            }
        }
        missing
    }

    /// Adds another run's counts.
    pub fn merge(&mut self, other: &Self) {
        self.checks += other.checks;
        for (branch, count) in &other.witnesses {
            *self.witnesses.entry(branch).or_insert(0) += count;
        }
    }

    /// Checks one handled request against its port's laws.
    pub fn check(
        &mut self,
        profile: &Profile,
        before: &World,
        binding: &Binding,
        operation: &Operation,
        handled: &Handled,
        after: &World,
    ) -> Result<(), LawFailure> {
        self.checks += 1;
        let (old, new) = (&before.durable, &after.durable);
        let unchanged = old == new;
        let answer = match handled {
            Handled::Answer(response) => Some(response),
            Handled::Crash => None,
            Handled::Suspend => return Ok(()),
        };
        // No request changes the receiver; only the transport sends copies.
        if before.dedup != after.dedup || before.effects != after.effects {
            return fail("send-exact", "a request changed the receiver");
        }
        if !matches!(operation, Operation::Send(..)) && before.queue != after.queue {
            return fail(
                "send-exact",
                "a request other than Send changed the transport",
            );
        }
        if let Some(response) = answer {
            if operation.port() != super::registry::Port::Send
                && *response == Response::not_applied(operation.port(), Fault::Full)
            {
                self.saw("fault/full");
            }
            let branch = match response {
                Response::Clock(Err(Fault::Busy)) => Some("fault/clock-busy"),
                Response::Clock(Err(Fault::Full)) => Some("fault/clock-full"),
                Response::Clock(Err(Fault::Io)) => Some("fault/clock-io"),
                Response::Random(Err(Fault::Busy)) => Some("fault/random-busy"),
                Response::Random(Err(Fault::Full)) => Some("fault/random-full"),
                Response::Random(Err(Fault::Io)) => Some("fault/random-io"),
                Response::Wait(Err(WaitError::Failed(Fault::Busy))) => Some("fault/wait-busy"),
                Response::Wait(Err(WaitError::Failed(Fault::Full))) => Some("fault/wait-full"),
                Response::Wait(Err(WaitError::Failed(Fault::Io))) => Some("fault/wait-io"),
                _ => None,
            };
            if let Some(branch) = branch {
                self.saw(branch);
            }
        }
        match operation {
            Operation::Commit(_, key, data, mode) => {
                self.commit(old, new, *key, data, *mode, answer)
            }
            Operation::ReadState(_) => {
                if !unchanged {
                    return fail("read-state", "a read changed D");
                }
                match answer {
                    Some(Response::ReadState(Outcome::Ok(snapshot))) => {
                        if *snapshot != head(old) {
                            return fail("read-state", "the snapshot is not the head");
                        }
                        self.saw("read-state/exact");
                        Ok(())
                    }
                    Some(Response::ReadState(Outcome::NotApplied(_) | Outcome::Refused(_)))
                    | None => Ok(()),
                    _ => fail("read-state", "unexpected answer"),
                }
            }
            Operation::ReadHistory(_) | Operation::ReadOutbox(_) => {
                if !unchanged {
                    return fail("read-exact", "a read changed D");
                }
                match answer {
                    Some(Response::ReadHistory(Outcome::Ok(history))) => {
                        if history.head != head(old) || history.commits != old.commits {
                            return fail("read-exact", "history is not exact");
                        }
                        self.saw("read-exact/history");
                        Ok(())
                    }
                    Some(Response::ReadOutbox(Outcome::Ok(pending))) => {
                        let exact: Vec<_> = old
                            .outbox
                            .iter()
                            .filter(|(_, acked)| !acked)
                            .map(|(delivery, _)| delivery)
                            .collect();
                        if pending.len() != exact.len()
                            || pending.iter().zip(exact).any(|(got, want)| got != want)
                        {
                            return fail("read-exact", "the outbox is not exact");
                        }
                        self.saw("read-exact/outbox");
                        Ok(())
                    }
                    Some(
                        Response::ReadHistory(Outcome::NotApplied(_) | Outcome::Refused(_))
                        | Response::ReadOutbox(Outcome::NotApplied(_) | Outcome::Refused(_)),
                    )
                    | None => Ok(()),
                    _ => fail("read-exact", "unexpected answer"),
                }
            }
            Operation::Reopen(store) => {
                if !unchanged {
                    return fail("reopen-audits", "reopen changed D");
                }
                match answer {
                    Some(Response::Reopen(Outcome::Ok(snapshot))) => {
                        let fresh = (store.handle as usize)
                            .checked_sub(1)
                            .and_then(|index| after.handles.get(index).copied());
                        if *snapshot != head(old) || fresh != Some(Some(old.commits.len() as u64)) {
                            return fail("reopen-audits", "reopen kept volatile state");
                        }
                        self.saw("reopen-audits/exact");
                        Ok(())
                    }
                    Some(Response::Reopen(Outcome::NotApplied(_) | Outcome::Refused(_))) | None => {
                        Ok(())
                    }
                    _ => fail("reopen-audits", "unexpected answer"),
                }
            }
            Operation::Send(_, delivery) => {
                if !unchanged {
                    return fail("send-exact", "a send changed D");
                }
                let kept = before.queue.len();
                if after.queue.len() < kept
                    || after.queue[..kept] != before.queue[..]
                    || after.queue[kept..].iter().any(|copy| copy != delivery)
                    || after.queue.len() > kept + 2
                {
                    return fail("send-exact", "a copy is not exact");
                }
                let copies = after.queue.len() - kept;
                match answer {
                    Some(Response::Send(Sent::Sent(report))) => {
                        if report.delivery_id != delivery.id
                            || report.payload_sha256 != delivery.payload_sha256
                            || copies == 0
                        {
                            return fail("send-exact", "the report does not match a copy");
                        }
                        self.saw(if copies == 2 {
                            "send-exact/duplicate"
                        } else {
                            "send-exact/sent"
                        });
                        Ok(())
                    }
                    Some(Response::Send(Sent::NotSent(_) | Sent::Refused(_))) => {
                        if copies != 0 {
                            return fail("send-exact", "not sent, yet a copy left");
                        }
                        Ok(())
                    }
                    Some(Response::Send(Sent::Unknown(_))) => {
                        self.saw("send-exact/lost");
                        Ok(())
                    }
                    None => Ok(()),
                    _ => fail("send-exact", "unexpected answer"),
                }
            }
            Operation::Acknowledge(_, id, payload_sha256) => {
                self.acknowledge(old, new, *id, *payload_sha256, answer)
            }
            Operation::Clock => {
                if !unchanged {
                    return fail("time-monotone", "the clock changed D");
                }
                match answer {
                    Some(Response::Clock(Ok(tick))) => {
                        if tick.epoch != binding.id.epoch
                            || tick.tick < before.tick
                            || tick.tick > profile.ticks
                        {
                            return fail("time-monotone", "the clock went back");
                        }
                        self.saw("time-monotone/clock");
                        Ok(())
                    }
                    Some(Response::Clock(Err(_))) | None => Ok(()),
                    _ => fail("time-monotone", "unexpected answer"),
                }
            }
            Operation::Wait(deadline) => {
                if !unchanged {
                    return fail("time-monotone", "a wait changed D");
                }
                match answer {
                    Some(Response::Wait(Ok(tick))) => {
                        if tick.epoch != binding.id.epoch
                            || tick.tick < deadline.tick
                            || tick.tick < before.tick
                        {
                            return fail("time-monotone", "woke before the deadline");
                        }
                        self.saw("time-monotone/wake");
                        Ok(())
                    }
                    Some(Response::Wait(Err(WaitError::Interrupted))) => {
                        self.saw("time-monotone/interrupted");
                        Ok(())
                    }
                    Some(Response::Wait(Err(WaitError::Failed(_)))) | None => Ok(()),
                    _ => fail("time-monotone", "unexpected answer"),
                }
            }
            Operation::Random(_, bound) => {
                if !unchanged {
                    return fail("jitter-in-range", "a draw changed D");
                }
                match answer {
                    Some(Response::Random(Ok(draw))) => {
                        if *draw >= bound.get() {
                            return fail("jitter-in-range", "the draw is not below its bound");
                        }
                        self.saw("jitter-in-range/draw");
                        Ok(())
                    }
                    Some(Response::Random(Err(_))) | None => Ok(()),
                    _ => fail("jitter-in-range", "unexpected answer"),
                }
            }
        }
    }

    fn commit(
        &mut self,
        old: &Durable,
        new: &Durable,
        key: Hash32,
        data: &PublicationData,
        mode: CommitMode,
        answer: Option<&Response>,
    ) -> Result<(), LawFailure> {
        let unchanged = old == new;
        let apply = applied(old, new, key, data);
        if !unchanged && !apply {
            return fail(
                "commit-atomic",
                "D is neither the pre-state nor the complete post-state",
            );
        }
        let existing = old.replays.iter().position(|replay| *replay == key);
        if existing.is_some() {
            if !unchanged {
                return fail("commit-once", "a present key applied again");
            }
            self.saw("commit-once/key-present");
        }
        match answer {
            Some(Response::Commit(Outcome::Ok(receipt))) => match receipt.status {
                CommitStatus::Committed => {
                    if !apply
                        || receipt.version != new.commits.len() as u64
                        || receipt.chain != new.chain
                    {
                        return fail("commit-atomic", "a committed receipt without its commit");
                    }
                    self.saw("commit-atomic/applied");
                    Ok(())
                }
                CommitStatus::IdempotentReplay => {
                    let original = existing.and_then(|index| {
                        let entry = old.commits.get(index)?;
                        let chain = old.receipts.get(index)?;
                        Some((entry, *chain))
                    });
                    match original {
                        Some((entry, chain))
                            if unchanged
                                && entry.publication == *data
                                && receipt.version == entry.version
                                && receipt.chain == chain =>
                        {
                            self.saw("commit-once/replay");
                            Ok(())
                        }
                        _ => fail("commit-once", "a replay without the original receipt"),
                    }
                }
            },
            Some(Response::Commit(Outcome::Refused(code))) => {
                if !unchanged {
                    return fail("commit-atomic", "a refusal changed D");
                }
                let version = old.commits.len() as u64;
                let guarded = match mode {
                    CommitMode::Unversioned => None,
                    CommitMode::At(expected) | CommitMode::BoundedAt(expected, _) => Some(expected),
                };
                match code {
                    Code::Concurrent if guarded.is_some_and(|expected| expected != version) => {}
                    Code::Replay => match existing.and_then(|index| old.commits.get(index)) {
                        Some(entry) if entry.publication != *data => {
                            self.saw("commit-once/changed-refused");
                        }
                        _ => return fail("commit-once", "a replay refusal without changed bytes"),
                    },
                    Code::Concurrent => {
                        return fail("commit-once", "a concurrency refusal at the right version");
                    }
                    _ => {}
                }
                self.saw("commit-atomic/unchanged");
                Ok(())
            }
            Some(Response::Commit(Outcome::NotApplied(_))) => {
                if !unchanged {
                    return fail("commit-atomic", "not applied, yet D changed");
                }
                self.saw("commit-atomic/unchanged");
                Ok(())
            }
            Some(Response::Commit(Outcome::Unknown(_))) | None => {
                self.saw("commit-atomic/unknown");
                Ok(())
            }
            _ => fail("commit-atomic", "unexpected answer"),
        }
    }

    fn acknowledge(
        &mut self,
        old: &Durable,
        new: &Durable,
        id: Hash32,
        payload_sha256: Hash32,
        answer: Option<&Response>,
    ) -> Result<(), LawFailure> {
        let Some(changed) = ack_changes(old, new) else {
            return fail(
                "acknowledge-exact",
                "an acknowledgement changed more than a bit",
            );
        };
        let record = old
            .outbox
            .iter()
            .position(|(delivery, _)| delivery.id == id);
        let matching = record.filter(|index| {
            let (delivery, acked) = &old.outbox[*index];
            delivery.payload_sha256 == payload_sha256 && !acked
        });
        let only_matching = match matching {
            Some(index) => changed.is_empty() || changed == [index],
            None => changed.is_empty(),
        };
        if !only_matching {
            return fail("acknowledge-exact", "another record's bit changed");
        }
        match answer {
            Some(Response::Acknowledge(Outcome::Ok(()))) => match matching {
                Some(index) if changed == [index] => {
                    self.saw("acknowledge-exact/applied");
                    Ok(())
                }
                _ => fail("acknowledge-exact", "acknowledged without setting its bit"),
            },
            Some(Response::Acknowledge(Outcome::Refused(code))) => {
                if !changed.is_empty() {
                    return fail("acknowledge-exact", "a refusal changed D");
                }
                let witness = match (code, record) {
                    (Code::UnknownDelivery, None) => "acknowledge-exact/unknown-id",
                    (Code::PayloadMismatch, Some(index))
                        if old.outbox[index].0.payload_sha256 != payload_sha256 =>
                    {
                        "acknowledge-exact/mismatch"
                    }
                    (Code::AlreadyAcknowledged, Some(index))
                        if old.outbox[index].1
                            && old.outbox[index].0.payload_sha256 == payload_sha256 =>
                    {
                        "acknowledge-exact/already"
                    }
                    _ => return fail("acknowledge-exact", "the refusal does not match the record"),
                };
                self.saw(witness);
                Ok(())
            }
            Some(Response::Acknowledge(Outcome::NotApplied(_))) => {
                if !changed.is_empty() {
                    return fail("acknowledge-exact", "not applied, yet a bit changed");
                }
                Ok(())
            }
            Some(Response::Acknowledge(Outcome::Unknown(_))) | None => Ok(()),
            _ => fail("acknowledge-exact", "unexpected answer"),
        }
    }

    /// Drives the probe requests from `durable` under every choice, through
    /// `stack` and a simulation covering every port, checking each law.
    /// It also checks that a repeated commit replays the first receipt and
    /// that a repeated outbox read agrees.
    pub fn probe(
        &mut self,
        profile: &Profile,
        durable: &Durable,
        stack: &Stack,
    ) -> Result<(), LawFailure> {
        let binding_of = |operation: &Operation| Binding::of(OpId::first(1), operation);
        let mut queue_full = World::new(durable.clone());
        if let Some((delivery, _)) = durable.outbox.first() {
            queue_full.queue = vec![delivery.clone(); profile.queue];
        }
        for world in [World::new(durable.clone()), queue_full] {
            for operation in probes(profile, durable) {
                let binding = binding_of(&operation);
                let mut choice = 0;
                loop {
                    let mut simulation = Simulation::new(
                        world.clone(),
                        *profile,
                        simulation::ALL_PORTS,
                        Script::new(vec![choice]),
                    );
                    let mut layers = stack.clone();
                    let handled = layers.handle(&mut simulation, &binding, &operation);
                    if handled == Handled::Suspend {
                        break;
                    }
                    self.check(
                        profile,
                        &world,
                        &binding,
                        &operation,
                        &handled,
                        &simulation.world,
                    )?;
                    if simulation.script.made().is_empty() {
                        break;
                    }
                    choice += 1;
                }
            }
        }
        self.repeat(profile, durable, stack)
    }

    fn repeat(
        &mut self,
        profile: &Profile,
        durable: &Durable,
        stack: &Stack,
    ) -> Result<(), LawFailure> {
        let mut simulation = Simulation::new(
            World::new(durable.clone()),
            *profile,
            simulation::ALL_PORTS,
            Script::fault_free(),
        );
        let mut layers = stack.clone();
        let data = simulation::publication(&durable.state, 1);
        let commit = Operation::Commit(
            simulation::store(1),
            Hash32::new([0x7e; 32]),
            data,
            CommitMode::Unversioned,
        );
        let first = layers.handle(
            &mut simulation,
            &Binding::of(OpId::first(1), &commit),
            &commit,
        );
        let second = layers.handle(
            &mut simulation,
            &Binding::of(OpId::first(2), &commit),
            &commit,
        );
        match (first, second) {
            (
                Handled::Answer(Response::Commit(Outcome::Ok(one))),
                Handled::Answer(Response::Commit(Outcome::Ok(two))),
            ) if one.status == CommitStatus::Committed
                && two.status == CommitStatus::IdempotentReplay
                && (one.version, one.chain) == (two.version, two.chain) =>
            {
                self.saw("commit-once/second-replays");
            }
            (Handled::Answer(Response::Commit(Outcome::Refused(_))), _) => {}
            _ => {
                return fail(
                    "commit-once",
                    "a repeated commit did not replay the first receipt",
                );
            }
        }
        let read = Operation::ReadOutbox(simulation::store(1));
        let one = layers.handle(&mut simulation, &Binding::of(OpId::first(3), &read), &read);
        let two = layers.handle(&mut simulation, &Binding::of(OpId::first(4), &read), &read);
        if one != two {
            return fail("read-exact", "repeated reads disagree");
        }
        self.saw("read-exact/repeat");
        Ok(())
    }
}

/// The probe requests from one store state.
fn probes(profile: &Profile, durable: &Durable) -> Vec<Operation> {
    let (one, two) = (simulation::store(1), simulation::store(2));
    let version = durable.commits.len() as u64;
    let fresh = Hash32::new([0x7f; 32]);
    let data = simulation::publication(&durable.state, 1);
    let mut forged = data.clone();
    if let Some(byte) = forged.subject.first_mut() {
        *byte ^= 1;
    }
    let mut operations = vec![
        Operation::ReadState(one),
        Operation::ReadState(two),
        Operation::ReadHistory(one),
        Operation::ReadOutbox(two),
        Operation::Reopen(one),
        Operation::Reopen(two),
        Operation::Commit(one, fresh, data.clone(), CommitMode::At(version)),
        Operation::Commit(two, fresh, data.clone(), CommitMode::Unversioned),
        Operation::Commit(one, fresh, data.clone(), CommitMode::At(version + 1)),
        Operation::Commit(one, fresh, data.clone(), CommitMode::BoundedAt(version, 0)),
        Operation::Commit(one, fresh, forged, CommitMode::Unversioned),
        Operation::Commit(
            one,
            fresh,
            simulation::publication(&[9], 1),
            CommitMode::Unversioned,
        ),
        Operation::Acknowledge(one, Hash32::new([0xee; 32]), Hash32::ZERO),
        Operation::Clock,
        Operation::Random(Purpose::RetryJitter, profile.jitter),
        Operation::Random(Purpose::RetryJitter, NonZeroU64::MIN),
        Operation::Wait(EpochTick { epoch: 1, tick: 1 }),
        Operation::Wait(EpochTick { epoch: 1, tick: 0 }),
    ];
    for entry in &durable.commits {
        operations.push(Operation::Commit(
            one,
            entry.key,
            entry.publication.clone(),
            CommitMode::Unversioned,
        ));
        let other = simulation::publication(&entry.publication.prestate, 2);
        operations.push(Operation::Commit(
            one,
            entry.key,
            other,
            CommitMode::Unversioned,
        ));
    }
    for (delivery, _) in &durable.outbox {
        operations.push(Operation::Send(one, delivery.clone()));
        operations.push(Operation::Acknowledge(
            one,
            delivery.id,
            delivery.payload_sha256,
        ));
        operations.push(Operation::Acknowledge(one, delivery.id, Hash32::ZERO));
    }
    operations
}

/// Applies one publication fault-free, for building probe and scenario stores.
#[must_use]
pub fn committed(durable: &Durable, key: Hash32, body: u8) -> Durable {
    let data = PublicationData {
        prestate: durable.state.clone(),
        command: vec![body],
        context: vec![1],
        identity: IDENTITY.to_vec(),
        subject: sha256(&[IDENTITY, &durable.state, &[body], &[1]])
            .as_bytes()
            .to_vec(),
    };
    expected_commit(durable, key, &data).unwrap_or_else(|| durable.clone())
}

/// Acknowledges every record fault-free.
#[must_use]
pub fn acknowledged(durable: &Durable) -> Durable {
    let mut durable = durable.clone();
    for record in &mut durable.outbox {
        record.1 = true;
    }
    durable
}

/// Independently derives the complete history fold, including every delivery field.
#[must_use]
pub fn history_fold(entries: &[HistoryEntry]) -> Option<Durable> {
    let mut folded = Durable {
        state: vec![0],
        chain: sha256(&[b"genesis", IDENTITY]),
        commits: Vec::new(),
        receipts: Vec::new(),
        replays: Vec::new(),
        outbox: Vec::new(),
    };
    for entry in entries {
        if entry.version != folded.commits.len() as u64 + 1 {
            return None;
        }
        folded = expected_commit(&folded, entry.key, &entry.publication)?;
    }
    Some(folded)
}
