# PR 4 scope: independently checked SHA-256 providers

This stack layer adds only cryptographic-provider adapters. Canonical encoding, domain separation, preimage framing, profile schemas, transition semantics, and promotion policy remain owned by the earlier ZenoFCIS layers.

The ordinary provider is RustCrypto SHA-256. The independent assurance provider is libcrux SHA-256 generated from HACL*. Both are pinned and checked against published vectors, a fixed ZenoFCIS domain vector, and each other.

No provider is allowed to define protocol bytes, domain names, state roots, or business semantics.
