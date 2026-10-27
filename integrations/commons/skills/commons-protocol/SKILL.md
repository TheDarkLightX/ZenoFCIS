---
name: commons-protocol
description: Use Zeno Commons, the shared state of a ZenoFCIS software factory, whenever you work in a fleet of agents. Read before acting, write when you learn, acknowledge messages and lease what you use.
---

# Commons protocol

Zeno Commons is the factory's shared memory and coordination store. Other agents, the lead and the factory's
own tools read what you write there, and you read what they wrote. It records and cites; it proves nothing. Every
verdict in it comes from a ZenoFCIS checker or a person.

## Access

Use the MCP tools `commons_*` if your session has them. Otherwise use the command line, which works inside a
sandbox as long as the store directory is writable:

```sh
export ZENO_FCIS_COMMONS=<store directory from your brief>
export ZENO_FCIS_COMMONS_ACTOR=<your agent name from your brief>
C="python3 -I <repo>/integrations/commons/cli.py"   # or: zeno-fcis commons
```

Every command prints JSON. Exit 2 means refused, with the reason on stderr; exit 3 from `lease check` means
leased.

## Identity: register once

Every agent has a unique **name** and a **hash ID** (`ag-` followed by 16 hex digits). Register once at the start.
Leases, status, messages and records are refused until you do.

```sh
$C register --kind builder --role "collections"   # prints your id
$C whoami                                         # your identity
$C agents                                         # everyone: name, id, kind, role, state, read-through
$C send ag-1a2b3c4d5e6f7a8b "question about the merge order"   # address by ID or by name
```

Put your name and ID at the top of every report.

## The three rules

**1. Read before acting.** Before any risky step, ask what is already known. Risky steps include launching or
deleting something, choosing a design, starting a literature search, running a long check, or changing a shared
file.

```sh
$C advise "launch a sandboxed builder with a RAM-disk target" --tag codex-launch --text "/tmp/zf26-x"
$C records --type refutation --tag synthesis      # what has already failed here
$C inbox                                           # unread messages, numbered
```

Act on any lesson, refutation or lease it returns. If you go against one, say why in a record.

**2. Write when you learn, not at the end.** The moment you find something, record it with its evidence (paths,
commits, commands with exit codes, hashes). Don't wait for your final report.

```sh
$C add gap "A mailbox needs keyed collections" --tag collections --body "what_zenofcis_lacks: ..." --ref <evidence>
$C add lesson "Debug builds exceed the POSIX containment limit" --tag ci --tag containment --body "Set CARGO_PROFILE_DEV_DEBUG=0" --ref <log>
$C add hypothesis "Adaptive indexing halves search work" --tag synthesis
$C update <id> --status refuted --note "no gain below 300k comparisons" --ref <evidence>
$C update <id> --title "corrected title"                  # the old title stays in a dated note
$C link <refutation-id> refutes <hypothesis-id>
```

A record holds at most 65,536 characters. When one is full, add a new record and link it to the old one.

Gaps carry a severity: `--tag severity-blocker|high|medium|low` when you add one, or `$C update <id> --severity high`.
Before adding a gap, run `$C gaps --brief` and `$C dedup`. If yours is already there, fold yours into it with
`$C merge <yours> --into <existing>`. The merge keeps your references and counts you as an extra reporter, which
raises the gap's rank.

Record types: hypothesis, claim, evidence, refutation, counterexample, result, gap, lesson, decision, question,
answer, risk, note. Negative results matter most, because they stop the next agent repeating your dead end. Tag
records with the tool, the area and the action kind (for example `cargo`, `ci`, `codex-launch`, `zenoui`,
`collections`).

**3. Acknowledge and lease.**

```sh
$C lease take /tmp/zf26-x --note "collections build" --ttl 14400   # before using a folder, branch or port
$C lease renew /tmp/zf26-x --ttl 14400                             # heartbeat on long work
$C lease check /tmp/some/folder || echo "leased: do not touch"     # before deleting or editing anything shared
$C status working --step "port the checker"                        # or: blocked --question "...", idle
$C ack <N>                                                         # after reading messages up to N
$C lease release /tmp/zf26-x && $C status done                     # done is refused while you hold leases
```

A lease past its expiry is **stale, not free**. Its holder may be paused rather than finished, and clocks can
jump, so a stale lease still blocks `lease check` and `lease take`. Renew while you work. If you need a stale
lease's resource, message its holder. Take it with `--force-reason "..."` only after the holder or the lead
confirms it has finished; the reason is recorded.

Every report you send states **"read through message N"**, and lists the IDs of the records you added.

## Size and history

The store keeps the newest 20,000 events live. A keeper service runs `$C archive --if-over` every hour. Older
events move to compressed segments under `archive/`, and each move is checked against the hash chain and recorded
in it. Records and unread messages always stay live. `$C storage` shows the sizes. `$C verify --archives` checks
the whole history. `$C export records --format md` and `$C export events --with-archive` write copies. Agents
don't run `archive`; the lead and the keeper do. The keeper also writes the newest event's number and hash to an
anchor file outside the store, so `verify --anchors` can detect a history that was rewritten or cut short.

## What never goes in

No secrets, credentials, private keys, personal data or hidden reasoning. Keep text short and factual, and cite
evidence rather than pasting large outputs.
