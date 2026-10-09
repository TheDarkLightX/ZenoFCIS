//! The delivery lifecycle as consuming tokens: `Pending` → `Delivered` →
//! acknowledged.
//!
//! [`V2SqliteShell::next_pending`] and [`V2SqliteShell::pending_by_id`] are
//! the only places a `Pending` token is made, and [`Pending::deliver`] and
//! [`Pending::relayed`] the only places a `Delivered` one is. The fields of
//! both tokens are private to this module, so these transitions are the only
//! way from one state to the next. `relayed` makes a `Delivered` token from
//! a relay's report of the payload hash, with nothing delivered in this
//! process: an acknowledgment through it rests on that report, not on the
//! types. [`Pending`] states what the types enforce and what stays a
//! run-time check.

use std::fmt;

use rusqlite::{OptionalExtension, TransactionBehavior};
use zeno_fcis_codec::{DecodeLimits, Hash32, decode_value};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell::{IdempotentDestination, MemoryDestination};

use super::{
    BoundMemoryInterpreter, Delivery, Error, V2SqliteShell, check_cached_tip, check_commit,
    load_commit, load_delivery, parse_hash, previous_state,
};

impl<'a, 'p> V2SqliteShell<'a, 'p> {
    /// Issues the oldest pending delivery, in commit order, as a [`Pending`]
    /// token, after replaying its owning commit under the contract of that
    /// commit's history segment. `None` when every delivery is acknowledged.
    ///
    /// The token holds this handle's exclusive borrow until it is consumed
    /// or dropped. An entry whose token was dropped stays pending, and the
    /// next call issues it again.
    ///
    /// # Errors
    /// A store changed outside this handle refuses as its audit does, and a
    /// store that no longer runs this handle's current contract as
    /// `Error::Identity`.
    pub fn next_pending(&mut self) -> Result<Option<Pending<'_, 'a, 'p>>, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let position: Option<(i64,i64,i64)> = tx.query_row("SELECT sequence,lane,ordinal FROM v2_deliveries WHERE acknowledged=0 ORDER BY sequence,lane,ordinal LIMIT 1", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let delivery = match position {
            None => None,
            Some((sequence, lane, ordinal)) => {
                let row = load_commit(&tx, sequence)?;
                let state = previous_state(&tx, sequence)?;
                check_commit(&tx, self.members.authorities(), &segments, &row, &state)?;
                Some(load_delivery(&tx, sequence, lane, ordinal)?)
            }
        };
        tx.commit()?;
        Ok(delivery.map(|delivery| Pending {
            shell: self,
            delivery,
        }))
    }

    /// Issues the pending delivery with this ID as a [`Pending`] token, after
    /// replaying its owning commit under the contract of that commit's
    /// history segment. This is how an acknowledgment from an external relay
    /// enters the lifecycle; see [`relay`](crate::v2::relay).
    ///
    /// # Errors
    /// `Error::UnknownDelivery` when the store holds no delivery with this
    /// ID, `Error::AlreadyAcknowledged` when it holds it as acknowledged, and
    /// the refusals of [`next_pending`](Self::next_pending). Nothing is
    /// written.
    pub fn pending_by_id(&mut self, id: Hash32) -> Result<Pending<'_, 'a, 'p>, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let (sequence, lane, ordinal, acknowledged): (i64, i64, i64, i64) = tx
            .query_row(
                "SELECT sequence,lane,ordinal,acknowledged FROM v2_deliveries WHERE delivery_id=?1",
                [id.as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or(Error::UnknownDelivery)?;
        if acknowledged != 0 {
            return Err(Error::AlreadyAcknowledged);
        }
        let row = load_commit(&tx, sequence)?;
        let state = previous_state(&tx, sequence)?;
        check_commit(&tx, self.members.authorities(), &segments, &row, &state)?;
        let delivery = load_delivery(&tx, sequence, lane, ordinal)?;
        tx.commit()?;
        Ok(Pending {
            shell: self,
            delivery,
        })
    }
}

