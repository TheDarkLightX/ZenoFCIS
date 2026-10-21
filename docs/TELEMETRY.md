# Optional invocation telemetry

Set `ZENO_FCIS_TELEMETRY` to an append-only JSON-lines file. Each normal CLI
invocation and Python check-tool entrypoint attempts one record with schema
`zeno-fcis/telemetry/1`. An unset or empty variable disables the observer.
An observation failure leaves the command's result and output unchanged.

Records contain UTC `timestamp`, `actor` (from `ZENO_FCIS_COMMONS_ACTOR`, else
`unknown`), a command-word array, input names and SHA-256 digests, the digest
of the sorted distinct input digests joined by newlines (`subject_sha256`),
`exit_code`, a normalized `verdict` when available, numeric `work_units` when
observed, and monotonic `duration_ms`. No arguments, source contents, result
bodies or filesystem paths are serialized. Work is advisory observation, never
resource accounting authority. Non-verdict operations have a null verdict.

The CLI snapshots file-typed operands and default inputs before execution;
contract directories contribute their project, policy and decision examples.
Output-only operands are excluded. A file that cannot be hashed, is special,
or exceeds the observer's 64 MiB read cap has a null digest. Python tools hash
regular file operands with a streaming SHA-256. Implicit transitive reads are
outside this observation inventory; these records are not source manifests.
Contained workers do not append a second CLI invocation record.

The writer opens with `O_APPEND` and performs one write containing one complete
JSON line. It does not retry a short write. Import refuses incomplete lines;
previously imported complete lines remain retained.

```sh
python3 -I integrations/commons/cli.py import-telemetry observations.jsonl
python3 -I integrations/commons/cli.py loops
```

The importer validates each record and appends a `factory.cli.<command>`
event. The exact line's SHA-256 (excluding its terminator) is a durable
deduplication key in the existing meta table. Event and key share one write
transaction. Re-importing that line appends nothing, including after archival.

An episode is a maximal consecutive run with the same actor and subject.
Changing either splits it; missing subjects are singletons. Summaries include
all runs and failure rates, and mean iterations and elapsed time to the first
accepted verdict in completed episodes. Iterations include all command calls
in that episode prefix; time includes the gaps between calls and the last
call's duration. Incomplete episodes keep null acceptance measures. Records
are summarized in import order, so importing out of order changes episodes.
Telemetry is untrusted coordination data and establishes no checker verdict,
performance comparison, production authority or release qualification.
