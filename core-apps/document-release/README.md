# Document release

An author edits one document under optimistic versioning, submits the current
revision for review, two of three reviewers approve, and the approved revision
is released exactly once on channel 300 to `publisher`, with payload (content,
version). Submission locks the content: the released content and version are
always the ones that were reviewed. A release retry with the current version
is accepted unchanged and emits nothing.

## Deliberate difference from the earlier design note

The V2.3 research note proposed that editing during review invalidates earlier
approvals. The catalogue's approval queue has no reset or cancel, so that would
need a new family. This profile instead refuses edits during review
(`wrong_phase`). The note's finite phase machine is not used: the approval
queue's phase is the lifecycle (Draft, Review, Released), and a second phase
field would only duplicate it.

## Components

- `register`: versioned register `C=2`, owning content (110) and version (111).
  Edit is its Write; Submit and Release only read the version.
- `review`: approval queue `K=2`, owning phase (112) and the three votes
  (113..115). Its state view is `phase + 150` and the votes. Its command view
  is `action - 1`: Submit 161 to Enqueue 160, Approve 162 to Vote 161 and
  Release 163 to Execute 162. Edit 160 would map to 159, which is not a queue
  command; no occurrence steps the queue on Edit.

The idempotent release retry makes no component transition. Each occurrence's
listed step order is descriptive: every step reads the same pre-state.

## A resolved ambiguity in the brief

The requirements brief does not settle one case: a Release with a stale
`expected_version` once the document is Released. Its wrong-phase rule refuses
"any other command" in Released as `wrong_phase` (202), and its stale-version
rule refuses a stale Release as `stale_version` (205). The reference and the
contract both answer 202: in Released, only the exact retry with the current
version is accepted, and every other command, including a stale Release, is in
the wrong phase. Either way the decision refuses and changes nothing; only the
reason code differs.

## Contract and evidence

The complete input domain has 46,656 tuples, including invalid coupled states.
The rules and coupled laws in `project.zeno` and `v2/policy.json` were written
from the requirements brief and compile into the ordinary library-owned
Authority contract. The independent Python reference used for the decision
examples was written separately from the same brief; the complete actual
Authority table is compared with it on every input. Neither is an owner
adoption or an independent review.

## Limits

- One document, one review cycle. No re-edit after submission, no withdrawal,
  no second release. Approvals are supplied reviewer slots, not authenticated
  people. Content is a three-value tag; binding a real document's bytes or
  digest is a separate adapter qualification.
- A release intent records a decision; it does not prove publication.
- Closed finite scope only. No family-wide theorem.

`tools/check_core_app.py --app document-release --static` checks the complete
reference corpus, and that the contract's rules, read through the names the
composition's views use, reproduce every written decision. On every accepted
input, each stepping component's input and successor, derived from the
composition's views, lie in its instance's domains, and its local reference
accepts that input and yields that successor; components that take no step keep
their state, the owners partition the state, and the declared effect is the
whole outbox. Every accepted input fires at least one declared occurrence, and
every declared occurrence fires on at least one accepted input. Separately, it
checks the composition against its reviewed pin and the component sources
against the catalogue. Its execution mode also checks the actual Authority, a
freshly scaffolded app, the durable journey and law-violating candidates.
