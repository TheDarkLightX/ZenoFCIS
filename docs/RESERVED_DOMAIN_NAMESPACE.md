# Reserved commitment domains

V2 reserves exactly the name `zeno-fcis` and every name beginning with
`zeno-fcis/`. `Domain::new`, `DomainPrefix::try_new`, and
`StateDomainBinding::try_new` reject those names at their normal public
boundaries. Their `try_new_project` conveniences enforce the same reservation.
There is no normalization: `zeno-fcis-app/state`, `app/zeno-fcis/state`, and
`ZENO-FCIS/state` are different names and do not enter this namespace.

Library code selects a fixed `Domain<'static>` from
`zeno_fcis_codec::domains`. Each constant fixes both name and version; `ALL`
contains the complete sorted registry. No API accepts a string and returns an
arbitrary reserved domain, and no caller/provider declaration grants an
exception. Private `Domain` fields prevent constructing or changing a tag.
Selecting a published constant gives no verification or publication authority.

The registry contains 147 entries. Existing library pairs retain their exact
name and version. The domain preimage remains
`ZENOFCIS-HASH\0 || version:u16be || name_length:u16be || name ||
payload_length:u64be || payload`. Native golden tests independently spell out
each original name/version and construct this preimage. The cryptographic
known-answer vectors retain their original expected values.

The fixed V2 shell entries are `V2_STATE`, `V2_GENESIS`, `V2_PUBLICATION`,
`V2_CHAIN`, `V2_EFFECT_DELIVERY`, `V2_CHECKPOINT`, and `V2_CERTIFICATE`, each
version 1 under `zeno-fcis/v2/` with the corresponding lowercase hyphenated
suffix. `OUTBOX_ENTRY` and `DELIVERY` retain their original version-1 domains.
`DELIVERY_INTERPRETER` uses version 2 at `zeno-fcis/delivery-interpreter` to
bind the concrete memory delivery source and its sealed hash provider. The
SQLite genesis records this identity and refuses a different interpreter at
open or delivery binding. This source identity assumes the named compiler and
platform; it is not binary attestation or a proof of external delivery.
Private library hash helpers accept an explicit `Domain` rather than a string.
Project state/profile constructors cannot repurpose those fixed names.

This preserves domain preimage bytes, not every caller's payload format. The
separate evidence/law V2 migration intentionally changes certain payloads and
identities; old receipts for those subjects must be regenerated. The registry
and hash provider are native shell/evidence dependencies. Listing them in an
inventory does not establish a Verus theorem about their implementation.
