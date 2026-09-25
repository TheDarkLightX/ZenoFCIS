//! The delivery destinations are the pure shell crate's types, re-exported.

use super::*;

/// Compiles only if this crate's `MemoryDestination` is the shell crate's.
fn delivered(destination: &zeno_fcis_shell::MemoryDestination) -> usize {
    destination.delivered_count()
}

/// Compiles only if this crate's `IdempotentDestination` is the shell crate's
/// contract.
fn deliver_under_the_shell_contract<D: zeno_fcis_shell::IdempotentDestination>(
    destination: &mut D,
    delivery_id: Hash32,
    entry_hash: Hash32,
    entry: &OutboxEntry,
) -> Result<Hash32, D::Error> {
    destination.deliver(delivery_id, entry_hash, entry)
}

#[test]
fn the_re_exported_destinations_are_the_shell_crate_types() {
    let entry = OutboxEntry::new(0, 300, Value::U128(1), Value::U128(2));
    let (id, hash, other) = (
        Hash32::new([1; 32]),
        Hash32::new([2; 32]),
        Hash32::new([3; 32]),
    );
    // Built through this crate's path, used through the shell crate's.
    let mut destination = MemoryDestination::default();
    assert_eq!(
        deliver_under_the_shell_contract(&mut destination, id, hash, &entry),
        Ok(hash)
    );
    assert_eq!(delivered(&destination), 1);
    // The same identity with the same content is idempotent; other content
    // collides, and the collision is the shell crate's type.
    assert_eq!(destination.deliver(id, hash, &entry), Ok(hash));
    let collision: zeno_fcis_shell::DeliveryCollision = match destination.deliver(id, other, &entry)
    {
        Err(collision) => collision,
        Ok(hash) => panic!("a collision was accepted as {hash}"),
    };
    assert_eq!(collision, DeliveryCollision);
    assert_eq!(delivered(&destination), 1);
    assert_eq!(
        collision.to_string(),
        "delivery identity already binds different entry content"
    );
    // The shell crate's `std` feature keeps the error trait for this crate's users.
    let _error: &dyn std::error::Error = &collision;
}
