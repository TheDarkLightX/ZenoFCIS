# SHA-256 provider policy

ZenoFCIS does not implement a cryptographic hash primitive. The semantic kernel owns canonical bytes, domain separation, length framing, and exact preimage construction. `zeno-fcis-crypto` supplies narrow adapters to vetted SHA-256 implementations.

## Providers

- `RustCryptoSha256` is the ordinary Rust provider and is pinned to `sha2` 0.11.0.
- `LibcruxSha256` is the independent assurance provider and is pinned to `libcrux-sha2` 0.0.8, whose SHA-2 implementation is generated from HACL*.
- The `parity` feature builds both and rejects any disagreement.

Provider algorithm identifiers are evidence metadata. They are not protocol domain tags, and changing a provider does not change ZCVE/1 bytes or the semantic kernel's domain-separated preimage.

## Required gates

Every release candidate using SHA-256 must pass:

1. published SHA-256 known-answer vectors;
2. a fixed ZenoFCIS domain-preimage vector;
3. ordinary/independent provider parity over boundary and project vectors;
4. canonical codec and root vectors from each promoted profile;
5. the mounted runtime-refinement gate.

## Nonclaims

Provider parity does not prove that a profile schema, domain name, canonical encoding, or business transition is correct. It establishes that independent SHA-256 implementations agree on the exact bytes supplied by the semantic kernel.
