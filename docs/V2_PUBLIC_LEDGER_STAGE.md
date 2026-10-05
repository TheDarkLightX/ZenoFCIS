# V2 public ledger stage

This bounded stage implements mandatory formula-path resolution, honest
byte-bound evidence APIs, and closed reserved-domain construction. It also
repairs the law-evaluation self-hash ledger entry. It does not qualify the
combined V2 executor, shell, templates, workspace cutover, or release.

`LawEvaluation` and `GenesisLawEvaluation` now encode only their semantic
fields: format magic/version 2, law-set identity, input identity, decision tag
(for transitions), and ordered observations. Their cached `evaluation_hash`
is absent from canonical bytes. Construction computes the digest from exactly
those bytes under the existing version-1 law-evaluation domain. Independent
native tests construct both canonical byte streams and both domain preimages,
including all three transition decision tags. Changing the private cache does
not change the encoded subject.

This intentionally breaks V1 canonical bytes and identities. Evidence envelopes
also have a new versioned format without `source_commit`, and law-set format 2
binds the revised subjects. Law input formats using the same version constants
change as well. Dependent law-set, authorization, genesis/transition receipt,
refinement, composition, and retained-store evidence must be regenerated under
the exact integrated V2 source. Historical receipts are not silently promoted
or claimed compatible. Existing fixed domain names, versions, and domain
preimage framing remain identical.

The source changes are confined to the public-ledger ownership manifest.
Excluded synthesis/finite, shell-sqlite, umbrella exports, and template callers
receive separately hashed adapter patches. A patch is an integration proposal,
not evidence that its target was built or qualified. Root owns those ports and
the final combined gates. Fixed V2 shell domains are documented in
[RESERVED_DOMAIN_NAMESPACE.md](RESERVED_DOMAIN_NAMESPACE.md).

The registry, generic hash interfaces, native language front end, and external
attestation adapters remain explicit native trusted dependencies. No source
inventory extends the finite core's theorem. The source-bound result and
manifest at the stage handoff record actual native checks, retained failed
probes, custody controls, dependency hashes, and pending integration work.
