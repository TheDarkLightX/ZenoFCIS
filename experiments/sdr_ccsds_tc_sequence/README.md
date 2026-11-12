# CCSDS telecommand sequence pilot (ZenoFCIS)

**Status:** exploratory; ZenoFCIS CLI verification not yet run.
**Upstream case:** [gr-satellites issue #674](https://github.com/daniestevez/gr-satellites/issues/674), inspected at main commit 0ff8b614c3ed57110c3497c287bced9a01bbc238 on 2026-11-12.

This pilot models one CCSDS transfer-frame primary-header sequence counter and
its bypass behavior as two bounded, finite synthesis problems. The on-wire
sequence number is an eight-bit field. The initial sequence and bypass reset
semantics here follow the current gr-satellites block, with non-bypass
increments moved to **after** frame emission as the maintainer suggested.
It does not establish all CCSDS protocol obligations.

The model is a pure FCIS core. Inputs are immutable (sequence, bypass), and
outputs are (sent, successor):

    bypass = true:  sent = 0,        successor = 0
    bypass = false: sent = sequence, successor = (sequence + 1) mod 256

GNU Radio/PMT message delivery, framing, and transmission are imperative-shell
concerns outside this finite relation.

## Why two problems?

The current finite-i128/1 profile allows at most 4,096 output tuples. A joint
pair of 8-bit outputs has 65,536 combinations, exceeding that limit.
We therefore synthesize sent and successor separately, each over all **512**
possible (sequence, bypass) inputs. The pair is replayed against an independent
Python oracle. This is a composition *test*, not a composition proof.

## Independently exercise the proposed specification

From this directory, without GNU Radio or the ZenoFCIS CLI:

    python3 -m unittest -v test_pilot.py

The test checks both relations for all 131,072 input/output combinations
*each*, enumerates the candidate grammar (three choices for sent; eighteen
for successor), checks four planted defects, and replays 1,024 non-bypass
transitions. Passing is finite oracle evidence, **not** a ZenoFCIS receipt.

## Run actual ZenoFCIS synthesis and target conformance

With the ZenoFCIS 2.7 CLI installed, from this directory:

    mkdir -p receipts
    zeno-fcis synth run sent.json --target python --out sent-python \
      --max-assignments 20 --max-steps 10000000
    zeno-fcis synth verify sent.json --target python --out sent-python \
      --max-assignments 20 --max-steps 10000000 \
      --receipt receipts/sent-python.json
    zeno-fcis synth run successor.json --target python --out successor-python \
      --max-assignments 20 --max-steps 10000000
    zeno-fcis synth verify successor.json --target python --out successor-python \
      --max-assignments 20 --max-steps 10000000 \
      --receipt receipts/successor-python.json

Output directories must be new or empty. Store receipts outside the immutable
generated artifacts. A synth run alone does not establish runtime conformance;
inspect both synth verify receipts and their source identities. Optionally
repeat with --target rust in separate directories and receipts.

**The actual ZenoFCIS CLI was unavailable in the authoring environment**;
no CLI synthesis or target-conformance pass is claimed.

## Replay upstream code at the source level

The optional integration harness loads the *actual gr-satellites Python source*
from a checkout, replacing GNU Radio message transport and Construct header
serialization with controlled stubs. Given a source checkout:

    GR_SATELLITES_ADDER_SOURCE=/path/to/gr-satellites/python/ccsds/telecommand_primaryheader_adder.py \
      python3 -m unittest -v test_adder_integration.py

In an isolated local replay of the inspected upstream source, its tests fail:
the first non-bypass frame emits 1 instead of 0, and a 254-to-255 transition
then tries to build 256. After increment-after-publication plus modulo 256,
all three source-level tests pass. This does not run the real GNU Radio runtime
or test RF behavior; upstream integration qualification is still outstanding.

## Upstream policy

Read gr-satellites' [LLM usage policy](https://github.com/daniestevez/gr-satellites/blob/main/LLM_POLICY.md)
before proposing a change. Its experiment for LLM-created code requires a
willing reviewer arranged in advance, disclosure, an llm-assisted label,
thorough tests, and personal review. Ordinary LLM-created issue comments and
PR descriptions are not allowed. **Do not submit an upstream PR automatically
from this pilot.** The code here is an independent ZenoFCIS research artifact,
not a gr-satellites contribution.
