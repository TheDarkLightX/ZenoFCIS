# Scalar directory and mailbox composition

This LLM-generated example checks two local, channel-free cores. One directory
has two device entries; one mailbox has two message slots. Reads must use the
selected device's status and key generation. `composition.json` fixes bindings
and order. `tools/check_service_product.py --repin --binary PRODUCT_BINARY`
regenerates source and checked-program pins through the compiler.

The complete admissible product contains 3,414 states and 16 requests (54,624
rows), below the 65,536-row cap. Admissibility means both local invariants and
all global couplings hold. Unreachable admissible states are included. The
independent Python interpreter compares outcome, reason, and every successor
field, and separately checks accepted reads. Over-budget work is refused.

This check grants no store or publication authority. The directory has no
channel; a delivering directory is a different subject. There are no signature,
storage, finality, caller privacy, live credential, or collection-scale claims.
The directory's anchor operation is a modeled command, not verified finality.

## Lineage and scope

Background: C. A. R. Hoare, *Communicating Sequential Processes*, CACM, 1978,
and Lamport, *Specifying Systems*, Addison-Wesley, 2002, describe composition
and explicit state-machine reasoning. These references do not certify this
implementation. Prior-art search pending. Evidence is exhaustive only over the
declared finite domains and the specified admissibility predicates.

## Local service authority

`runtime/` adds explicit read-only validation commands so every core's actual
checked Program participates on every request. `service.json` pins the nested
composition, the schemas, request/decision laws and the service budget. Run
`tools/make_service_manifest.py --binary PRODUCT_BINARY runtime_directory` to
regenerate these pins. A schema, policy, program, binding, order or checker
change changes the service identity and requires a new store version.

`ServiceAuthority` returns a private, immutable `ServicePublication` only when
all actual per-core publications and global laws pass. Rejection discards all
provisional successors and retains actual core and wrapper accounting. Its
preparation tables supply bindings, not decision or publication authority.
This scalar profile refuses channels, CommittedFailure and skipped cores.
