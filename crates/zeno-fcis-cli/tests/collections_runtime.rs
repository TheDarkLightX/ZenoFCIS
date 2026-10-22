//! Keyed collections at run time: real keys, the library Authority on the
//! view, and the SQLite store under its Merkle roots.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::collections::BTreeMap;

use zeno_fcis_contract::collections::runtime::{
    Entries, Framed, Input, Key, Layout, RecordValue, decode_record,
};
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, Outcome},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};
use zeno_fcis_shell_sqlite::collections::{CollectionStore, rebuild_root};

const TOKENS: &str = include_str!("../../../examples/collections/token-balances/project.zeno");
const TOKEN_RULES: &str =
    include_str!("../../../examples/collections/token-balances/v2/policy.json");
const LIBRARY: &str = include_str!("../../../examples/collections/library/project.zeno");
const LIBRARY_RULES: &str = include_str!("../../../examples/collections/library/v2/policy.json");

/// Reads the store, counting reads.
struct Reader<'a> {
    store: &'a CollectionStore,
    reads: Cell<u64>,
}

impl Entries for Reader<'_> {
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        self.store.get(map, key).unwrap()
    }
}

/// One step: a command and context, and whether it commits.
struct Step {
    command: Input,
    context: Input,
}

struct Outcomes {
    committed: u64,
    rejected: u64,
    most_reads: u64,
}

/// A check of the store and the scalar state after a committed decision.
type Check<'a> = dyn FnMut(&CollectionStore, &BTreeMap<u16, i128>, &Framed) + 'a;

/// Runs steps through the Authority and the store, calling `check` after
/// every committed decision with the store and the scalar state.
fn run(
    project: &str,
    rules_text: &str,
    store: &mut CollectionStore,
    state: &mut BTreeMap<u16, i128>,
    steps: &[Step],
    check: &mut Check<'_>,
) -> Outcomes {
    let layout = Layout::read(project, rules_text).unwrap();
    let rules = Rules::read(rules_text).unwrap();
    let declarations = Declarations::read(project, &rules.leaf_bindings).unwrap();
    let positions = domain::positions(&declarations).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        let mut outcomes = Outcomes {
            committed: 0,
            rejected: 0,
            most_reads: 0,
        };
        for step in steps {
            let reader = Reader {
                store,
                reads: Cell::new(0),
            };
            let framed = layout
                .frame(state, &step.command, &step.context, &reader)
                .unwrap();
            outcomes.most_reads = outcomes.most_reads.max(reader.reads.get());
            let decision = match evaluate::evaluate(authority, &framer, &framed.tuple) {
                Outcome::Decision(decision) => decision,
                Outcome::Refused(refusal) => panic!("refused {refusal:?} at {:?}", framed.tuple),
            };
            if decision.class == Class::Reject {
                outcomes.rejected += 1;
                continue;
            }
            let post: BTreeMap<u16, i128> = decision
                .post
                .iter()
                .map(|(id, v)| (*id, v.number().unwrap()))
                .collect();
            let writes = layout.writes(&framed, &post).unwrap();
            let bytes = format!("{:?}{:?}", framed.tuple, post).into_bytes();
            let scalars = format!("{:?}", writes.scalars).into_bytes();
            store
                .commit(&framed.named, &writes.entries, &bytes, &scalars)
                .unwrap();
            *state = writes.scalars;
            outcomes.committed += 1;
            check(store, state, &framed);
        }
        outcomes
    })
    .unwrap()
}

struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn key(&mut self) -> Key {
        let mut key = [0u8; 32];
        for chunk in key.chunks_mut(8) {
            chunk.copy_from_slice(&self.next().to_be_bytes());
        }
        key
    }
}

fn token_steps(accounts: &[Key], stream: &mut Stream, count: usize) -> Vec<Step> {
    let mut steps = Vec::new();
    for i in 0..count {
        // Senders come from a few funded accounts, receivers from all, so
        // transfers keep opening new accounts.
        let from = accounts[(stream.next() % 4) as usize];
        let to = if i.is_multiple_of(9) {
            from
        } else {
            accounts[(stream.next() % accounts.len() as u64) as usize]
        };
        let mint = i < 4 || stream.next().is_multiple_of(10);
        let (from, to) = if i < 4 {
            (accounts[i], accounts[i])
        } else {
            (from, to)
        };
        steps.push(Step {
            command: Input {
                scalars: BTreeMap::from([
                    (130, if mint { 150 } else { 151 }),
                    (133, i128::from((stream.next() % 4) as u8)),
                ]),
                keys: BTreeMap::from([(131, from), (132, to)]),
            },
            context: Input {
                scalars: BTreeMap::from([(0, if mint { 180 } else { 181 })]),
                keys: BTreeMap::new(),
            },
        });
    }
    steps
}

