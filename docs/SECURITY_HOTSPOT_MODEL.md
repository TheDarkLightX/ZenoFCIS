# Security hotspot and exploit-chain model

This document defines the deterministic model used by
`tools/security_hotspots.py`. The model helps a reviewer decide where to spend
scarce attention. It does not decide whether a vulnerability exists.

## The number means review priority

The scanner emits an **Exploitability-Potential Index (EPI)** from 0 through
100 for each admitted source or security-control file.

EPI is:

- ordinal: 90 should be reviewed before 60 under the same model and scope;
- decomposable: every point comes from six visible components;
- deterministic: the same admitted files and model produce identical JSON;
- source-based: it uses architectural path roles and bounded lexical signals;
- prompt-minimized: it reports canonical safe-ASCII paths, rule identifiers,
  and line numbers, not matched source text.

EPI is **not**:

- a finding or proof that a weakness exists;
- the probability that a file contains a vulnerability;
- the probability that an attacker will exploit anything;
- vulnerability severity, CVSS, EPSS, KEV, or SSVC;
- a replacement for reachability, data-flow, threat-model, or deployment
  analysis;
- a reason to approve or reject a release by itself.

Calling EPI a probability would be false precision. ZenoFCIS does not yet have
a sufficiently large, representative, independently labeled history of source
snapshots, reviews, and confirmed vulnerabilities from which to calibrate that
probability.

## Formula

Each component is an integer from 0 through 5. A component takes the maximum
level produced by any matching owner-reviewed path role or lexical rule.
Repeated tokens do not add unbounded points.

\[
\operatorname{EPI}
=
\operatorname{round}\left(
\frac{
25A + 20R + 20H + 15S + 10C + 10X
}{5}
\right)
\]

where:

| Symbol | Weight | Question |
| --- | ---: | --- |
| \(A\), authority | 25 | Can this surface mint, validate, persist, promote, release, or consume security-relevant authority? |
| \(R\), reachability | 20 | How directly can untrusted bytes, project text, process output, database state, workflow metadata, or public callers reach it? |
| \(H\), hazardous mechanisms | 20 | Does it parse, execute processes, access SQL/files/network, handle secrets/crypto, generate code, or use unsafe/concurrent mechanisms? |
| \(S\), state coupling | 15 | Does correctness span roots, versions, replay, transactions, receipts, outboxes, acknowledgements, time, or multiple authority domains? |
| \(C\), complexity | 10 | How much production code and security-signal diversity must a reviewer reason about? |
| \(X\), chain adjacency | 10 | How many distinct vulnerability-category lenses meet at this surface? |

The implementation uses integer arithmetic:

```text
(25*A + 20*R + 20*H + 15*S + 10*C + 10*X + 2) // 5
```

The score is monotone: adding a higher architectural or mechanism signal
cannot lower it. Tests, proofs, and documentation do not lower inherent
potential. They belong in the separate evidence record. This prevents a test
name or proof claim from numerically erasing a high-authority boundary.

## Priority bands

The bands name review order, not vulnerability severity:

| EPI | Band | Required first action |
| ---: | --- | --- |
| 85–100 | `priority-1` | Review in the first pass and before release promotion. |
| 70–84 | `priority-2` | Review after the first-pass trust boundaries. |
| 50–69 | `priority-3` | Review in the category and dependency pass. |
| 30–49 | `priority-4` | Review when connected to a finding, change, or deployment path. |
| 0–29 | `priority-5` | Do not skip automatically; sample and revisit when the model or threat surface changes. |

`confidence` describes how many deterministic path and line signals produced a
ranking. It does not express confidence that a vulnerability exists.

## Admitted scope and safety limits

The scanner admits:

- production Rust under `crates/*/src`;
- host-side Python under `tools`;
- GitHub Actions workflows;
- root dependency, toolchain, and agent-guardrail configuration.

It excludes tests, examples, fixtures, benchmarks, and demos from hotspot
ranking because those are evidence or non-production surfaces. A production
file's conventional trailing Rust `#[cfg(test)] mod tests` is also removed
before matching.

The scanner:

- reads at most 10,000 candidate files;
- reads at most 2 MiB per file and 64 MiB in aggregate;
- requires UTF-8 and rejects candidate symlinks;
- rejects noncanonical or Markdown-active candidate paths and detects a file
  that changes while it is read;
- uses fixed `git ls-files` arguments when a Git checkout is present;
- never imports, compiles, evaluates, or executes repository source;
- removes Python strings/comments and Rust comments before matching;
- never embeds matched source lines in JSON or Markdown.

These controls matter because a security scanner is itself an attack surface.
They reduce output-injection surface; they cannot make content trustworthy.
Repository text, accepted filenames, generated source, comments, workflow
prose, and scanner output remain untrusted data.

## Rule evidence

Every hotspot contains:

- the canonical repository path;
- EPI, band, and component levels;
- matched architectural role identifiers;
- matched lexical rule identifiers and bounded line-number lists;
- review-category identifiers and associated CWE families;
- a `production_lines` complexity input;
- the scanner model digest, admitted-inventory digest, and full-scope tier
  counts at report level.

The complete category questions, verification suggestions, path roles, regular
expressions, weights, and scales are bound by `model.sha256`. A model change
must intentionally update both the model digest and the committed baseline.

