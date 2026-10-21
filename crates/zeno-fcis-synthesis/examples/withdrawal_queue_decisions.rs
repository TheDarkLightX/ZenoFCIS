//! Diagnostic decision graph of the withdrawal-queue contract; authority: none.
//! Explores every vault reachable from the contract's genesis under the
//! complete declared command and context domain, through the checked V2
//! Authority of the generated contract, and prints each decision as JSON.
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/withdrawal-queue/src/v2_contract.rs"]
mod vault;

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_synthesis::finite::v2_composition as c;
use zeno_fcis_value::{Field, Value};

#[derive(Clone, Copy)]
enum Leaf {
    Int,
    Bool,
    Sum(u32),
}
const STATE: &[(u16, Leaf)] = &[
    (120, Leaf::Int),
    (121, Leaf::Sum(109)),
    (122, Leaf::Int),
    (123, Leaf::Sum(109)),
    (124, Leaf::Int),
    (125, Leaf::Int),
    (126, Leaf::Bool),
    (127, Leaf::Sum(108)),
];
const COMMAND: &[(u16, Leaf)] = &[
    (130, Leaf::Sum(107)),
    (131, Leaf::Sum(108)),
    (132, Leaf::Int),
];
const CONTEXT: &[(u16, Leaf)] = &[(140, Leaf::Sum(112)), (141, Leaf::Bool)];
// The declared command and context domains: action, lane, amount, caller, alarm.
const ACTIONS: [i128; 3] = [160, 161, 162];
const LANES: [i128; 2] = [170, 171];
const AMOUNTS: [i128; 2] = [1, 2];
const CALLERS: [i128; 4] = [190, 191, 192, 193];
const ALARMS: [i128; 2] = [0, 1];
// Miri interprets two decisions only; the graph then reports itself incomplete.
const MIRI_DECISIONS: usize = 2;

