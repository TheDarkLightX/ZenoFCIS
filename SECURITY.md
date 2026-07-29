# Security policy

## Supported versions

ZenoFCIS `1.0.0-rc.2` is a pre-release candidate. Security fixes are applied to
the current release-candidate line until it is superseded. Older development
branches and unqualified downstream deployments are not supported release
surfaces. Final support guarantees begin only when they are published with
`1.0.0`.

| Version | Security-fix status |
| --- | --- |
| `1.0.0-rc.2` | Current candidate; fixes are issued through a new RC |
| older branches and commits | Unsupported |

## Private reporting

Report suspected vulnerabilities through GitHub's private security-advisory
interface for `TheDarkLightX/ZenoFCIS`. Do not disclose a suspected issue in a
public issue, pull request, discussion, or chat transcript before coordinated
disclosure. Do not include private keys, production secrets, personal data, or
live-system exploit data.

Include, when available:

- affected version, exact commit, features, target, and Rust toolchain;
- the smallest reproduction or canonical input artifact;
- expected and observed behavior;
- authority, state, effect, outbox, persistence, or availability impact;
- whether the report depends on a downstream profile or deployment choice;
- a proposed test or invariant that would prevent recurrence.

Maintainers aim to acknowledge a complete report within three business days,
then coordinate validation, remediation, release, and disclosure timing with
the reporter. This target is not a guarantee of a particular fix date. Please
allow the private process to complete before public disclosure.

## Security-relevant boundaries

Reports are especially relevant when they demonstrate:

- noncanonical or ambiguous protocol admission;
- unauthorized construction or publication of state, effects, or outbox work;
- incorrect Reject or CommittedFailure semantics;
- stale-root, replay, candidate-binding, or persistence-integrity bypasses;
- resource-limit or allocation-amplification failures at hostile boundaries;
- undeclared ambient effects or unsafe code in semantic crates;
- forged promotion, proof, provider, projector, interpreter, or deployment
  evidence;
- secret exposure or a concrete side/covert-channel claim violation.

The semantic kernel forbids unsafe Rust and is designed to remain independent
of clocks, randomness, networking, filesystems, databases, threads, async
runtimes, and ambient process state. A violation of those boundaries is
security relevant even when no memory-safety defect exists.

## Scope and nonclaims

The candidate includes nominal catalog, law, invocation, provider,
interpreter, deployment, replay, genesis, and authenticated-publication
authority mechanisms. Those library mechanisms do not qualify a downstream
project's laws, selected verifier, storage backend, effect interpreter,
hardware, operational controls, or deployment.

A report about an application-specific policy may belong to that downstream
project. A reusable-library bypass, unsafe default, misleading production
claim, or failure to enforce a documented ZenoFCIS boundary belongs here.
This policy does not establish audit completion, value-custody approval,
physical side-channel resistance, or general production qualification.