/// A pending delivery of a store, as a one-use token that only
/// [`V2SqliteShell::next_pending`] (the oldest pending entry) and
/// [`V2SqliteShell::pending_by_id`] (the entry with a given ID) issue.
///
/// The lifecycle is `Pending` → [`Delivered`] → acknowledged:
/// [`deliver`](Pending::deliver) or [`relayed`](Pending::relayed) consumes
/// this token and returns a `Delivered` one, and
/// [`acknowledge`](Delivered::acknowledge) consumes that and marks the entry
/// acknowledged in the store.
///
/// # What the types enforce
///
/// - Only the store issues tokens. Their fields are private, and they have
///   no public constructor, `Clone`, `Default`, or conversion from a
///   [`Delivery`] record. A `Delivered` token exists only after `deliver`
///   or `relayed`.
/// - Only `Delivered` has `acknowledge`, so acknowledging a `Pending` token
///   directly does not compile. The types do not establish that the entry
///   reached anything: `relayed` turns a `Pending` token into a `Delivered`
///   one given only the SHA-256 of the stored payload, which any caller
///   holding the token can compute, so an acknowledgment through `relayed`
///   rests on the relay's report of its outbound call, not on the types.
/// - Each transition takes its token by value, and neither token is `Clone`,
///   so a consumed token cannot be used again and one delivery cannot be
///   acknowledged twice through its token.
/// - A token holds the exclusive borrow of the handle that issued it until
///   it is consumed or dropped, and both transitions act through that handle.
///   No operation takes a token and a store, so a token cannot be presented
///   to another store or another lineage's handle. It cannot outlive its
///   handle, and the handle issues no second token while one is live.
///
/// # What is checked at run time
///
/// - `next_pending` audits a store another connection changed, requires the
///   store to run this handle's current contract, and replays the owning
///   commit under its segment's contract before issuing the token.
/// - `deliver` binds the library memory interpreter by its identity and
///   decodes the stored destination and payload. The destination refuses a
///   delivery ID it already holds with other content.
/// - `acknowledge` checks the store again: a write by another connection
///   forces a full audit first, and then, in one immediate transaction, the
///   owning commit is replayed and the entry hash the destination reported
///   must equal the stored one. Each refusal is a typed [`Error`] and leaves
///   the entry pending.
///
/// # What neither covers
///
/// The types reach one handle. Two handles, or two processes, on one file
/// issue their own tokens: both can deliver an entry and acknowledge it, and
/// the second acknowledgment finds it acknowledged and leaves it so; for a
/// token from [`Pending::relayed`] it is refused as
/// `Error::AlreadyAcknowledged` instead. A
/// dropped token or a failed transition leaves the entry pending, and the
/// store issues it again, so delivery is at least once. A destination that
/// honours delivery IDs, as the memory destination does, records one effect.
/// A `Delivered` token shows that the library's memory destination accepted
/// the entry in this process, not that an external system acted on it.
///
/// # Misuse that does not compile
///
/// Each failing example sits beside one that compiles, written the same way
/// apart from the misuse. Delivering, then acknowledging:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_one(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         pending.deliver(destination)?.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// Acknowledging without delivering: `Pending` has no `acknowledge` (E0599).
///
/// ```compile_fail,E0599
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_one(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         pending.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// Delivering once:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_once(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         let delivered = pending.deliver(destination)?;
///         delivered.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// Reusing the token `deliver` consumed: a use of a moved value (E0382).
///
/// ```compile_fail,E0382
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_once(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         let delivered = pending.deliver(destination)?;
///         let again = pending.deliver(destination)?;
///         delivered.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// A token from the store:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Delivery, Error, Pending, V2SqliteShell}};
/// fn issue(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
///     record: Delivery,
/// ) -> Result<(), Error> {
///     let pending = shell.next_pending()?.ok_or(Error::Delivery)?;
///     pending.deliver(destination)?.acknowledge()
/// }
/// ```
///
/// A token made outside the crate from a genuine record: its fields are
/// private (E0451).
///
/// ```compile_fail,E0451
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Delivery, Error, Pending, V2SqliteShell}};
/// fn issue(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
///     record: Delivery,
/// ) -> Result<(), Error> {
///     let pending = Pending { shell, delivery: record };
///     pending.deliver(destination)?.acknowledge()
/// }
/// ```
///
/// One token after another from a handle:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_two(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     let first = shell.next_pending()?.ok_or(Error::Delivery)?;
///     first.deliver(destination)?.acknowledge()?;
///     let second = shell.next_pending()?.ok_or(Error::Delivery)?;
///     second.deliver(destination)?.acknowledge()
/// }
/// ```
///
/// A second token while the first is live: the handle is already borrowed
/// (E0499).
///
/// ```compile_fail,E0499
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn deliver_two(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     let first = shell.next_pending()?.ok_or(Error::Delivery)?;
///     let second = shell.next_pending()?.ok_or(Error::Delivery)?;
///     first.deliver(destination)?.acknowledge()?;
///     second.deliver(destination)?.acknowledge()
/// }
/// ```
#[must_use = "the entry stays pending until it is delivered and acknowledged"]
pub struct Pending<'s, 'a, 'p> {
    shell: &'s mut V2SqliteShell<'a, 'p>,
    delivery: Delivery,
}

