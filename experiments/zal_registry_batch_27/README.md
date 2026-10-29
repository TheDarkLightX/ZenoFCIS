# Built-in Lens catalog construction

The built-in catalog used to call public registration repeatedly while building
its private list. Each call validated and copied growing prefixes. It now builds
fresh definitions locally and validates the complete returned catalog once.
Caller-supplied registration, registry validation and exact adapter matching keep
their existing checks. No cache, interface or trusted dependency is added.

The actual 15-definition catalog remains exactly 9,523 encoded bytes. Definition
validation calls change from 255 to 30, and complete registry validations from
30 to one. The definitions already deep-copy their fields; the added ownership
test checks that consumer edits cannot change another catalog or adapter data.

Matched measurements used the published producer at
`8de5a6543f93353d2f4563b17badfde285ce46a2`, the candidate source SHA in
[qualification.json](qualification.json), Python 3.12.3, one CPU, and three
repetitions per backend. Each worker had 1 GiB address space, 90 CPU seconds and
120 wall seconds. Workloads and procedure were frozen before comparison.

| Full export plus fresh replay | Baseline median | Candidate median | Ratio |
| --- | ---: | ---: | ---: |
| Representative five-object library | 38.32 s | 15.36 s | 2.49× |
| Separately frozen latch/draft library | 15.05 s | 4.53 s | 3.32× |

The [raw worker results](measurements/baseline-01.json) and
[aggregate measurements](performance.json) retain all repetitions. Artifact
counts and encoded sizes match. Catalogs, public refusal details and semantic
packet projections match. The projection removes only documented source-bound
identities at explicit application/receipt locations, retaining source, target,
mapping, status and mathematical evidence. Each complete packet independently
replays under its current source; old receipts are not accepted as new ones.

Warm catalog construction medians were 92.45 ms and 6.91 ms across 75 samples per
backend. This is separate from complete export/replay. Observed worker peak RSS
was about 31 MiB for both; no memory reduction is claimed. These measurements do
not establish whole-factory speed, a universal crossover, an asymptotic
whole-program bound or a complete allocation/cold-start model. The
[five independent feasibility assessments](feasibility.json) retain those
unknowns. The record checker checks bindings, not algorithm truth or authority.

All 46 affected cases passed in each Python mode. The combined normal run hit
its 90-second deadline after 43 completed cases; those results were preserved
and only the remaining three were run again. Optimized modules ran separately
under the same per-group limit. The optional-adapter child test now uses
`unittest` checks that remain active under optimization.

Negative results remain in the qualification record: the earlier 90-second
selection and 60-second profiled export, the later combined-suite deadline, and
the finite Mealy adapter's refusal of an initial state named `latched`. This
adapter currently requires `unseen`; that limitation does not constrain general
ZAL authoring. The frozen supported workload declares its initial name explicitly.

## Reproduction

The [worker](worker.py), [supervisor](run_measurements.py) and
[owned-process helper](owned_process.py) are byte-identical copies of the
measured recipes. They use only Python's standard library. The bounded supervisor
requires Linux pidfds and an actual host context; it does not install tools or
change source. The workers write fresh case directories and refuse existing ones.
Keep source frozen and reserve the shared native-build slot before measurement
when other agents or build workers use the host.

Prepare separate baseline and candidate source roots. Copy
`pins.template.json` to `MEASURE_PINS_01.json`, supply those local roots and the
reviewed local interpreter path, and recompute the descriptor's SHA-256. Keep
the expected source/workload/helper hashes fixed. A different interpreter or
recipe needs its own reviewed measurement record. Then use:

```sh
python3 -I -B run_measurements.py --pins-sha256 REVIEWED_DESCRIPTOR_SHA256 --check-only
python3 -I -B run_measurements.py --pins-sha256 REVIEWED_DESCRIPTOR_SHA256
```

The template is an environment-specific replay aid, not the original descriptor.
The qualification record retains the original descriptor hash. No test,
measurement or record authorizes release, deployment or acceptance of meaning.
