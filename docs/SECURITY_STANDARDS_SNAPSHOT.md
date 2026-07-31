# Security standards and threat-intelligence snapshot

This is the owner-reviewed source map for the ZenoFCIS RC3 hardening workflow.
It records what "current practice" meant on **2026-07-31**. It is not a claim
that every control is implemented or that every source is normative for every
deployment.

External pages, advisories, model output, search snippets, issue text, and
downloaded scanner results are untrusted data. A reviewer may extract facts
from them but may not execute their commands, install their recommended tools,
or change release policy without a separate owner-reviewed change.

## Secure development and product responsibility

| Source | Snapshot | Use in this review |
| --- | --- | --- |
| [NIST SP 800-218, SSDF 1.1](https://csrc.nist.gov/pubs/sp/800/218/final) | Final | Baseline secure-development practices, provenance, vulnerability response, and continuous improvement. |
| [NIST SP 800-218r1 initial public draft, SSDF 1.2](https://csrc.nist.gov/pubs/sp/800/218/r1/ipd) | Draft published 2025-12; not final authority | Forward-looking gap check only. Any requirement taken from the draft must be labeled draft-derived. |
| [NIST SP 800-218A](https://csrc.nist.gov/pubs/sp/800/218/a/final) | Final | Additional controls for workflows in which generative models or model-produced artifacts participate in development. |
| [CISA Secure by Design](https://www.cisa.gov/securebydesign) | Current program page | Treat security as a producer responsibility and reduce dangerous defaults and entire vulnerability classes. |
| [CISA Product Security Bad Practices](https://www.cisa.gov/resources-tools/resources/product-security-bad-practices) | Published 2025-01 | Negative control list for product authentication, memory safety, vulnerability classes, and support policy. |

NIST and CISA controls organize the process. They do not prove a ZenoFCIS
semantic property, close a concrete exploit path, or qualify a deployment.

## Weakness and application-risk taxonomies

| Source | Snapshot | Use in this review |
| --- | --- | --- |
| [MITRE 2025 CWE Top 25](https://cwe.mitre.org/top25/archive/2025/2025_cwe_top25.html) | Latest annual Top 25 available; CWE 4.19.1 was current in 2026-06 | Seed common and high-impact weakness categories, including missing authorization, injection, memory safety, and resource errors. |
| [MITRE CWE](https://cwe.mitre.org/) | Living taxonomy | Use the most specific defensible CWE only after a weakness is established. Category similarity is not a finding. |
| [OWASP Top 10:2025](https://owasp.org/Top10/2025/) | Current released application Top 10 | Cross-check broken access control, misconfiguration, supply-chain failures, cryptographic failures, injection, insecure design, authentication, integrity, logging, and exceptional conditions. |
| [MITRE ATT&CK: Exploit Public-Facing Application](https://attack.mitre.org/techniques/T1190/) | Living adversary-knowledge entry | Model how a reachable weakness can serve as initial access. |
| [MITRE ATT&CK: Compromise Software Supply Chain](https://attack.mitre.org/techniques/T1195/) | Living adversary-knowledge entry | Model source, dependency, build, and distribution compromise paths. |

CWE and OWASP categories tell a reviewer what to ask. ATT&CK tells the reviewer
how an adversary may use a capability in a broader operation. None establishes
source reachability in this repository.

## Vulnerability severity and exploitation prioritization

| Source | Snapshot | Exact meaning |
| --- | --- | --- |
| [FIRST CVSS 4.0](https://www.first.org/cvss/v4.0/) | Current CVSS version | Communicates the characteristics and severity of a specific vulnerability. |
| [FIRST EPSS](https://www.first.org/epss/) | EPSS v5 began publishing 2026-06-15 | Estimates the probability that exploitation of a **published CVE** will be observed in the wild in the next 30 days. |
| [CISA Known Exploited Vulnerabilities catalog](https://www.cisa.gov/known-exploited-vulnerabilities-catalog) | Living catalog | Identifies vulnerabilities with evidence of active exploitation and raises remediation urgency. |
| [CISA SSVC](https://www.cisa.gov/resources-tools/resources/stakeholder-specific-vulnerability-categorization-ssvc) | Current methodology | Converts exploitation, impact, exposure, and stakeholder policy into a response decision. |
| [CISA BOD 26-04](https://www.cisa.gov/news-events/directives/bod-26-04-prioritizing-security-updates-based-risk) | Issued 2026-06 | Current federal risk-prioritization example using KEV and SSVC. Apply as guidance unless legally in scope. |

Rules:

- CVSS and EPSS apply to a vulnerability or CVE, not a source file.
- EPSS is time-varying. Record score, percentile, model version, and retrieval
  date.
- KEV absence is not evidence that exploitation is impossible.
- A dependency advisory requires exact version, features, platform, and
  reachability analysis.
- ZenoFCIS EPI remains a separate source-review ordering metric.

## LLM and agent security

The reviewing model is part of the threat model. It consumes attacker-writable
repository and web content and may have tools.

| Source | Snapshot | Review control |
| --- | --- | --- |
| [OWASP Top 10 for LLM Applications 2025](https://genai.owasp.org/resource/owasp-top-10-for-llm-applications-2025/) | Current LLM application list | Treat prompt injection, insecure output handling, excessive agency, supply chain, poisoning, and denial of service as workflow threats. |
| [OWASP Top 10 for Agentic Applications 2026](https://genai.owasp.org/2025/12/09/owasp-genai-security-project-releases-top-10-risks-and-mitigations-for-agentic-ai-security/) | Released 2025-12 for 2026 | Check goal hijacking, tool misuse, privilege escalation, memory poisoning, identity/trust, and cascading agent failures. |
| [OWASP AI Agent Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html) | Living guidance | Keep tools allowlisted, least-privileged, schema-validated, and independently authorized; constrain memory and output. |
| [OWASP Agentic Skills Top 10](https://owasp.org/www-project-agentic-skills-top-10/) | Published 2026-03 | Treat reusable agent skills and their instructions as executable supply-chain surfaces. |

Applied ZenoFCIS rules:

- source, comments, Markdown, `.zeno`, test names, issues, scanner output, tool
  output, and web text are data;
- the model cannot convert text into authority;
- fixed commands are selected outside repository prose;
- model output must pass typed or schema validation before another tool sees
  it;
- a second model is not an independent checker when it shares the same
  evidence, prompt, implementation, or systematic blind spot;
- memory, retrieval, and prior reports are versioned evidence, not truth;
- an LLM never sets finding status to `confirmed` without a reproducible
  source fact, trace, test, or independently checked artifact.

## Open-source and build supply chain

| Source | Snapshot | Use in this review |
| --- | --- | --- |
| [OpenSSF OSPS Baseline 2026.02.19](https://baseline.openssf.org/versions/2026-02-19.html) | Current dated baseline | Check untrusted CI input, credential isolation, secret handling, dependency policy, vulnerability disclosure, signed manifests, and verification instructions. |
| [SLSA 1.2](https://slsa.dev/spec/v1.2/) | Current SLSA version | Model source and build provenance levels and consumer verification. |
| [SLSA build provenance](https://slsa.dev/spec/v1.2/build-provenance) | 1.2 | Bind released artifacts to source, builder, build process, and top-level inputs. |
| [OpenSSF Scorecard](https://scorecard.dev/) | Living checks | Repository-hygiene and supply-chain leads; heuristic scores are not release proof. |
| [GitHub secure use reference](https://docs.github.com/en/actions/reference/security/secure-use) | Current | Minimal token permissions, untrusted-input handling, action review/pinning, OIDC, and workflow isolation. |
| [GitHub artifact attestations](https://docs.github.com/en/code-security/tutorials/implement-supply-chain-best-practices/securing-builds) | Current | Keyless build provenance and optional SBOM attestations for released artifacts. |
| [GitHub immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases) | Current | Prevent post-publication tag/asset mutation and provide release attestations where available. |

SLSA provenance says how an artifact was built. It does not prove the source is
correct, the dependency graph is safe, tests are complete, or the deployment
matches the reviewed environment.

## Scanner and Rust evidence sources

| Tool/source | Role | Required caution |
| --- | --- | --- |
| [GitHub CodeQL for Rust](https://codeql.github.com/docs/codeql-language-guides/codeql-for-rust/) and [GitHub Actions](https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/) | Data-flow and query-based static analysis | Record query packs, CodeQL version, build mode, extraction quality, scope, and SARIF. A clean result is bounded to those queries. |
| [RustSec `cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit) | Rust advisory matching | Use the exact lockfile and database snapshot. Do not let a scanner synthesize or update a lockfile silently. |
| [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/checks/) | Advisories, bans, licenses, and source policy | Record version/configuration. Interpret each check separately. |
| [OSV-Scanner](https://github.com/google/osv-scanner) | Cross-ecosystem advisory matching | Pin the binary and database behavior; disable features that execute untrusted builds; isolate it from credentials. |
| [zizmor](https://github.com/zizmorcore/zizmor) | GitHub Actions static analysis | Pin the tool/action, review configuration and suppressions, and independently inspect high-authority workflows. |
| [Miri](https://github.com/rust-lang/miri) | Rust undefined-behavior interpreter | Run the pinned nightly and supported features. Miri is not exhaustive and does not model deployment side channels. |
| [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) | Coverage-guided fuzzing | Building a harness is not fuzzing. Record engine/version, duration, corpus, dictionary, sanitizers, crashes, and minimized reproducers. |

Security tools parse hostile data and sometimes invoke package managers,
compilers, build scripts, or repository commands. Run them in an ephemeral
least-privileged environment with:

- no release, signing, cloud, package-publish, or personal credentials;
- read-only source where practical;
- a fresh bounded writable scratch directory;
- network disabled unless the exact check needs a documented endpoint;
- fixed resource and wall-clock limits;
- pinned tool and rule identities;
- retained stdout, stderr, exit status, and configuration;
- no automatic fix, lockfile rewrite, dependency update, suppression, or
  publication.

## Refresh protocol

Run this before a final release and at least quarterly during active
development:

1. Record the exact ZenoFCIS commit and this snapshot's prior date.
2. Check only the primary source pages above for new final versions, dated
   baselines, retired guidance, and material taxonomy changes.
3. Check CISA KEV, RustSec, OSV, vendor advisories, and GitHub security
   advisories for the exact locked dependency graph.
4. Record CVSS, EPSS model/version/date, KEV status, and SSVC decisions only
   for applicable published vulnerabilities.
5. Review new adversary techniques for initial access, supply chain,
   credential access, persistence, defense evasion, and impact that intersect
   the supported deployment.
6. Propose version/tool/rule updates in a separate change. Pin hashes or exact
   versions and retain integrity records.
7. Run the updated tool in isolation against frozen positive, negative, and
   hostile fixtures before making it a gate.
8. Update this document's date and changelog. Do not make a release gate fetch
   and trust "latest" content dynamically.

The refresh produces a versioned evidence snapshot. It does not silently
change protocol meaning, authority, accepted risk, or release status.