impl<'s, 'a, 'p> Pending<'s, 'a, 'p> {
    /// The stored delivery this token is for.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }

    /// Delivers the entry through the library memory interpreter, consuming
    /// this token. The store is not written: the entry stays pending until
    /// the returned token is acknowledged.
    ///
    /// # Errors
    /// `Error::Interpreter` when the store's interpreter identity is not the
    /// library's, and `Error::Delivery` when the stored destination or
    /// payload does not decode or the destination already holds this
    /// delivery ID with other content. The entry then stays pending.
    pub fn deliver(
        self,
        destination: &mut MemoryDestination,
    ) -> Result<Delivered<'s, 'a, 'p>, Error> {
        let Pending { shell, delivery } = self;
        let interpreter = BoundMemoryInterpreter::bind(destination, shell.delivery_interpreter)?;
        let entry = OutboxEntry::new(
            delivery.ordinal,
            delivery.channel,
            decode_value(&delivery.destination, DecodeLimits::default())
                .map_err(|_| Error::Delivery)?,
            decode_value(&delivery.payload, DecodeLimits::default())
                .map_err(|_| Error::Delivery)?,
        );
        let observed = interpreter
            .destination
            .deliver(delivery.delivery_id, delivery.entry_hash, &entry)
            .map_err(|_| Error::Delivery)?;
        Ok(Delivered {
            shell,
            delivery,
            observed,
            relayed: None,
        })
    }

    /// Records that an external relay reports delivering this entry with a
    /// payload whose SHA-256 is `payload_sha256`, consuming this token. The
    /// store is not written: the entry stays pending until the returned
    /// token is acknowledged, and [`Delivered::acknowledge`] checks the hash
    /// again, with the entry still unacknowledged, in its transaction.
    ///
    /// The shell cannot observe the external system: the returned token
    /// shows only that a relay presented this ID and the stored payload's
    /// hash, not that a receiver acted on it.
    ///
    /// # Errors
    /// `Error::PayloadMismatch` when `payload_sha256` is not the SHA-256 of
    /// the stored payload. The entry then stays pending.
    pub fn relayed(self, payload_sha256: Hash32) -> Result<Delivered<'s, 'a, 'p>, Error> {
        let Pending { shell, delivery } = self;
        if super::relay::payload_sha256(&delivery.payload) != payload_sha256 {
            return Err(Error::PayloadMismatch);
        }
        let observed = delivery.entry_hash;
        Ok(Delivered {
            shell,
            delivery,
            observed,
            relayed: Some(payload_sha256),
        })
    }
}

impl fmt::Debug for Pending<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pending")
            .field("delivery", &self.delivery)
            .finish_non_exhaustive()
    }
}

