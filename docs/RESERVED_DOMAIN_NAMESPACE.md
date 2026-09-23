# Reserved commitment-domain namespace

## Purpose

Every ZenoFCIS commitment hashes a domain name and version together with its
payload. Library identities use names in the `zeno-fcis/` namespace:
`zeno-fcis/candidate` for candidate IDs, `zeno-fcis/value` for patch
preconditions, and so on. The V1 constructors for project-supplied names
accept any bounded ASCII name. A project could therefore bind its state root
to a library domain such as `zeno-fcis/candidate` or choose the profile prefix
`zeno-fcis`. Its commitments would then share a domain with library
identities, which defeats the purpose of domain separation.

This package reserves the namespace and adds project constructors that reject
it.

## Inputs and outputs

- `zeno_fcis_project::RESERVED_DOMAIN_NAMESPACE` is `zeno-fcis`.
  `is_reserved_domain_name(name)` is true for `zeno-fcis` itself and every
  name that starts with `zeno-fcis/`, and false otherwise. For example,
  `zeno-fcis-app/state` and `myproject/zeno-fcis/x` are not reserved.
- `DomainPrefix::try_new_project` returns `ProfileError::InvalidDomainPrefix`
  for a reserved prefix, in addition to every rejection `try_new` makes.
- `StateDomainBinding::try_new_project` returns
  `AuthorityError::Encode(EncodeError::InvalidDomain)` for a reserved state
  domain, in addition to every rejection `try_new` makes.

## Authority boundary

The project constructors change no canonical bytes and no identity. The V1
constructors keep accepting reserved names, so existing profiles and stores
are unchanged. Projects opt in by calling the project constructors. Making the
check mandatory is recorded for V2.

## Trusted dependencies

The check compares bytes only. It adds no dependency.

## Deterministic resource bounds

The check is one prefix comparison over an already bounded name.

## Laws

1. A name is reserved exactly when it equals `zeno-fcis` or starts with
   `zeno-fcis/`.
2. A project constructor accepts a name only if the corresponding V1
   constructor accepts it and the name is not reserved.

## Negative cases

- `zeno-fcis`, `zeno-fcis/`, `zeno-fcis/candidate`, and `zeno-fcis/value` are
  reserved.
- `zeno-fcisx`, `zeno-fcis-app/state`, `myproject/zeno-fcis/x`, and `zeno` are
  not.
- `try_new_project` rejects reserved state domains and prefixes, and the V1
  `try_new` constructors still accept them.

## Assumptions

- Library code keeps its own domain names inside the reserved namespace.

## Explicit nonclaims

- The V1 constructors do not enforce the reservation. Code that calls them
  can still choose a reserved name.
- This package does not register individual library domains or detect a
  collision between two library domains. A registry with golden preimages is
  separate follow-up work.