fn balance(record: &[u8]) -> i128 {
    match decode_record(record).unwrap()[0] {
        RecordValue::Number(n) => n,
        RecordValue::Key(_) => panic!("balance is a number"),
    }
}

#[test]
fn collections_runtime_keeps_the_supply_equal_to_every_balance() {
    let mut stream = Stream(0x00c0_ffee);
    let accounts: Vec<Key> = (0..400).map(|_| stream.key()).collect();
    let steps = token_steps(&accounts, &mut stream, 1500);
    let mut store = CollectionStore::in_memory().unwrap();
    let mut state = BTreeMap::from([(120u16, 0i128)]);
    let outcomes = run(
        TOKENS,
        TOKEN_RULES,
        &mut store,
        &mut state,
        &steps,
        &mut |store, state, framed| {
            // Every named key is at most the two the command holds.
            assert!(framed.named[&110].len() <= 2);
            let entries = store.entries(110).unwrap();
            let total: i128 = entries.iter().map(|(_, r)| balance(r)).sum();
            assert_eq!(state[&120], total, "the supply is the sum of every balance");
            assert_eq!(store.root(110).unwrap(), rebuild_root(&entries));
        },
    );
    // Reads stay at the two named entries while the map grows to hundreds.
    assert_eq!(outcomes.most_reads, 2);
    assert!(store.len(110).unwrap() > 150, "{}", store.len(110).unwrap());
    let counts = (outcomes.committed, outcomes.rejected);
    assert!(counts.0 > 300 && counts.1 > 0, "{counts:?}");
    // Replaying the same commands into a fresh store gives the same journal.
    let mut again = CollectionStore::in_memory().unwrap();
    let mut state = BTreeMap::from([(120u16, 0i128)]);
    run(
        TOKENS,
        TOKEN_RULES,
        &mut again,
        &mut state,
        &steps,
        &mut |_, _, _| {},
    );
    let first: Vec<_> = store
        .commits()
        .unwrap()
        .into_iter()
        .map(|c| c.chain)
        .collect();
    let second: Vec<_> = again
        .commits()
        .unwrap()
        .into_iter()
        .map(|c| c.chain)
        .collect();
    assert_eq!(first, second);
}

#[test]
fn collections_runtime_records_holders_as_keys() {
    let mut stream = Stream(0x1b2a);
    let members: Vec<Key> = (0..60).map(|_| stream.key()).collect();
    let copies: Vec<Key> = (0..40).map(|_| stream.key()).collect();
    let mut steps = Vec::new();
    let step = |action: i128, member: Key, copy: Key| Step {
        command: Input {
            scalars: BTreeMap::from([(130, action)]),
            keys: BTreeMap::from([(131, member), (132, copy)]),
        },
        context: Input {
            scalars: BTreeMap::from([(0, 180)]),
            keys: BTreeMap::new(),
        },
    };
    for member in &members {
        steps.push(step(150, *member, copies[0]));
    }
    for copy in &copies {
        steps.push(step(151, members[0], *copy));
    }
    for _ in 0..2000 {
        let member = members[(stream.next() % 60) as usize];
        let copy = copies[(stream.next() % 40) as usize];
        steps.push(step(152 + (stream.next() % 2) as i128, member, copy));
    }
    let mut store = CollectionStore::in_memory().unwrap();
    let mut state = BTreeMap::new();
    let outcomes = run(
        LIBRARY,
        LIBRARY_RULES,
        &mut store,
        &mut state,
        &steps,
        &mut |store, _, _| {
            // Across the two maps: each member's count is the copies it holds.
            let held: BTreeMap<Key, i128> = store
                .entries(115)
                .unwrap()
                .iter()
                .filter_map(|(_, record)| match decode_record(record).unwrap()[..] {
                    [RecordValue::Number(1), RecordValue::Key(holder)] => Some(holder),
                    _ => None,
                })
                .fold(BTreeMap::new(), |mut counts, holder| {
                    *counts.entry(holder).or_default() += 1;
                    counts
                });
            for (member, record) in store.entries(110).unwrap() {
                let loans = balance(&record);
                assert_eq!(loans, held.get(&member).copied().unwrap_or(0));
                assert!(loans <= 2);
            }
        },
    );
    assert_eq!(store.len(110).unwrap(), 60);
    assert_eq!(store.len(115).unwrap(), 40);
    let counts = (outcomes.committed, outcomes.rejected);
    assert!(counts.0 > 150 && counts.1 > 150, "{counts:?}");
}
