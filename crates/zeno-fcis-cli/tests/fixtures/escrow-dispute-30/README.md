# Escrow contract fixture, 30-day dispute window

The rule change of the app-building study of 2026-10-05: the escrow
contract of `../escrow` with its dispute window moved from 14 to 30 days.
Only the `dispute_deadline` variable of `v2/policy.json` differs, so
`zeno-fcis contract diff ../escrow .` classifies the change as a
`rule-change`: the schema, the channels and the laws are unchanged, and the
decisions at the window's edge change.

- `project.zeno`: a copy of `../escrow/project.zeno`.
- `v2/policy.json`: `../escrow/v2/policy.json` with `shipped_at + 1209600`
  replaced by `shipped_at + 2592000`.
- `tests/decision-examples.txt`: `../escrow`'s examples with every example
  at the old window's edge moved to the new one, and a dispute 14 days and
  one second after shipping, which the old rules refuse and these accept.
  Written by an AI agent from the rule text; not an independent oracle.

`tools/check_contract_evolve.py` evolves an escrow application with a
committed history to this contract and upgrades its store.
