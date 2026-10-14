# Warehouse reservation: one atomic composed core

This closed example joins a reservation pool (`C=4, Q=3`), an idempotency
slot (`C=3`) and a phase machine (`C=2`). Its complete input domain contains
14,400 tuples. Qualification is pending; generated data and stored component
references do not establish the combined guarantee.

Start with four available units and no order. Reserve three for key 1: one unit
remains available, three are held, and the order becomes pending. Repeating that
request changes nothing. The same key with a different quantity refuses. Ship
the matching order: its three held units are consumed and exactly one shipment
intent is recorded. Repeating Ship emits nothing. A second order is refused.

The state invariant connects the components: pending quantity equals held stock;
after shipping, available stock plus the shipped quantity equals four. Delivery
law 503 checks the actual candidate outbox against the consumed stock and stored
order key. The law bound admits two entries so a planted duplicate can reach
that law, which requires at most one. Rejects preserve the entire application.

`composition.json` pins the exact component instances, projections, field owners
and selected occurrence order. Each occurrence's listed step order is
descriptive: every step reads the same pre-state. The table compiles into one
ordinary library Authority descriptor; publication commits into one SQLite
transaction. There are no component publication callbacks. This is one checked
assembly profile, not a general-purpose composition compiler. Component
references remain Identified and need replay. The combined contract needs its
own finite replay, generated native journey, mutation controls and independent
review.

This profile supports one immutable order quantity in 1..3, one key in 0..1 and
capacity four. It provides no cancellation, reset, eviction or memory of many
historical keys. The earlier roadmap's capacity-five journey was a proposal;
the currently qualified reservation family supports capacity four. Supplied
authorization is an assumption; it does not authenticate an operator. A
shipment intent does not prove that goods were delivered.

Generate and inspect the contract using the ordinary workflow:

```sh
zeno-fcis generate contract core-apps/warehouse-reservation
zeno-fcis contract review core-apps/warehouse-reservation --out review.json
zeno-fcis new warehouse-app --contract core-apps/warehouse-reservation --source .
```

`tools/check_core_app.py --app warehouse-reservation --static` checks the
complete independent corpus, and that the contract's rules, read through the
names the composition's views use, reproduce every written decision. On every
accepted input, each stepping component's input and successor, derived from the
composition's views, lie in its instance's domains, and its local reference
accepts that input and yields that successor; components that take no step keep
their state, the owners partition the state, and the declared effect is the
whole outbox. Every accepted input fires at least one declared occurrence, and
every declared occurrence fires on at least one accepted input. Separately, it
checks the composition against its reviewed pin and the component sources
against the catalogue. Its execution mode additionally checks the actual
Authority through `contract review`, the freshly scaffolded app and actual law-violating
candidates. Execution mode builds and tests generated applications.
