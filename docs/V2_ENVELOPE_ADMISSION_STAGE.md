# Original envelope framing for V2

This stage starts from `1ed6f88` and preserves the existing envelope wire
format: eight magic bytes `ZFCISV1\0`, a big-endian u32 root type, the exact
32-byte schema commitment, a big-endian u32 payload length, and the payload.
Admission uses the original immutable byte slice. It does not reconstruct a
Value and re-encode it as a substitute for the received input.

The mathematical result is total over every slice, expected root, expected
schema commitment and maximum input size. Refusal precedence is size, short
frame, magic, root, schema, then payload length. A successful result retains
the complete original slice and borrows exactly its suffix after byte 48;
there are no trailing bytes, aliases or invented payloads. Generic root zero
and every schema-commitment byte are preserved. An empty framed payload is
valid framing; typed value admission must subsequently reject it.

The framing primitive neither validates a catalog nor interprets the payload.
The expected root/commitment must come from the bound V2 catalog in the later
authority route. Hash security remains an explicit cryptographic assumption.
It is unmetered like the existing integer readers: an integrating caller must
charge the envelope header before interpreting it, then use the same private
meter for protected payload reads and complete decision construction. A frame
result alone is neither a schema-admitted value nor authorization.

Required evidence is exact contracts on every executable helper, whole-source
pinned Verus and translated body/contract inventory, independent standard-
library framing oracles, truncation/length/identity negatives, proof mutations,
frame-contract controls that the verified framed caller must reject, and
separately verifying specification/inventory coverage controls. Full
native/Miri/no-std/Clippy, template regeneration, acceptance and independent
review follow integration.
Catalog extraction, typed payload admission, encoding/hashing, complete
decisions, laws/genesis and mandatory authority/replay remain open until their
actual bridges are implemented and qualified.
