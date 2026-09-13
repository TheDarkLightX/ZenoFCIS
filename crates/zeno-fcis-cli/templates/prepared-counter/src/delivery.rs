//! Imperative demonstration destination, retained across database reopen in this process.
//! A real remote destination must persist its idempotency ledger independently.

use std::{cell::RefCell, rc::Rc};
use zeno_fcis_codec::Hash32;
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell_sqlite::{DeliveryCollision, IdempotentDestination, MemoryDestination};

#[derive(Clone, Default)]
pub struct Destination(Rc<RefCell<MemoryDestination>>);

impl Destination {
    pub fn delivered_count(&self) -> usize {
        self.0.borrow().delivered_count()
    }
}

impl IdempotentDestination for Destination {
    type Error = DeliveryCollision;
    fn deliver(
        &mut self,
        id: Hash32,
        hash: Hash32,
        entry: &OutboxEntry,
    ) -> Result<Hash32, Self::Error> {
        self.0.borrow_mut().deliver(id, hash, entry)
    }
}
