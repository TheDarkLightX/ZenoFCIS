# Original schema binding

This stage checks a complete proposed schema description against the original
canonical schema bytes. The proposal is untrusted. A private successful result
must establish the original bytes, every definition, name, bound, field and
variant, the canonical order, full consumption and closed acyclic references.
Input roots and output/channel schemas must subsequently be obtained from this
checked description. A supplied scalar descriptor or legacy schema getter is
not sufficient evidence of that relation.

Schema admission supports Unit, Bool, full-width signed and unsigned intervals,
bounded Bytes/Text, Enum, Tuple, Record, payload-free or payload-bearing Sum,
bounded Vector and bounded Map using their original canonical wire tags.
Names and all unused definitions remain part of the original canonical
identity. Type/field/variant zero, empty records, empty variant sets, arbitrarily
wide integer intervals and original account-lockout roots are retained when
legal in this grammar. Every compound reference, including unused definitions,
must resolve and participate in the acyclicity check. Record fields may refer
to compound types or nested records. The templates' original domains remain
complete. Admitting this schema grammar alone does not establish compound
execution, byte-preserving candidate carry or typed root delivery support;
those actual execution bridges require their own checked qualification.

The native checker resolves each reference once into an index graph, then runs
at most one synchronous resolution pass per definition. Its work is bounded by
O(N*(N+E)) including linear index lookup, with O(N+E) scratch storage. It does
not recursively enumerate dependency paths. The checked mathematical bridge
relates successful resolution to finite dependency height and excludes every
directed cyclic path. Native tests exercise a shared 64-type graph with more
than 2^63 paths, a complete-depth chain, unknown and cyclic references of every
compound kind, and the exact original 501-byte 12-type fixture schema, including
its 11 state fields and payload-bearing Event definition.

The checker uses the production integer readers and byte comparisons. Its
mathematical specification fixes the schema wire grammar and exact canonical
ordering rather than accepting an application assertion of correspondence.
Every executable function must have a total exact contract, zero executable
preconditions, complete translated contract/body coverage, and no application
assumption or opaque external body. Native oracles use the existing schema
encoder independently; mutations challenge omitted definitions, bounds,
references, names, full consumption and legal generic identifiers.

Schema binding is construction-time policy work. It does not authorize an
invocation or grant protected access, and it does not assert physical resource
costs. Original-envelope/root/commitment binding, protected payload reads,
decision/law composition, source identity and replay remain separate checked
bridges. Hash commitments retain their stated cryptographic assumption. The
reviewed closed contract supplies intent; an unchecked .zeno lowering or LLM
proposal cannot silently acquire semantic correctness from a wire match.