/// A delivery the memory destination accepted, or an external relay reports
/// sending, that the store has not yet acknowledged, as a one-use token that
/// only [`Pending::deliver`] and [`Pending::relayed`] make.
/// What the types enforce and what stays a run-time check is stated on
/// [`Pending`].
///
/// # Misuse that does not compile
///
/// Acknowledging once:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn acknowledge_once(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         let delivered = pending.deliver(destination)?;
///         delivered.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// Acknowledging twice: `acknowledge` consumed the token (E0382).
///
/// ```compile_fail,E0382
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// fn acknowledge_once(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     if let Some(pending) = shell.next_pending()? {
///         let delivered = pending.deliver(destination)?;
///         delivered.acknowledge()?;
///         delivered.acknowledge()?;
///     }
///     Ok(())
/// }
/// ```
///
/// A delivered token from delivering:
///
/// ```
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Delivered, Delivery, Error, V2SqliteShell}};
/// fn acknowledge(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
///     record: Delivery,
/// ) -> Result<(), Error> {
///     let pending = shell.next_pending()?.ok_or(Error::Delivery)?;
///     let delivered: Delivered<'_, '_, '_> = pending.deliver(destination)?;
///     delivered.acknowledge()
/// }
/// ```
///
/// A delivered token made outside the crate, to acknowledge without
/// delivering: its fields are private (E0451).
///
/// ```compile_fail,E0451
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Delivered, Delivery, Error, V2SqliteShell}};
/// fn acknowledge(
///     shell: &mut V2SqliteShell<'_, '_>,
///     destination: &mut MemoryDestination,
///     record: Delivery,
/// ) -> Result<(), Error> {
///     let observed = record.entry_hash();
///     let delivered = Delivered { shell, delivery: record, observed, relayed: None };
///     delivered.acknowledge()
/// }
/// ```
///
/// After a restart, delivering again under the reopened handle:
///
/// ```
/// # use std::path::Path;
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// # use zeno_fcis_synthesis::finite::v2_authority::Authority;
/// fn restart(
///     path: &Path,
///     authority: &Authority<'_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     let mut shell = V2SqliteShell::open(path, authority)?;
///     let pending = shell.next_pending()?.ok_or(Error::Delivery)?;
///     let delivered = pending.deliver(destination)?;
///     drop(delivered);
///     drop(shell);
///     let mut reopened = V2SqliteShell::open(path, authority)?;
///     let pending = reopened.next_pending()?.ok_or(Error::Delivery)?;
///     pending.deliver(destination)?.acknowledge()
/// }
/// ```
///
/// Keeping the token past its handle, to acknowledge it after the restart:
/// the handle cannot move while the token borrows it (E0505).
///
/// ```compile_fail,E0505
/// # use std::path::Path;
/// # use zeno_fcis_shell_sqlite::{MemoryDestination, v2::{Error, V2SqliteShell}};
/// # use zeno_fcis_synthesis::finite::v2_authority::Authority;
/// fn restart(
///     path: &Path,
///     authority: &Authority<'_>,
///     destination: &mut MemoryDestination,
/// ) -> Result<(), Error> {
///     let mut shell = V2SqliteShell::open(path, authority)?;
///     let pending = shell.next_pending()?.ok_or(Error::Delivery)?;
///     let delivered = pending.deliver(destination)?;
///     drop(shell);
///     let mut reopened = V2SqliteShell::open(path, authority)?;
///     delivered.acknowledge()
/// }
/// ```
#[must_use = "a delivered entry stays pending until it is acknowledged"]
pub struct Delivered<'s, 'a, 'p> {
    shell: &'s mut V2SqliteShell<'a, 'p>,
    delivery: Delivery,
    observed: Hash32,
    /// The payload hash an external relay reported, for a token made by
    /// [`Pending::relayed`].
    relayed: Option<Hash32>,
}