fn value(kind: Leaf, n: i128) -> Value {
    match kind {
        Leaf::Int => Value::signed(n),
        Leaf::Bool => Value::boolean(n == 1),
        Leaf::Sum(type_id) => Value::sum(
            type_id,
            u16::try_from(n).unwrap_or_else(|_| panic!("variant")),
            None,
        ),
    }
}
fn record(fields: &[(u16, Leaf)], numbers: &[i128]) -> Value {
    Value::record_canonical(
        fields
            .iter()
            .zip(numbers)
            .map(|((id, kind), n)| Field::new(*id, value(*kind, *n)))
            .collect(),
    )
    .unwrap_or_else(|_| panic!("record"))
}
fn frame(binding: &c::FrameBinding, v: Value) -> Vec<u8> {
    Envelope::new(binding.root, Hash32::new(binding.schema), v)
        .canonical_bytes()
        .unwrap_or_else(|_| panic!("frame"))
}
fn number(atom: &c::Atom<'_>) -> i128 {
    match atom {
        c::Atom::I128(n) => *n,
        c::Atom::Bool(b) => i128::from(*b),
        c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => i128::from(*variant),
        other => panic!("unexpected atom {other:?}"),
    }
}
fn fields(values: &[c::Field<'_>], expected: &[(u16, Leaf)]) -> Vec<i128> {
    assert_eq!(
        values.iter().map(|f| f.id).collect::<Vec<_>>(),
        expected.iter().map(|(id, _)| *id).collect::<Vec<_>>()
    );
    values.iter().map(|f| number(&f.value)).collect()
}
fn list(values: &[i128]) -> String {
    let items: Vec<String> = values.iter().map(ToString::to_string).collect();
    format!("[{}]", items.join(","))
}

fn main() {
    let contract = vault::Contract::new();
    let descriptor = contract.descriptor();
    let Ok(authority) = vault::checked_authority(&descriptor) else {
        panic!("checked withdrawal-queue authority");
    };
    let framing = &vault::FRAMING;
    let genesis = fields(vault::GENESIS, STATE);
    let commits = authority
        .genesis(&frame(&framing.state, record(STATE, &genesis)))
        .result()
        .is_ok();
    let mut changed = genesis.clone();
    changed[0] += 1;
    let changed_refused = authority
        .genesis(&frame(&framing.state, record(STATE, &changed)))
        .result()
        .is_err();
    let mut index = BTreeMap::from([(genesis.clone(), 0usize)]);
    let mut vaults = vec![genesis];
    let mut queue = VecDeque::from([0usize]);
    let mut decisions = String::new();
    let mut count = 0usize;
    let inputs: Vec<[i128; 5]> = ACTIONS
        .iter()
        .flat_map(|&action| {
            LANES.iter().flat_map(move |&lane| {
                AMOUNTS.iter().flat_map(move |&amount| {
                    CALLERS.iter().flat_map(move |&caller| {
                        ALARMS
                            .iter()
                            .map(move |&alarm| [action, lane, amount, caller, alarm])
                    })
                })
            })
        })
        .collect();
    let mut complete = true;
    'explore: while let Some(from) = queue.pop_front() {
        let state = vaults[from].clone();
        for [action, lane, amount, caller, alarm] in inputs.iter().copied() {
            if cfg!(miri) && count == MIRI_DECISIONS {
                complete = false;
                break 'explore;
            }
            let raw = [
                frame(&framing.state, record(STATE, &state)),
                frame(&framing.command, record(COMMAND, &[action, lane, amount])),
                frame(&framing.context, record(CONTEXT, &[caller, alarm])),
            ];
            let output = authority.evaluate(c::Raw {
                state: &raw[0],
                command: &raw[1],
                context: &raw[2],
            });
            let (class, reason, to, outbox) = match output.result() {
                Err(refusal) => (format!("refused {refusal:?}"), None, None, vec![]),
                Ok(candidate) => {
                    let class = match candidate.class() {
                        c::Class::Accept => "accept",
                        c::Class::Reject => "reject",
                        c::Class::CommittedFailure => "failure",
                        other => panic!("unexpected class {other:?}"),
                    };
                    let to = (class != "reject").then(|| {
                        let post = fields(candidate.post(), STATE);
                        *index.entry(post.clone()).or_insert_with(|| {
                            vaults.push(post);
                            queue.push_back(vaults.len() - 1);
                            vaults.len() - 1
                        })
                    });
                    let outbox = candidate
                        .outbox()
                        .iter()
                        .map(|d| {
                            let mut row = vec![i128::from(d.channel)];
                            row.extend(d.payload.iter().map(|f| number(&f.value)));
                            list(&row)
                        })
                        .collect::<Vec<_>>();
                    (class.to_owned(), candidate.reason(), to, outbox)
                }
            };
            if count > 0 {
                decisions.push(',');
            }
            count += 1;
            let _ = write!(
                decisions,
                "[{from},{action},{lane},{amount},{caller},{alarm},\"{class}\",{},{},[{}]]",
                reason.map_or_else(|| "null".to_owned(), |r| r.to_string()),
                to.map_or_else(|| "null".to_owned(), |t| t.to_string()),
                outbox.join(",")
            );
        }
    }
    let rendered: Vec<String> = vaults.iter().map(|v| list(v)).collect();
    println!(
        "{{\"profile\":\"withdrawal-queue/decision-graph/1\",\"authority\":\"none\",\
\"complete\":{complete},\"genesis_commits\":{commits},\"changed_genesis_refused\":{changed_refused},\
\"domain\":{{\"action\":{},\"lane\":{},\"amount\":{},\"caller\":{},\"alarm\":{}}},\
\"vaults\":[{}],\"decisions\":[{decisions}]}}",
        list(&ACTIONS),
        list(&LANES),
        list(&AMOUNTS),
        list(&CALLERS),
        list(&ALARMS),
        rendered.join(",")
    );
}