Lexical matches are leads. For example, a process-execution match can be a
strict fixed-argument verifier adapter rather than command injection. The
reviewer must inspect callers, data origin, validation, authority, failure
handling, and tests before classifying it.

## Current category families

The model gives lesser-capability reviewers a bounded lens for each hotspot:

- authority, authentication, and access control;
- input admission, parsing, and canonicalization;
- state, replay, concurrency, and transaction atomicity;
- effects, command/SQL injection, and filesystem boundaries;
- secrets, cryptography, and authentication evidence;
- resource exhaustion and algorithmic denial of service;
- CI/CD, dependencies, provenance, and release authority;
- memory safety, FFI, and synchronization;
- external verifier, evidence, and promotion boundaries;
- side channels, covert channels, and observable discrepancies;
- exceptional conditions, logging, and observability;
- code generation, build scripts, and derived artifacts.

The scanner's JSON `category_catalog` is authoritative for the exact questions,
CWE mappings, and suggested verification under a model digest.

## Candidate routes are not exploit chains

The report groups category-adjacent hotspots into candidate review routes such
as:

```text
untrusted input
  -> authority
  -> state/replay
  -> external effect
```

That grouping does not establish calls, data flow, reachability, compatible
preconditions, or an exploit. It tells a reviewer which boundaries must be
connected or separated with evidence.

An actual chain is a graph of findings. Every finding node must declare:

- exact attacker-controlled entry facts;
- required identity, privilege, state, deployment, and timing facts;
- the action or observation a reproducer establishes;
- the exact facts it provides to a later node;
- security impact and reliability;
- whether the node is confirmed, likely, hypothetical, or refuted.

An edge is valid only when the provider's postcondition satisfies the
consumer's precondition in the same supported deployment. Similar words,
shared CWE categories, or an LLM narrative are not edge evidence.

## Chain Feasibility Index

After findings and edges exist, a reviewer may calculate a separate
**Chain Feasibility Index (CFI)**. CFI is also ordinal and is not an exploitation
probability.

Score four 0-through-5 factors:

- \(E\): weakest edge evidence;
- \(T\): terminal authority or impact;
- \(P\): feasibility of attacker preconditions;
- \(Q\): repeatability and reliability.

\[
\operatorname{CFI} = 8E + 5T + 4P + 3Q
\]

Weakest-edge evidence levels are:

| Level | Evidence |
| ---: | --- |
| 0 | Missing, contradicted, or incompatible pre/postconditions. |
| 1 | Category adjacency or prose hypothesis only. |
| 2 | Plausible source/data-flow link with exact unresolved assumptions. |
| 3 | Complete source trace with matched facts and controlled node reproducers. |
| 4 | Reproducible end-to-end chain in an isolated representative environment. |
| 5 | Independent reproduction in a deployment-equivalent environment. |

Apply evidence caps:

- \(E \le 1\): CFI is at most 39 and the chain is `unsubstantiated`;
- \(E = 2\): CFI is at most 59 and the chain is `plausible`;
- \(E = 3\): CFI is at most 79 and the chain is `source-established`;
- \(E \ge 4\): the uncapped result may be called `reproduced`.

The cap prevents a dramatic terminal impact from laundering a missing edge
into a high-confidence chain.

## Known vulnerabilities use other systems

For a dependency or component with a published CVE:

1. record the CVE and affected-version evidence;
2. use CVSS 4.0 for vulnerability characteristics and severity;
3. use the current FIRST EPSS score only for its documented next-30-day
   in-the-wild exploitation estimate;
4. record whether CISA KEV identifies known exploitation;
5. apply the owner's SSVC decision policy and deployment context;
6. verify reachability and applicable features in the exact lockfile;
7. retain the advisory snapshot date and source.

Never put EPI into a CVSS vector. Never assign EPSS to an unpublished
hypothesis or source file. Never infer "not exploitable" from absence in an
advisory database.

## Baseline and anti-Goodhart rules

The committed `security/hotspots-baseline.json` is a review-control snapshot.
CI rejects drift so a reviewer must inspect:

- a new or removed hotspot;
- component, category, rule, or line-number changes;
- changes to the admitted inventory;
- scoring-model changes;
- priority reordering.

Do not optimize code to lower EPI. A lower score without a reduced trust
boundary is not a security improvement. Mitigation success is established by
closing the exact reachable path and adding evidence, not by deleting words
that a lexical rule recognizes.

Model revisions should be evaluated against frozen labeled snapshots. Useful
measurements include precision among the top \(k\) files, reviewer time to first
confirmed finding, category coverage, false-negative postmortems, rank
stability, and inter-reviewer agreement. Probability calibration metrics such
as Brier score become appropriate only after the model produces explicit
probabilities from a representative preregistered dataset.

## Commands

```bash
python3 tools/security_hotspots.py self-test
python3 tools/security_hotspots.py check
python3 tools/security_hotspots.py scan --format markdown --top 30
python3 tools/security_hotspots.py scan --format json --top 50
```

Run these only in a clean, isolated checkout of the exact revision. The
[security review playbook](SECURITY_REVIEW_PLAYBOOK.md) explains how to turn
the ranking into findings, chains, mitigation plans, and residual-risk
decisions.