impl Delivered<'_, '_, '_> {
    /// The stored delivery this token is for.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }

    /// Marks the delivery acknowledged in the store, consuming this token,
    /// after checking the store again; the check and the mark share one
    /// immediate transaction.
    ///
    /// # Errors
    /// A store changed outside this handle refuses as its audit does,
    /// `Error::Concurrent` when another connection writes during the check,
    /// and `Error::Delivery` when the delivery ID is no longer stored or the
    /// entry hash the destination reported differs from the stored one. A
    /// token from [`Pending::relayed`] is also refused with
    /// `Error::AlreadyAcknowledged` when the entry was acknowledged since it
    /// was issued, and with `Error::PayloadMismatch` when the reported hash
    /// is not the stored payload's. A refusal leaves the entry pending and
    /// writes nothing.
    pub fn acknowledge(self) -> Result<(), Error> {
        let Delivered {
            shell,
            delivery,
            observed,
            relayed,
        } = self;
        let id = delivery.delivery_id;
        shell.synchronize()?;
        let tx = shell
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &shell.anchor,
            shell.data_version,
            shell.members.authorities(),
        )?;
        let sequence: i64 = tx
            .query_row(
                "SELECT sequence FROM v2_deliveries WHERE delivery_id=?1",
                [id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::Delivery)?;
        let row = load_commit(&tx, sequence)?;
        let state = previous_state(&tx, sequence)?;
        check_commit(&tx, shell.members.authorities(), &segments, &row, &state)?;
        let expected: Vec<u8> = tx.query_row(
            "SELECT entry_hash FROM v2_deliveries WHERE delivery_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if parse_hash(&expected)? != observed {
            return Err(Error::Delivery);
        }
        if let Some(reported) = relayed {
            let (acknowledged, payload): (i64, Vec<u8>) = tx.query_row(
                "SELECT acknowledged,payload FROM v2_deliveries WHERE delivery_id=?1",
                [id.as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if acknowledged != 0 {
                return Err(Error::AlreadyAcknowledged);
            }
            if super::relay::payload_sha256(&payload) != reported {
                return Err(Error::PayloadMismatch);
            }
        }
        tx.execute(
            "UPDATE v2_deliveries SET acknowledged=1 WHERE delivery_id=?1",
            [id.as_bytes().as_slice()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

impl fmt::Debug for Delivered<'_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Delivered")
            .field("delivery", &self.delivery)
            .field("observed", &self.observed)
            .field("relayed", &self.relayed)
            .finish_non_exhaustive()
    }
}

/// The durable-counter template's generated contract, for the tests below.
#[cfg(test)]
#[allow(dead_code, missing_docs, unreachable_pub)]
#[path = "../../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub(super) mod durable;

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_codec::{CanonicalEncode, Envelope};
    use zeno_fcis_synthesis::finite::{
        v2_authority::PublicationOutcome,
        v2_composition::{FrameBinding, Raw},
    };
    use zeno_fcis_value::{Field, Value};

    fn ok<T, E: fmt::Debug>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
    }

    fn envelope(binding: FrameBinding, value: Value) -> Vec<u8> {
        ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
    }

    // With the memory destination a delivered token always carries the stored
    // entry hash, so only a token made here can carry another one. The check
    // stays for any later destination that reports the hash it observed.
    #[test]
    fn acknowledging_refuses_an_observed_hash_other_than_the_stored_one() {
        let contract = durable::Contract::new();
        let descriptor = contract.descriptor();
        let authority = ok(durable::checked_authority(&descriptor));
        let counter = |count: i128| {
            let value = ok(Value::record_canonical(vec![
                Field::new(110, Value::signed(count)),
                Field::new(111, Value::signed(0)),
            ]));
            envelope(durable::FRAMING.state, value)
        };
        let initial = counter(0);
        let PublicationOutcome::Commit(genesis) = authority.publish_genesis(&initial) else {
            panic!("genesis refused");
        };
        let mut shell = ok(V2SqliteShell::create_in_memory(&authority, genesis));
        let command = envelope(durable::FRAMING.command, Value::sum(101, 120, None));
        let context = envelope(durable::FRAMING.context, Value::boolean(true));
        let PublicationOutcome::Commit(publication) = authority.publish(Raw {
            state: &initial,
            command: &command,
            context: &context,
        }) else {
            panic!("publication refused");
        };
        ok(shell.commit(Hash32::new([1; 32]), publication));
        let mut destination = MemoryDestination::default();
        let pending = ok(shell.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        let mut delivered = ok(pending.deliver(&mut destination));
        assert_eq!(delivered.observed, delivered.delivery.entry_hash);
        delivered.observed = Hash32::ZERO;
        assert!(matches!(delivered.acknowledge(), Err(Error::Delivery)));
        // Nothing was written: the store issues the entry again.
        assert_eq!(ok(shell.snapshot()).pending(), 1);
        let pending = ok(shell.next_pending()).unwrap_or_else(|| panic!("still pending"));
        ok(ok(pending.deliver(&mut destination)).acknowledge());
        assert_eq!(ok(shell.snapshot()).pending(), 0);
        assert_eq!(destination.delivered_count(), 1);
    }
}
