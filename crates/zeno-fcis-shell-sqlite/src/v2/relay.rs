//! The relay protocol: how a process outside the shell delivers a store's
//! pending entries to an external system and acknowledges them.
//!
//! The protocol has no dependency beyond the shell's own. It is two
//! operations on a store, each through the typed delivery lifecycle:
//!
//! - **Export.** [`V2SqliteShell::export_pending`] lists every pending
//!   delivery in commit order (commit, then lane, then ordinal), after
//!   replaying each owning commit under its segment's contract, and
//!   [`export_line`] writes one as a line of JSON: the delivery ID, channel,
//!   destination and payload, the payload's SHA-256 and the place in commit
//!   order. The stream is durable because the store is: an entry stays in
//!   every later export until it is acknowledged, with the same ID and bytes.
//! - **Acknowledge.** [`acknowledge`] takes a delivery ID and the SHA-256 of
//!   the payload the relay sent. It issues the entry's
//!   [`Pending`](crate::v2::Pending) token with
//!   [`pending_by_id`](crate::v2::V2SqliteShell::pending_by_id), turns it into
//!   a [`Delivered`](crate::v2::Delivered) token with
//!   [`Pending::relayed`](crate::v2::Pending::relayed), and acknowledges that.
//!   An unknown ID is refused with `Error::UnknownDelivery`, a payload hash
//!   other than the stored payload's with `Error::PayloadMismatch`, and an
//!   entry already acknowledged with `Error::AlreadyAcknowledged`. The last
//!   two checks are repeated in the acknowledging transaction, so a refusal
//!   changes no delivery and no commit. When another connection wrote the
//!   store first, the call audits it before anything else, and that audit
//!   records its checkpoint, as every audit does.
//!
//! # Strength
//!
//! Transport is at least once. A relay that stops between its outbound call
//! and the acknowledgment sends the entry again after a restart, and every
//! attempt carries the same delivery ID and payload. A receiver that honours
//! the delivery ID as an idempotency key sees each effect once; one that
//! does not may act twice. The shell cannot observe the external system: an
//! acknowledgment records that a relay presented an exported ID with the
//! stored payload's hash, not that a receiver acted on it. The binding to the
//! payload hash catches a relay that mixes up entries or stores; it is not
//! authentication, since anyone who can run the acknowledgment can present
//! an exported line.

use std::fmt::Write as _;

use rusqlite::TransactionBehavior;
use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;

use super::{
    Delivery, Error, V2SqliteShell, check_cached_tip, check_commit, load_commit, load_delivery,
    previous_state,
};

/// The `schema` field of every exported line.
pub const EXPORT_SCHEMA: &str = "zeno-fcis/relay-export/1";

impl V2SqliteShell<'_, '_> {
    /// Every pending delivery, in commit order, after replaying each owning
    /// commit under the contract of that commit's history segment. Nothing is
    /// written, and an entry stays in every later export until it is
    /// acknowledged.
    ///
    /// # Errors
    /// The refusals of [`next_pending`](Self::next_pending).
    pub fn export_pending(&mut self) -> Result<Vec<Delivery>, Error> {
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
        let positions = {
            let mut statement = tx.prepare(
                "SELECT sequence,lane,ordinal FROM v2_deliveries WHERE acknowledged=0 ORDER BY sequence,lane,ordinal",
            )?;
            statement
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut checked = None;
        let mut deliveries = Vec::with_capacity(positions.len());
        for (sequence, lane, ordinal) in positions {
            if checked != Some(sequence) {
                let row = load_commit(&tx, sequence)?;
                let state = previous_state(&tx, sequence)?;
                check_commit(&tx, self.members.authorities(), &segments, &row, &state)?;
                checked = Some(sequence);
            }
            deliveries.push(load_delivery(&tx, sequence, lane, ordinal)?);
        }
        tx.commit()?;
        Ok(deliveries)
    }
}

/// Acknowledges the pending delivery `id` for a relay that sent a payload
/// whose SHA-256 is `payload_sha256`, through the typed lifecycle.
///
/// # Errors
/// `Error::UnknownDelivery`, `Error::AlreadyAcknowledged` or
/// `Error::PayloadMismatch` as the [module](self) describes, and the
/// refusals of [`Delivered::acknowledge`](crate::v2::Delivered::acknowledge).
/// A refusal changes no delivery and no commit.
pub fn acknowledge(
    shell: &mut V2SqliteShell<'_, '_>,
    id: Hash32,
    payload_sha256: Hash32,
) -> Result<(), Error> {
    shell
        .pending_by_id(id)?
        .relayed(payload_sha256)?
        .acknowledge()
}

/// The SHA-256 of exact payload bytes, the hash an acknowledgment carries.
pub fn payload_sha256(payload: &[u8]) -> Hash32 {
    RustCryptoSha256::hash(payload)
}

/// One exported delivery as a line of JSON with no line break: `schema`,
/// then `commit`, `lane` and `ordinal` (its place in commit order),
/// `delivery_id`, `channel`, `destination_root`, `payload_root`,
/// `destination` and `payload` (canonical bytes in lowercase hexadecimal),
/// `payload_sha256` and `entry_hash`. The same delivery always gives the
/// same line.
pub fn export_line(delivery: &Delivery) -> String {
    format!(
        "{{\"schema\":\"{EXPORT_SCHEMA}\",\"commit\":{},\"lane\":{},\"ordinal\":{},\"delivery_id\":\"{}\",\"channel\":{},\"destination_root\":{},\"payload_root\":{},\"destination\":\"{}\",\"payload\":\"{}\",\"payload_sha256\":\"{}\",\"entry_hash\":\"{}\"}}",
        delivery.version(),
        delivery.lane(),
        delivery.ordinal(),
        hex(delivery.delivery_id().as_bytes()),
        delivery.channel(),
        delivery.destination_root(),
        delivery.payload_root(),
        hex(delivery.destination()),
        hex(delivery.payload()),
        hex(payload_sha256(delivery.payload()).as_bytes()),
        hex(delivery.entry_hash().as_bytes()),
    )
}

/// Lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// A 32-byte hash from exactly 64 lowercase hexadecimal digits, as
/// [`export_line`] writes them; `None` for anything else.
pub fn parse_hex_hash(text: &str) -> Option<Hash32> {
    let digits = text.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks_exact(2)) {
        *byte = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(Hash32::new(bytes))
}

fn digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexadecimal_round_trips_and_refuses_other_text() {
        let hash = Hash32::new(core::array::from_fn(|i| (i * 37) as u8));
        assert_eq!(parse_hex_hash(&hex(hash.as_bytes())), Some(hash));
        let text = hex(hash.as_bytes());
        assert_eq!(parse_hex_hash(&text.to_uppercase()), None);
        assert_eq!(parse_hex_hash(&text[1..]), None);
        assert_eq!(parse_hex_hash(&format!("{text}0")), None);
        assert_eq!(parse_hex_hash(&format!("g{}", &text[1..])), None);
        assert_eq!(parse_hex_hash(""), None);
    }

    #[test]
    fn payload_hash_is_plain_sha256() {
        assert_eq!(
            hex(payload_sha256(b"abc").as_bytes()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
