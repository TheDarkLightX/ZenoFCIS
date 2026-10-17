# Security policy

## Supported versions

ZenoFCIS `1.0.0` started the stable release line. `2.3.0`, the first stable
V2 release, is the current stable release, and the `2.3.x` line is the only
supported line. Compatible security fixes are issued as patch releases.
Release candidates, development branches and unqualified downstream
deployments are not supported release surfaces.

| Version | Security-fix status |
| --- | --- |
| `2.3.x` | Supported stable line; update to the latest patch release |
| `1.1.x` | Superseded by `2.3.x`; no longer supported |
| `1.0.x` | Superseded by `2.3.x`; no longer supported |
| `2.2.0-rc.1`, `2.3.0-rc.1` | Prereleases superseded by `2.3.x`; not supported |
| `1.0.0-rc.*` | Superseded by the stable release; not supported |
| older branches and commits | Unsupported |

## Private reporting

Report suspected vulnerabilities through GitHub's private security-advisory
interface for `TheDarkLightX/ZenoFCIS`. Do not disclose a suspected issue in a
public issue, pull request, discussion, or chat transcript before coordinated
disclosure. Do not include private keys, production secrets, personal data, or
exploit data from live systems.

Include, when available:

- affected version, exact commit, features, target, and Rust toolchain;
- the smallest reproduction or canonical input artifact;
- expected and observed behavior;
- authority, state, effect, outbox, persistence, or availability impact;
- whether the report depends on a downstream profile or deployment choice;
- a proposed test or invariant that would prevent recurrence.

The deterministic [hotspot model](docs/SECURITY_HOTSPOT_MODEL.md) and
[review playbook](docs/SECURITY_REVIEW_PLAYBOOK.md) can help you identify and
describe the affected boundary. An EPI score or a scanner match alone is not a
vulnerability report; include reachability, impact, and evidence.

Maintainers aim to acknowledge a complete report within three business days,
then coordinate validation, remediation, release, and disclosure timing with
the reporter. This target does not guarantee a particular fix date. Please let
the private process finish before public disclosure.

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
- secret exposure or a concrete side/covert-channel claim violation;
- an LLM, scanner, generated artifact, workflow, dependency, or compromised
  evidence producer obtaining authority or composing with another weakness;
- a reproducible multi-stage chain from untrusted input or supply-chain
  control to publication, persistence, release, secret access, or external
  effects.

The semantic kernel forbids unsafe Rust and is designed to stay independent
of clocks, randomness, networking, filesystems, databases, threads, async
runtimes, and ambient process state. A violation of those boundaries is
security-relevant even when no memory-safety defect exists.

## Scope and nonclaims

The candidate includes nominal catalog, law, invocation, provider,
interpreter, deployment, replay, genesis, and authenticated-publication
authority mechanisms. Those library mechanisms do not qualify a downstream
project's laws, selected verifier, storage backend, effect interpreter,
hardware, operational controls, or deployment.

A report about an application-specific policy may belong to that downstream
project. A bypass of the reusable library, an unsafe default, a misleading
production claim, or a failure to enforce a documented ZenoFCIS boundary
belongs here. This policy does not establish audit completion, value-custody
approval, physical side-channel resistance, or general production
qualification.
