# Checked value and consuming reservation workflow

This original example illustrates Rust construction and sequencing guarantees. It is not a formal verification of a reservation service. The decision is pure; `ReserveIntent` describes an intended operation against a supplied stock snapshot. The shell must atomically check that snapshot and apply the operation, with its own authorization and idempotency checks.

Compile this block as a Rust library to test consumers across a crate boundary:

```rust
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity(u16);

#[derive(Debug, PartialEq, Eq)]
pub struct ZeroQuantity;

impl TryFrom<u16> for Quantity {
    type Error = ZeroQuantity;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 0 { Err(ZeroQuantity) } else { Ok(Self(value)) }
    }
}

impl Quantity {
    pub fn get(self) -> u16 { self.0 }
}

#[derive(Debug)]
pub struct Draft { quantity: Quantity }

#[derive(Debug)]
pub struct Ready { quantity: Quantity, expected_available: u16 }

#[derive(Debug)]
pub enum Decision { Ready(Ready), Insufficient(Draft) }

#[derive(Debug)]
pub struct ReserveIntent { quantity: Quantity, expected_available: u16 }

impl Draft {
    pub fn new(quantity: Quantity) -> Self { Self { quantity } }

    pub fn decide(self, available: u16) -> Decision {
        if self.quantity.get() <= available {
            Decision::Ready(Ready {
                quantity: self.quantity,
                expected_available: available,
            })
        } else {
            Decision::Insufficient(self)
        }
    }
}

impl Ready {
    pub fn into_intent(self) -> ReserveIntent {
        ReserveIntent {
            quantity: self.quantity,
            expected_available: self.expected_available,
        }
    }
}

impl ReserveIntent {
    pub fn quantity(&self) -> Quantity { self.quantity }
    pub fn expected_available(&self) -> u16 { self.expected_available }
}
```

`Quantity::try_from(0)` refuses; positive quantities including `u16::MAX` are admitted. When requested quantity equals availability, the outcome is `Ready`; when it exceeds availability, the outcome preserves `Draft` for caller-directed recovery.

A consumer cannot construct `Quantity(0)` (private field, E0423), call `Draft::into_intent` (no such method, E0599), or use the same `Ready` after `into_intent` consumes it (moved value, E0382). `Ready` has no `Clone` implementation. Test each negative in its own external consumer, alongside one valid complete workflow. A failure from a missing import or broken library build is not evidence for the intended boundary.

These restrictions do not prevent a caller from submitting another draft, and the intent does not attest to current stock. Keep runtime commit checks. If a versioned snapshot is required to prevent ABA or identify history, add the version explicitly; an equal stock count alone cannot establish that history.
