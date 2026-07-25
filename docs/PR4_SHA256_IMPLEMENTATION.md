# Independently checked SHA-256 implementation

The provider crate is deliberately narrower than the semantic kernel. It implements `CommitmentHasher` with two external implementations, publishes stable algorithm identities, and exposes fixed-vector and parity reports. It does not construct protocol preimages.

The final PR must contain a pinned lockfile, a read-only provider workflow, no write-enabled assembly workflow, and no trigger or transport artifacts.
