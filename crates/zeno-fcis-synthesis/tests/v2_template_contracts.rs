//! Independent complete-decision checks against the original applications.
//! Numeric reference rules and retained owner examples do not call the emitter,
//! its expression trees, or the generated scalar/law programs.
#![allow(clippy::too_many_lines, clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/account-lockout/src/v2_contract.rs"]
mod account;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
mod counter;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/compliance-gateway/src/v2_contract.rs"]
mod gateway;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/order-fulfillment/src/v2_contract.rs"]
mod order;
#[allow(dead_code, unreachable_pub)]
#[path = "../../../verification/kernel-laws/src/oracle/templates/withdrawal-queue/original/src/controller.rs"]
mod original_controller;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/prepared-counter/src/v2_contract.rs"]
mod prepared;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/inventory-reservation/src/v2_contract.rs"]
mod stock;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/agent-treasury-guard/src/v2_contract.rs"]
mod treasury;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/withdrawal-queue/src/v2_contract.rs"]
mod vault;

use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_synthesis::finite::{
    V2Resource as Resource, v2_authority as a, v2_catalog as catalog, v2_composition as c,
    v2_laws as l,
};
use zeno_fcis_value::{Field, Value};

#[derive(Clone, Copy)]
enum Leaf {
    Int,
    Bool,
    Sum(u32),
}
use Leaf::{Bool, Int, Sum};
const COUNTER_STATE: &[(u16, Leaf)] = &[(110, Int), (111, Int)];
const STOCK_COMMAND: &[(u16, Leaf)] = &[(120, Sum(107)), (121, Int)];
const STOCK_CONTEXT: &[(u16, Leaf)] = &[(130, Bool)];
const ORDER_STATE: &[(u16, Leaf)] = &[(120, Sum(105)), (121, Int)];
const ORDER_COMMAND: &[(u16, Leaf)] = &[(125, Sum(111)), (126, Int)];
const ORDER_CONTEXT: &[(u16, Leaf)] = &[(130, Sum(107))];
const ACCOUNT_STATE: &[(u16, Leaf)] = &[(110, Int), (111, Int), (112, Int)];
const ACCOUNT_CONTEXT: &[(u16, Leaf)] = &[(130, Int), (131, Bool)];
const VAULT_STATE: &[(u16, Leaf)] = &[
    (120, Int),
    (121, Sum(109)),
    (122, Int),
    (123, Sum(109)),
    (124, Int),
    (125, Int),
    (126, Bool),
    (127, Sum(108)),
];
const VAULT_COMMAND: &[(u16, Leaf)] = &[(130, Sum(107)), (131, Sum(108)), (132, Int)];
const VAULT_CONTEXT: &[(u16, Leaf)] = &[(140, Sum(112)), (141, Bool)];
const TREASURY_STATE: &[(u16, Leaf)] = &[
    (130, Int),
    (131, Int),
    (132, Int),
    (133, Int),
    (134, Sum(109)),
    (135, Int),
    (136, Int),
];
const TREASURY_COMMAND: &[(u16, Leaf)] = &[
    (140, Sum(115)),
    (141, Sum(110)),
    (142, Int),
    (143, Int),
    (144, Int),
    (145, Int),
];
const TREASURY_CONTEXT: &[(u16, Leaf)] = &[
    (150, Sum(116)),
    (151, Int),
    (152, Int),
    (153, Int),
    (154, Sum(118)),
];

struct App {
    name: &'static str,
    state: &'static [(u16, Leaf)],
    command: &'static [(u16, Leaf)],
    context: &'static [(u16, Leaf)],
    examples: &'static str,
    genesis: &'static [i64],
}
const APPS: [App; 6] = [
    App {
        name: "durable-counter",
        state: COUNTER_STATE,
        command: &[],
        context: &[],
        examples: include_str!(
            "../../zeno-fcis-cli/templates/durable-counter/tests/decision-examples.txt"
        ),
        genesis: &[0, 0],
    },
    App {
        name: "inventory-reservation",
        state: COUNTER_STATE,
        command: STOCK_COMMAND,
        context: STOCK_CONTEXT,
        examples: include_str!(
            "../../zeno-fcis-cli/templates/inventory-reservation/tests/decision-examples.txt"
        ),
        genesis: &[0, 0],
    },
    App {
        name: "order-fulfillment",
        state: ORDER_STATE,
        command: ORDER_COMMAND,
        context: ORDER_CONTEXT,
        examples: include_str!(
            "../../zeno-fcis-cli/templates/order-fulfillment/tests/decision-examples.txt"
        ),
        genesis: &[160, 0],
    },
    App {
        name: "account-lockout",
        state: ACCOUNT_STATE,
        command: &[],
        context: ACCOUNT_CONTEXT,
        examples: include_str!(
            "../../zeno-fcis-cli/templates/account-lockout/tests/decision-examples.txt"
        ),
        genesis: &[0, 0, 0],
    },
    App {
        name: "withdrawal-queue",
        state: VAULT_STATE,
        command: VAULT_COMMAND,
        context: VAULT_CONTEXT,
        examples: include_str!(
            "../../zeno-fcis-cli/templates/withdrawal-queue/tests/decision-examples.txt"
        ),
        genesis: &[0, 180, 0, 180, 0, 0, 0, 170],
    },
    App {
        name: "agent-treasury-guard",
        state: TREASURY_STATE,
        command: TREASURY_COMMAND,
        context: TREASURY_CONTEXT,
        examples: include_str!(
            "../../zeno-fcis-cli/templates/agent-treasury-guard/tests/decision-examples.txt"
        ),
        genesis: &[6, 1, 0, 0, 170, 0, 0],
    },
];
fn value(kind: Leaf, n: i64) -> Value {
    match kind {
        Int => Value::signed(i128::from(n)),
        Bool => {
            assert!(n == 0 || n == 1);
            Value::boolean(n == 1)
        }
        Sum(type_id) => Value::sum(type_id, u16::try_from(n).unwrap(), None),
    }
}
fn atom(kind: Leaf, n: i64) -> c::Atom<'static> {
    match kind {
        Int => c::Atom::I128(i128::from(n)),
        Bool => c::Atom::Bool(n != 0),
        Sum(type_id) => c::Atom::Sum {
            type_id,
            variant: u16::try_from(n).unwrap(),
        },
    }
}
fn record(fields: &[(u16, Leaf)], numbers: &[i64]) -> Value {
    assert_eq!(fields.len(), numbers.len());
    Value::record_canonical(
        fields
            .iter()
            .zip(numbers)
            .map(|((id, kind), n)| Field::new(*id, value(*kind, *n)))
            .collect(),
    )
    .unwrap()
}
fn frame(binding: &c::FrameBinding, v: Value) -> Vec<u8> {
    Envelope::new(binding.root, Hash32::new(binding.schema), v)
        .canonical_bytes()
        .unwrap()
}
impl App {
    fn wire(&self, f: &c::Framing, s: &[i64], cmd: &[i64], ctx: &[i64]) -> [Vec<u8>; 3] {
        [
            frame(&f.state, record(self.state, s)),
            frame(
                &f.command,
                if self.command.is_empty() {
                    value(Sum(101), cmd[0])
                } else {
                    record(self.command, cmd)
                },
            ),
            frame(
                &f.context,
                if self.context.is_empty() {
                    value(Bool, ctx[0])
                } else {
                    record(self.context, ctx)
                },
            ),
        ]
    }
    fn split<'n>(&self, all: &'n [i64]) -> (&'n [i64], &'n [i64], &'n [i64]) {
        let (s, rest) = all.split_at(self.state.len());
        let (c, x) = rest.split_at(self.command.len().max(1));
        assert_eq!(x.len(), self.context.len().max(1));
        (s, c, x)
    }
}
#[derive(Debug)]
struct Expected {
    class: c::Class,
    reason: Option<u32>,
    post: Vec<i64>,
    outbox: Vec<(u32, Vec<i64>)>,
    refusal: Option<&'static str>,
}
fn reject(reason: u32) -> Expected {
    Expected {
        class: c::Class::Reject,
        reason: Some(reason),
        post: vec![],
        outbox: vec![],
        refusal: None,
    }
}
fn accept(s: &[i64]) -> Expected {
    Expected {
        class: c::Class::Accept,
        reason: None,
        post: s.to_vec(),
        outbox: vec![],
        refusal: None,
    }
}
fn payload(channel: u32, app: &str) -> (&'static str, &'static [(u16, Leaf)]) {
    match (app, channel) {
        ("durable-counter", 300) => ("local-observer", &[(112, Int), (113, Int)]),
        ("inventory-reservation", 300) => ("warehouse", &[(140, Int)]),
        ("order-fulfillment", 300) => ("payment-provider", &[(140, Int), (141, Sum(108))]),
        ("order-fulfillment", 301) => ("carrier", &[(145, Int)]),
        ("account-lockout", 300) => ("security-team", &[(140, Sum(109)), (141, Int)]),
        ("withdrawal-queue", 300) => ("settlement", &[(150, Sum(108)), (151, Int)]),
        ("agent-treasury-guard", 300) => (
            "zenodex",
            &[
                (160, Int),
                (161, Sum(120)),
                (162, Sum(120)),
                (163, Int),
                (164, Int),
                (165, Int),
            ],
        ),
        _ => panic!("unexpected channel"),
    }
}
fn reference(app: usize, s: &[i64], cmd: &[i64], ctx: &[i64]) -> Expected {
    let mut out = accept(s);
    match app {
        0 => {
            if ctx[0] == 0 {
                return reject(200);
            }
            let index = usize::try_from(cmd[0] - 120).unwrap();
            if s[index] == 3 {
                return reject(201);
            }
            out.post[index] += 1;
            if index == 1 {
                out.class = c::Class::CommittedFailure;
                out.reason = Some(202)
            }
            out.outbox.push((300, out.post.clone()));
        }
        1 => {
            let (action, q) = (cmd[0], cmd[1]);
            if ctx[0] == 0 {
                return reject(200);
            }
            if action == 150 && s[0] < q {
                return reject(201);
            }
            if (action == 151 || action == 152) && s[1] < q {
                return reject(202);
            }
            if (action == 150 && s[1] + q > 5) || ((action == 151 || action == 153) && s[0] + q > 5)
            {
                return reject(203);
            }
            match action {
                150 => {
                    out.post[0] -= q;
                    out.post[1] += q
                }
                151 => {
                    out.post[0] += q;
                    out.post[1] -= q
                }
                152 => {
                    out.post[1] -= q;
                    out.outbox.push((300, vec![q]));
                }
                153 => out.post[0] += q,
                _ => unreachable!(),
            }
        }
        2 => {
            let (status, n, action, callback, caller) = (s[0], s[1], cmd[0], cmd[1], ctx[0]);
            let sender = match action {
                150 | 155 => 170,
                151 | 152 => 171,
                153 | 154 => 172,
                _ => unreachable!(),
            };
            if sender != caller {
                return reject(200);
            }
            let legal = match action {
                150 => status == 160,
                151 | 152 => status == 161,
                153 => status == 162,
                154 => status == 163,
                155 => status == 160 || status == 161,
                _ => false,
            };
            if !legal {
                return reject(201);
            }
            if (action == 151 || action == 152) && callback != n {
                return reject(202);
            }
            if action == 150 && n == 3 {
                return reject(203);
            }
            match action {
                150 => {
                    out.post = vec![161, n + 1];
                    out.outbox.push((300, vec![n + 1, 175]));
                }
                151 => {
                    out.post[0] = 162;
                    out.outbox.push((301, vec![n]));
                }
                152 => {
                    out.post[0] = 160;
                    out.class = c::Class::CommittedFailure;
                    out.reason = Some(204);
                }
                153 => out.post[0] = 163,
                154 => out.post[0] = 164,
                155 => {
                    out.post[0] = 165;
                    if status == 161 {
                        out.outbox.push((300, vec![n, 176]));
                    }
                }
                _ => unreachable!(),
            }
            if (161..=164).contains(&out.post[0]) && out.post[1] == 0 {
                out.refusal = Some("law500")
            }
        }
        3 => {
            let (failed, until, seen, action, now, admin) =
                (s[0], s[1], s[2], cmd[0], ctx[0], ctx[1]);
            if now < seen {
                return reject(200);
            }
            if action != 122 && now < until {
                return reject(201);
            }
            if action == 122 && admin == 0 {
                return reject(202);
            }
            out.post[2] = now;
            match action {
                120 => out.post[0] = 0,
                121 => {
                    out.class = c::Class::CommittedFailure;
                    out.reason = Some(203);
                    if failed == 2 {
                        out.post[0] = 0;
                        out.post[1] = now + 900;
                        out.outbox.push((300, vec![150, now + 900]));
                    } else {
                        out.post[0] += 1;
                    }
                }
                122 => {
                    out.post[0] = 0;
                    out.post[1] = 0;
                    out.outbox.push((300, vec![151, 0]));
                }
                _ => unreachable!(),
            }
            if out.post[1] > out.post[2] + 900 || (out.post[1] > out.post[2] && out.post[0] != 0) {
                out.refusal = Some("law500")
            }
        }
        4 => {
            let (action, lane, amount, caller, alarm) =
                (cmd[0], cmd[1], cmd[2], ctx[0], ctx[1] != 0);
            let sender = match action {
                160 => 190,
                161 => {
                    if lane == 170 {
                        191
                    } else {
                        192
                    }
                }
                162 => 193,
                _ => unreachable!(),
            };
            if sender != caller {
                return reject(200);
            }
            let pos = if lane == 170 { 1 } else { 3 };
            if action == 161 && s[pos] != 180 {
                return reject(201);
            }
            if action == 161 && amount > s[0] - s[2] - s[4] {
                return reject(202);
            }
            if action == 160 && s[0] + amount > 4 {
                return reject(203);
            }
            match action {
                160 => out.post[0] += amount,
                161 => {
                    out.post[pos] = 181;
                    out.post[pos + 1] = amount;
                }
                162 => {
                    // Independent retained certified table, not the new scalar policy.
                    let plant = original_controller::plant_index(
                        s[1] == 182,
                        s[3] == 182,
                        u8::try_from(s[5]).unwrap(),
                        s[6] != 0,
                    );
                    let symbol = original_controller::input_index(s[1] == 181, s[3] == 181, alarm);
                    let memory = original_controller::memory_index(plant, s[7] == 171);
                    let [pay, next] = original_controller::row(memory, symbol).unwrap();
                    assert_eq!(
                        original_controller::successor(plant, symbol, pay),
                        Some(next / 2)
                    );
                    let plant = next / 2;
                    let must = plant % 2;
                    let pause = (plant / 2) % 3;
                    let pending_b = (plant / 6) % 2;
                    let pending_a = (plant / 12) % 2;
                    out.post[1] = if pending_a != 0 { 182 } else { 180 };
                    out.post[3] = if pending_b != 0 { 182 } else { 180 };
                    out.post[5] = i64::from(pause);
                    out.post[6] = i64::from(must);
                    out.post[7] = 170 + i64::from(next % 2);
                    if pay != 0 {
                        let pos = if pay == 1 { 1 } else { 3 };
                        let paid = s[pos + 1];
                        out.post[0] -= paid;
                        out.post[pos + 1] = 0;
                        out.outbox.push((300, vec![169 + i64::from(pay), paid]));
                        if !(1..=2).contains(&paid) {
                            out.refusal = Some("schema")
                        }
                    }
                }
                _ => unreachable!(),
            }
            if out.post[0] < 0 {
                out.refusal = Some("schema")
            }
            if out.refusal.is_none() && !vault_invariant(&out.post) {
                out.refusal = Some("law500")
            }
        }
        5 => {
            let (action, direction, amount, min_out, intent, amount_out) =
                (cmd[0], cmd[1], cmd[2], cmd[3], cmd[4], cmd[5]);
            let (caller, now, price, price_time, model) = (ctx[0], ctx[1], ctx[2], ctx[3], ctx[4]);
            let proposes = action == 175;
            let buys = direction == 173;
            if caller != if proposes { 178 } else { 179 } {
                return reject(200);
            }
            if now <= s[3] {
                return reject(201);
            }
            if proposes && model != 180 {
                return reject(202);
            }
            if proposes && (price_time > now || now - price_time > 1) {
                return reject(203);
            }
            if proposes && s[4] != 170 {
                return reject(204);
            }
            if !proposes && s[4] == 170 {
                return reject(205);
            }
            if !proposes && intent != s[3] {
                return reject(206);
            }
            if action == 176 && amount_out < s[6] {
                return reject(207);
            }
            let spent = if now / 4 > s[3] / 4 { 0 } else { s[2] };
            let value = if buys { amount } else { amount * price };
            if proposes && value > 3 {
                return reject(208);
            }
            if proposes && spent + value > 4 {
                return reject(209);
            }
            let numerator = if buys { amount * 3 } else { amount * price * 3 };
            let denominator = if buys { 4 * price } else { 4 };
            let least = (numerator + denominator - 1) / denominator;
            if proposes && min_out < least {
                return reject(210);
            }
            if proposes
                && if buys {
                    s[0] - amount < 2
                } else {
                    s[1] < amount
                }
            {
                return reject(211);
            }
            out.post[3] = now;
            out.post[2] = spent;
            if proposes {
                out.post[usize::from(!buys)] -= amount;
                out.post[2] += value;
                out.post[4] = if buys { 171 } else { 172 };
                out.post[5] = amount;
                out.post[6] = min_out;
                out.outbox.push((
                    300,
                    vec![
                        now,
                        if buys { 183 } else { 184 },
                        if buys { 184 } else { 183 },
                        amount,
                        min_out,
                        now + 2,
                    ],
                ));
            } else {
                let buy = s[4] == 171;
                if action == 176 {
                    out.post[usize::from(buy)] += amount_out;
                } else {
                    out.post[usize::from(!buy)] += s[5];
                    out.class = c::Class::CommittedFailure;
                    out.reason = Some(212);
                }
                out.post[4] = 170;
                out.post[5] = 0;
                out.post[6] = 0;
            }
            if out.post[0] > 20 || out.post[1] > 20 {
                out.refusal = Some("schema")
            }
            if out.refusal.is_none()
                && (out.post[0] < 2
                    || out.post[1] < 0
                    || out.post[2] < 0
                    || out.post[2] > 4
                    || (out.post[4] == 170 && (out.post[5] != 0 || out.post[6] != 0))
                    || (out.post[4] != 170 && out.post[5] < 1))
            {
                out.refusal = Some("law500")
            }
        }
        _ => unreachable!(),
    }
    out
}
fn vault_invariant(s: &[i64]) -> bool {
    s[0] <= 4
        && s[0] >= s[2] + s[4]
        && if s[1] == 180 { s[2] == 0 } else { s[2] >= 1 }
        && if s[3] == 180 { s[4] == 0 } else { s[4] >= 1 }
        && (s[6] == 0 || (s[5] == 0 && (s[1] == 182 || s[3] == 182)))
}
fn check(app: &App, out: &a::Evaluation<'_>, s: &[i64], expected: &Expected) {
    check_report(
        app,
        out.result(),
        out.raw(),
        out.usage(),
        out.reads(),
        out.diagnostics(),
        s,
        expected,
    );
    assert_eq!(out.subject().is_ok(), expected.refusal.is_none());
}
#[allow(clippy::too_many_arguments)]
fn check_report(
    app: &App,
    result: Result<&c::Candidate<'_>, a::Refusal>,
    raw: c::Raw<'_>,
    usage: zeno_fcis_synthesis::finite::V2Usage,
    reads: &[c::ReadAttempt],
    diagnostics: &[l::Diagnostic],
    s: &[i64],
    expected: &Expected,
) {
    if let Some(refusal) = expected.refusal {
        let error = result.unwrap_err();
        match refusal {
            "law500" => assert!(
                matches!(error, a::Refusal::Core(c::Failure::Law(_))),
                "{error:?}; {expected:?}"
            ),
            "schema" => assert!(
                matches!(
                    error,
                    a::Refusal::Core(c::Failure::Schema | c::Failure::Decision(_))
                ),
                "{error:?}; {expected:?}"
            ),
            _ => unreachable!(),
        }
        return;
    }
    let result = result.unwrap_or_else(|e| {
        panic!(
            "{}: {e:?}; {expected:?}; pre={s:?}; diagnostics={:?}; usage={:?}",
            app.name, diagnostics, usage
        )
    });
    let expected_reads: Vec<_> = [(0_u8, app.state), (1, app.command), (2, app.context)]
        .into_iter()
        .flat_map(|(source, fields)| {
            if fields.is_empty() {
                vec![(source, c::Selector::Root, true)]
            } else {
                fields
                    .iter()
                    .map(|(id, _)| (source, c::Selector::Field(*id), true))
                    .collect()
            }
        })
        .collect();
    assert_eq!(
        reads
            .iter()
            .map(|r| (r.source, r.selector, r.permitted))
            .collect::<Vec<_>>(),
        expected_reads
    );
    assert_eq!(
        usage.used(Resource::Byte),
        (raw.state.len() + raw.command.len() + raw.context.len()) as u64
    );
    assert_eq!(usage.used(Resource::Candidate), 1);
    assert_eq!(
        usage.used(Resource::Write),
        if expected.class == c::Class::Reject {
            0
        } else {
            app.state.len() as u64
        }
    );
    assert_eq!(usage.used(Resource::Effect), expected.outbox.len() as u64);
    assert_eq!(usage.used(Resource::WitnessByte), 0);
    assert_eq!(usage.used(Resource::Depth), 0);
    assert_eq!(
        result.pre(),
        app.state
            .iter()
            .zip(s)
            .map(|((id, kind), n)| c::Field {
                id: *id,
                value: atom(*kind, *n)
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(result.class(), expected.class);
    assert_eq!(result.reason(), expected.reason);
    let post: Vec<_> = app
        .state
        .iter()
        .zip(&expected.post)
        .map(|((id, k), n)| c::Field {
            id: *id,
            value: atom(*k, *n),
        })
        .collect();
    assert_eq!(result.post(), post);
    assert!(result.effects().is_empty());
    let patch: Vec<_> = app
        .state
        .iter()
        .zip(s)
        .zip(&expected.post)
        .filter_map(|(((id, k), before), after)| {
            if before == after {
                None
            } else {
                Some((*id, atom(*k, *before), atom(*k, *after)))
            }
        })
        .collect();
    assert_eq!(
        result
            .patch()
            .iter()
            .map(|p| (p.field, p.before, p.after))
            .collect::<Vec<_>>(),
        patch
    );
    assert_eq!(result.outbox().len(), expected.outbox.len());
    for (ordinal, (got, (channel, values))) in
        result.outbox().iter().zip(&expected.outbox).enumerate()
    {
        let (destination, fields) = payload(*channel, app.name);
        assert_eq!(got.ordinal, ordinal as u32);
        assert_eq!(got.channel, *channel);
        assert_eq!(got.destination, c::Atom::Text(destination.as_bytes()));
        assert_eq!(got.idempotency, c::Atom::U128(ordinal as u128));
        assert_eq!(
            got.payload,
            fields
                .iter()
                .zip(values)
                .map(|((id, k), n)| c::Field {
                    id: *id,
                    value: atom(*k, *n)
                })
                .collect::<Vec<_>>()
        );
    }
    assert!(
        diagnostics
            .iter()
            .all(|d| matches!(d.verdict, l::Verdict::Skipped | l::Verdict::Satisfied))
    );
}
/// Finite signature of an observed checked outcome: its class and reason, or the
/// actual refusal variant. It is computed only after the independent comparison.
fn outcome(result: Result<&c::Candidate<'_>, a::Refusal>) -> String {
    match result {
        Ok(candidate) => {
            let class = match candidate.class() {
                c::Class::Accept => "accept",
                c::Class::Reject => "reject",
                c::Class::CommittedFailure => "failure",
                other => panic!("unexpected class {other:?}"),
            };
            candidate
                .reason()
                .map_or_else(|| class.to_owned(), |reason| format!("{class} {reason}"))
        }
        Err(refusal) => format!("refused {refusal:?}"),
    }
}
fn exercise(
    index: usize,
    authority: &a::Authority<'_>,
    framing: &c::Framing,
    all: &[i64],
) -> String {
    let app = &APPS[index];
    let (s, cmd, ctx) = app.split(all);
    let expected = reference(index, s, cmd, ctx);
    let raw = app.wire(framing, s, cmd, ctx);
    let input = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    if index < 4 {
        let output = authority.evaluate(input);
        check(app, &output, s, &expected);
        outcome(output.result())
    } else {
        // The checked Authority's very same immutable descriptor and protected
        // core are used for the million-tuple comparisons. Re-serializing its
        // complete embedded library sources on every tuple would add terabytes
        // of redundant seal work. Every retained example separately exercises
        // the production Authority and exact whole-subject replay.
        let core = c::bind(authority.descriptor()).unwrap();
        let output = core.frame(c::Kind::Transition, input, framing);
        let result = output.result().map_err(a::Refusal::Core);
        check_report(
            app,
            result,
            output.raw(),
            output.usage(),
            output.reads(),
            output.diagnostics(),
            s,
            &expected,
        );
        outcome(result)
    }
}
fn examples(index: usize, authority: &a::Authority<'_>, framing: &c::Framing) -> usize {
    let app = &APPS[index];
    let mut count = 0;
    for line in app
        .examples
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let sections: Vec<_> = line.split('|').map(str::trim).collect();
        let split = sections.len() - 2;
        let all: Vec<i64> = sections[..split]
            .join(" ")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let (s, cmd, ctx) = app.split(&all);
        let words: Vec<_> = sections[split].split_whitespace().collect();
        let class = match words[0] {
            "accept" => c::Class::Accept,
            "reject" => c::Class::Reject,
            "failure" => c::Class::CommittedFailure,
            _ => panic!("class"),
        };
        let post: Vec<i64> = words[2..].iter().map(|n| n.parse().unwrap()).collect();
        if class == c::Class::Reject {
            assert_eq!(post, s)
        }
        let mut deliveries = vec![];
        if sections[split + 1] != "-" {
            let mut nums: Vec<i64> = sections[split + 1]
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let channel = if index == 5 {
                300
            } else {
                nums.remove(0) as u32
            };
            deliveries.push((channel, nums));
        }
        let expected = Expected {
            class,
            reason: words[1].parse().ok(),
            post: if class == c::Class::Reject {
                vec![]
            } else {
                post
            },
            outbox: deliveries,
            refusal: None,
        };
        let raw = app.wire(framing, s, cmd, ctx);
        let r = c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        };
        let out = authority.evaluate(r);
        check(app, &out, s, &expected);
        let replay = authority.replay(r, out.subject().unwrap());
        check(app, &replay, s, &expected);
        assert_eq!(out.subject(), replay.subject());
        exercise(index, authority, framing, &all);
        count += 1;
    }
    count
}

/// One finite native corpus, named by its logical loop dimensions. Each corpus is
/// the complete Cartesian product of its pinned dimension values, so a case whose
/// every value is pinned is an actual native case. `input` alone places the
/// logical values into raw fields, so correlated fields stay correlated.
struct Corpus {
    name: &'static str,
    app: usize,
    size: usize,
    dimensions: &'static [(&'static str, &'static [i64])],
    input: fn(&[i64]) -> Vec<i64>,
    /// Every observed outcome signature of the complete native corpus.
    outcomes: &'static [&'static str],
    /// Named boundary relations that both the native corpus and the bounded
    /// profile must witness.
    boundaries: &'static [Boundary],
    /// The bounded Miri cases, strictly ascending.
    profile: &'static [&'static [i64]],
}
/// A named relation over a case's logical values.
type Boundary = (&'static str, fn(&[i64]) -> bool);

/// Counts, observed values, outcome signatures and boundary witnesses of the
/// cases exercised from one corpus. Only the first witness of each feature is
/// retained, never the corpus.
struct Observation<'c> {
    corpus: &'c Corpus,
    cases: usize,
    values: Vec<BTreeSet<i64>>,
    outcomes: BTreeMap<String, Vec<i64>>,
    boundaries: Vec<Option<Vec<i64>>>,
}
impl<'c> Observation<'c> {
    fn new(corpus: &'c Corpus) -> Self {
        let size: usize = corpus
            .dimensions
            .iter()
            .map(|(_, values)| values.len())
            .product();
        assert_eq!(size, corpus.size, "{}: native corpus size", corpus.name);
        Self {
            corpus,
            cases: 0,
            values: vec![BTreeSet::new(); corpus.dimensions.len()],
            outcomes: BTreeMap::new(),
            boundaries: vec![None; corpus.boundaries.len()],
        }
    }
    /// Exercises one case through the checked path; true for a technical refusal.
    fn exercise(
        &mut self,
        authority: &a::Authority<'_>,
        framing: &c::Framing,
        case: &[i64],
    ) -> bool {
        let corpus = self.corpus;
        assert_eq!(
            case.len(),
            corpus.dimensions.len(),
            "{}: case width",
            corpus.name
        );
        let outcome = exercise(corpus.app, authority, framing, &(corpus.input)(case));
        let refused = outcome.starts_with("refused ");
        self.cases += 1;
        for (values, value) in self.values.iter_mut().zip(case) {
            values.insert(*value);
        }
        self.outcomes
            .entry(outcome)
            .or_insert_with(|| case.to_vec());
        for (witness, (_, holds)) in self.boundaries.iter_mut().zip(corpus.boundaries) {
            if witness.is_none() && holds(case) {
                *witness = Some(case.to_vec());
            }
        }
        refused
    }
    fn report(&self) {
        let name = self.corpus.name;
        println!("observed {name}: cases={}", self.cases);
        for ((dimension, _), values) in self.corpus.dimensions.iter().zip(&self.values) {
            println!("observed {name}: values {dimension} {values:?}");
        }
        for (outcome, witness) in &self.outcomes {
            println!("observed {name}: outcome {outcome:?} first {witness:?}");
        }
        for ((boundary, _), witness) in self.corpus.boundaries.iter().zip(&self.boundaries) {
            println!("observed {name}: boundary {boundary:?} first {witness:?}");
        }
    }
}
/// Prints every observation, then checks each against its pins, so one run
/// reports every measured feature even when a pin differs.
fn check_observations(observations: &[Observation<'_>], bounded: bool) {
    for observed in observations {
        observed.report();
    }
    let mut differences = vec![];
    for observed in observations {
        let corpus = observed.corpus;
        let name = corpus.name;
        let cases = if bounded {
            corpus.profile.len()
        } else {
            corpus.size
        };
        if observed.cases != cases {
            differences.push(format!("{name}: {} cases, pinned {cases}", observed.cases));
        }
        for ((dimension, pinned), values) in corpus.dimensions.iter().zip(&observed.values) {
            if !values.iter().eq(pinned.iter()) {
                differences.push(format!(
                    "{name}: {dimension} values {values:?}, pinned {pinned:?}"
                ));
            }
        }
        if !observed.outcomes.keys().eq(corpus.outcomes.iter()) {
            differences.push(format!(
                "{name}: outcome signatures {:?}, pinned {:?}",
                observed.outcomes.keys(),
                corpus.outcomes
            ));
        }
        for ((boundary, _), witness) in corpus.boundaries.iter().zip(&observed.boundaries) {
            if witness.is_none() {
                differences.push(format!("{name}: no {boundary:?} witness"));
            }
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
fn bounded(
    corpus: &'static Corpus,
    authority: &a::Authority<'_>,
    framing: &c::Framing,
) -> Observation<'static> {
    assert!(
        corpus.profile.windows(2).all(|pair| pair[0] < pair[1]),
        "{}: bounded cases must be strictly ascending",
        corpus.name
    );
    let mut observation = Observation::new(corpus);
    for case in corpus.profile {
        observation.exercise(authority, framing, case);
    }
    observation
}
/// Smallest whole minimum output for a treasury proposal.
fn treasury_least(buys: bool, amount: i64, price: i64) -> i64 {
    let (numerator, denominator) = if buys {
        (amount * 3, 4 * price)
    } else {
        (amount * price * 3, 4)
    };
    (numerator + denominator - 1) / denominator
}
/// Proposal value, daily spend and least output when every earlier guard passes.
fn treasury_proposal(d: &[i64]) -> Option<(i64, i64, i64)> {
    let (spent, seen, now, buys, amount, price) = (d[0], d[1], d[2], d[3] == 173, d[4], d[6]);
    (d[7] == 170 && now > seen).then(|| {
        let spent = if now / 4 > seen / 4 { 0 } else { spent };
        let value = if buys { amount } else { amount * price };
        (value, spent, treasury_least(buys, amount, price))
    })
}
/// Whether a guard-partition proposal passes authority, clock, oracle, cap and
/// minimum-output guards, reaching the reserve guard.
fn treasury_reserve_reached(d: &[i64]) -> bool {
    let (buys, amount, min_out, price) = (d[2] == 173, d[3], d[4], d[8]);
    let value = if buys { amount } else { amount * price };
    d[5] == 178
        && d[6] == 180
        && (d[7] == 3 || d[7] == 4)
        && value <= 3
        && min_out >= treasury_least(buys, amount, price)
}
/// Whether a callback for the pending intent reaches its fill or refund.
fn treasury_callback_reached(d: &[i64]) -> bool {
    d[2] != 170 && d[8] == 2
}

const COUNTER_CORPUS: Corpus = Corpus {
    name: "durable-counter",
    app: 0,
    size: 64,
    dimensions: &[
        ("count", &[0, 1, 2, 3]),
        ("failed", &[0, 1, 2, 3]),
        ("action", &[120, 121]),
        ("allowed", &[0, 1]),
    ],
    input: |d| d.to_vec(),
    outcomes: &["accept", "failure 202", "reject 200", "reject 201"],
    boundaries: &[
        ("allowed increment reaches 3", |d| {
            d[3] == 1 && d[usize::from(d[2] == 121)] == 2
        }),
        ("allowed increment at 3", |d| {
            d[3] == 1 && d[usize::from(d[2] == 121)] == 3
        }),
    ],
    profile: &[
        &[0, 1, 120, 1],
        &[0, 2, 121, 1],
        &[1, 0, 120, 0],
        &[2, 3, 121, 1],
        &[3, 0, 120, 0],
    ],
};
const STOCK_CORPUS: Corpus = Corpus {
    name: "inventory-reservation",
    app: 1,
    size: 864,
    dimensions: &[
        ("available", &[0, 1, 2, 3, 4, 5]),
        ("reserved", &[0, 1, 2, 3, 4, 5]),
        ("action", &[150, 151, 152, 153]),
        ("quantity", &[1, 2, 3]),
        ("authorized", &[0, 1]),
    ],
    input: |d| d.to_vec(),
    outcomes: &[
        "accept",
        "reject 200",
        "reject 201",
        "reject 202",
        "reject 203",
    ],
    boundaries: &[
        ("reserve takes all available", |d| {
            d[4] == 1 && d[2] == 150 && d[0] == d[3]
        }),
        ("reserve short by one", |d| {
            d[4] == 1 && d[2] == 150 && d[0] + 1 == d[3]
        }),
        ("release or ship takes all reserved", |d| {
            d[4] == 1 && (d[2] == 151 || d[2] == 152) && d[1] == d[3]
        }),
        ("release or ship short by one", |d| {
            d[4] == 1 && (d[2] == 151 || d[2] == 152) && d[1] + 1 == d[3]
        }),
        ("reserve fills reserved to 5", |d| {
            d[4] == 1 && d[2] == 150 && d[0] >= d[3] && d[1] + d[3] == 5
        }),
        ("reserve overfills reserved by one", |d| {
            d[4] == 1 && d[2] == 150 && d[0] >= d[3] && d[1] + d[3] == 6
        }),
        ("release or restock fills available to 5", |d| {
            d[4] == 1 && (d[2] == 153 || (d[2] == 151 && d[1] >= d[3])) && d[0] + d[3] == 5
        }),
        ("release or restock overfills available by one", |d| {
            d[4] == 1 && (d[2] == 153 || (d[2] == 151 && d[1] >= d[3])) && d[0] + d[3] == 6
        }),
    ],
    profile: &[
        &[0, 0, 152, 2, 0],
        &[1, 4, 150, 1, 1],
        &[2, 1, 150, 3, 1],
        &[2, 5, 153, 3, 1],
        &[3, 3, 151, 3, 1],
        &[4, 2, 151, 3, 1],
        &[5, 3, 150, 3, 1],
    ],
};
const ORDER_CORPUS: Corpus = Corpus {
    name: "order-fulfillment",
    app: 2,
    size: 1728,
    dimensions: &[
        ("status", &[160, 161, 162, 163, 164, 165]),
        ("attempts", &[0, 1, 2, 3]),
        ("action", &[150, 151, 152, 153, 154, 155]),
        ("callback", &[0, 1, 2, 3]),
        ("caller", &[170, 171, 172]),
    ],
    input: |d| d.to_vec(),
    outcomes: &[
        "accept",
        "failure 204",
        "refused Core(Law(Violated))",
        "reject 200",
        "reject 201",
        "reject 202",
        "reject 203",
    ],
    boundaries: &[
        ("payment callback matches attempts", |d| {
            (d[2] == 151 || d[2] == 152) && d[4] == 171 && d[0] == 161 && d[3] == d[1]
        }),
        ("payment callback differs from attempts", |d| {
            (d[2] == 151 || d[2] == 152) && d[4] == 171 && d[0] == 161 && d[3] != d[1]
        }),
        ("placement at attempt limit 3", |d| {
            d[2] == 150 && d[4] == 170 && d[0] == 160 && d[1] == 3
        }),
        ("placement below attempt limit", |d| {
            d[2] == 150 && d[4] == 170 && d[0] == 160 && d[1] == 2
        }),
    ],
    profile: &[
        &[160, 2, 150, 1, 170],
        &[160, 3, 150, 0, 170],
        &[161, 0, 151, 0, 171],
        &[161, 0, 151, 1, 171],
        &[161, 0, 152, 0, 171],
        &[162, 0, 153, 0, 172],
        &[162, 1, 152, 2, 172],
        &[163, 0, 154, 0, 172],
        &[163, 3, 153, 3, 172],
        &[164, 0, 155, 0, 170],
        &[165, 0, 150, 0, 170],
    ],
};
const ACCOUNT_CORPUS: Corpus = Corpus {
    name: "account-lockout",
    app: 3,
    size: 13_122,
    dimensions: &[
        ("failed", &[0, 1, 2]),
        (
            "until",
            &[
                0, 1, 899, 900, 901, 4102444800, 4102444801, 4102445699, 4102445700,
            ],
        ),
        (
            "seen",
            &[
                0, 1, 899, 900, 901, 4102443899, 4102443900, 4102444799, 4102444800,
            ],
        ),
        ("action", &[120, 121, 122]),
        (
            "now",
            &[
                0, 1, 899, 900, 901, 4102443899, 4102443900, 4102444799, 4102444800,
            ],
        ),
        ("admin", &[0, 1]),
    ],
    input: |d| d.to_vec(),
    outcomes: &[
        "accept",
        "failure 203",
        "reject 200",
        "reject 201",
        "reject 202",
    ],
    boundaries: &[
        ("clock one before last seen", |d| d[4] + 1 == d[2]),
        ("clock equals last seen", |d| d[4] == d[2]),
        ("clock one after last seen", |d| d[4] == d[2] + 1),
        ("attempt one before lockout deadline", |d| {
            d[3] != 122 && d[4] >= d[2] && d[4] + 1 == d[1]
        }),
        ("attempt at lockout deadline", |d| {
            d[3] != 122 && d[4] >= d[2] && d[4] == d[1]
        }),
        ("attempt one after lockout deadline", |d| {
            d[3] != 122 && d[4] >= d[2] && d[4] == d[1] + 1
        }),
        ("third failure at the largest clock", |d| {
            d[0] == 2 && d[3] == 121 && d[4] == 4102444800 && d[4] >= d[2] && d[4] >= d[1]
        }),
    ],
    profile: &[
        &[0, 0, 1, 120, 1, 1],
        &[0, 899, 899, 122, 901, 0],
        &[0, 900, 901, 120, 0, 0],
        &[0, 901, 0, 120, 900, 0],
        &[0, 4102444801, 4102443899, 120, 4102443899, 0],
        &[0, 4102445699, 4102443900, 120, 4102443900, 0],
        &[0, 4102445700, 4102444800, 120, 4102444799, 0],
        &[1, 1, 900, 122, 899, 0],
        &[2, 4102444800, 4102444799, 121, 4102444800, 0],
    ],
};
const WITHDRAWAL_CORPUS: Corpus = Corpus {
    name: "withdrawal-queue",
    app: 4,
    size: 1_296_000,
    dimensions: &[
        ("balance", &[0, 1, 2, 3, 4]),
        ("lane_a", &[180, 181, 182]),
        ("amount_a", &[0, 1, 2, 3, 4]),
        ("lane_b", &[180, 181, 182]),
        ("amount_b", &[0, 1, 2, 3, 4]),
        ("pause", &[0, 1, 2]),
        ("must", &[0, 1]),
        ("priority", &[170, 171]),
        ("action", &[160, 161, 162]),
        ("lane", &[170, 171]),
        ("amount", &[1, 2]),
        ("caller", &[190, 191, 192, 193]),
        ("alarm", &[0, 1]),
    ],
    input: |d| d.to_vec(),
    outcomes: &[
        "accept",
        "refused Core(Decision(Domain))",
        "refused Core(Law(Violated))",
        "refused Core(Schema)",
        "reject 200",
        "reject 201",
        "reject 202",
        "reject 203",
    ],
    boundaries: &[
        ("request equals free balance", |d| {
            let a = d[9] == 170;
            d[8] == 161
                && d[11] == if a { 191 } else { 192 }
                && d[if a { 1 } else { 3 }] == 180
                && d[10] == d[0] - d[2] - d[4]
        }),
        ("request exceeds free balance by one", |d| {
            let a = d[9] == 170;
            d[8] == 161
                && d[11] == if a { 191 } else { 192 }
                && d[if a { 1 } else { 3 }] == 180
                && d[10] == d[0] - d[2] - d[4] + 1
        }),
        ("deposit reaches cap 4", |d| {
            d[8] == 160 && d[11] == 190 && d[0] + d[10] == 4
        }),
        ("deposit exceeds cap by one", |d| {
            d[8] == 160 && d[11] == 190 && d[0] + d[10] == 5
        }),
        ("controller step from an invariant-lawful state", |d| {
            d[8] == 162 && d[11] == 193 && vault_invariant(&d[..8])
        }),
        ("controller step from an invariant-unlawful state", |d| {
            d[8] == 162 && d[11] == 193 && !vault_invariant(&d[..8])
        }),
    ],
    profile: &[
        &[0, 180, 0, 180, 1, 0, 0, 170, 162, 170, 1, 193, 0],
        &[0, 180, 0, 181, 1, 0, 0, 170, 162, 170, 1, 193, 0],
        &[0, 180, 4, 180, 0, 0, 0, 170, 160, 170, 1, 191, 0],
        &[1, 180, 0, 180, 0, 0, 0, 170, 161, 170, 1, 191, 0],
        &[1, 180, 0, 180, 0, 0, 1, 170, 161, 170, 1, 191, 0],
        &[1, 182, 2, 182, 2, 2, 0, 170, 161, 170, 1, 191, 0],
        &[2, 181, 1, 181, 0, 1, 1, 171, 160, 171, 2, 190, 1],
        &[3, 180, 0, 180, 3, 0, 0, 170, 161, 171, 1, 192, 0],
        &[3, 180, 3, 180, 0, 0, 0, 170, 160, 170, 2, 190, 0],
        &[4, 180, 0, 181, 4, 0, 0, 170, 162, 170, 1, 193, 0],
    ],
};
const TREASURY_PROPOSALS: Corpus = Corpus {
    name: "agent-treasury-guard proposals",
    app: 5,
    size: 103_680,
    dimensions: &[
        ("spent", &[0, 1, 2, 3, 4]),
        ("seen", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        ("now", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        ("direction", &[173, 174]),
        ("amount", &[1, 2, 3]),
        ("min_out", &[0, 1, 2, 3]),
        ("price", &[1, 2]),
        ("pending", &[170, 171, 172]),
    ],
    // Funded state isolates cap/day/rounding from reserves; one clock is both
    // the context time and the oracle time.
    input: |d| {
        vec![
            20, 20, d[0], d[1], d[7], 1, 1, 175, d[3], d[4], d[5], 0, 0, 178, d[2], d[6], d[2], 180,
        ]
    },
    outcomes: &[
        "accept",
        "reject 201",
        "reject 204",
        "reject 208",
        "reject 209",
        "reject 210",
    ],
    boundaries: &[
        ("clock equals last seen", |d| d[2] == d[1]),
        ("clock one after last seen", |d| d[2] == d[1] + 1),
        ("same day keeps spend", |d| {
            treasury_proposal(d).is_some() && d[2] / 4 == d[1] / 4 && d[0] > 0
        }),
        ("new day resets spend", |d| {
            treasury_proposal(d).is_some() && d[2] / 4 > d[1] / 4 && d[0] > 0
        }),
        ("value at cap 3", |d| {
            treasury_proposal(d).is_some_and(|(value, _, _)| value == 3)
        }),
        ("value over cap by one", |d| {
            treasury_proposal(d).is_some_and(|(value, _, _)| value == 4)
        }),
        ("daily spend reaches 4", |d| {
            treasury_proposal(d).is_some_and(|(value, spent, _)| value <= 3 && spent + value == 4)
        }),
        ("daily spend exceeds 4 by one", |d| {
            treasury_proposal(d).is_some_and(|(value, spent, _)| value <= 3 && spent + value == 5)
        }),
        ("minimum output equals rounded quote", |d| {
            treasury_proposal(d).is_some_and(|(value, spent, least)| {
                spent + value <= 4 && value <= 3 && d[5] == least
            })
        }),
        ("minimum output one below rounded quote", |d| {
            treasury_proposal(d).is_some_and(|(value, spent, least)| {
                spent + value <= 4 && value <= 3 && d[5] + 1 == least
            })
        }),
    ],
    profile: &[
        &[0, 2, 2, 174, 1, 0, 1, 171],
        &[0, 5, 7, 174, 2, 0, 2, 170],
        &[0, 6, 0, 173, 1, 0, 1, 170],
        &[0, 7, 3, 173, 1, 0, 1, 170],
        &[0, 8, 8, 173, 1, 0, 1, 170],
        &[0, 9, 9, 173, 1, 0, 1, 170],
        &[0, 10, 10, 173, 1, 0, 1, 170],
        &[0, 11, 11, 173, 1, 0, 1, 170],
        &[1, 0, 1, 173, 3, 1, 2, 170],
        &[2, 1, 4, 173, 2, 2, 1, 170],
        &[3, 3, 5, 173, 1, 3, 1, 172],
        &[4, 4, 6, 173, 1, 0, 1, 170],
    ],
};
const TREASURY_CALLBACKS: Corpus = Corpus {
    name: "agent-treasury-guard callbacks",
    app: 5,
    size: 677_376,
    dimensions: &[
        (
            "quote",
            &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ],
        ),
        (
            "base",
            &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            ],
        ),
        ("pending", &[170, 171, 172]),
        ("held", &[0, 1, 2, 3]),
        ("minimum", &[0, 1, 2, 3]),
        ("action", &[176, 177]),
        ("amount_out", &[0, 1, 2, 3]),
        ("now", &[3, 4]),
        ("intent", &[1, 2]),
    ],
    // One clock is both the context time and the oracle time.
    input: |d| {
        vec![
            d[0], d[1], 4, 2, d[2], d[3], d[4], d[5], 173, 1, 0, d[8], d[6], 179, d[7], 1, d[7],
            180,
        ]
    },
    outcomes: &[
        "accept",
        "failure 212",
        "refused Core(Decision(Domain))",
        "refused Core(Law(Violated))",
        "reject 205",
        "reject 206",
        "reject 207",
    ],
    boundaries: &[
        ("callback for the pending intent", treasury_callback_reached),
        ("callback for another intent", |d| d[2] != 170 && d[8] == 1),
        ("fill equals minimum", |d| {
            treasury_callback_reached(d) && d[5] == 176 && d[6] == d[4]
        }),
        ("fill one below minimum", |d| {
            treasury_callback_reached(d) && d[5] == 176 && d[6] + 1 == d[4]
        }),
        ("fill reaches balance cap 20", |d| {
            treasury_callback_reached(d)
                && d[5] == 176
                && d[6] >= d[4]
                && d[usize::from(d[2] == 171)] + d[6] == 20
        }),
        ("fill exceeds balance cap by one", |d| {
            treasury_callback_reached(d)
                && d[5] == 176
                && d[6] >= d[4]
                && d[usize::from(d[2] == 171)] + d[6] == 21
        }),
        ("refund reaches balance cap 20", |d| {
            treasury_callback_reached(d) && d[5] == 177 && d[usize::from(d[2] != 171)] + d[3] == 20
        }),
        ("refund exceeds balance cap by one", |d| {
            treasury_callback_reached(d) && d[5] == 177 && d[usize::from(d[2] != 171)] + d[3] == 21
        }),
    ],
    profile: &[
        &[0, 4, 171, 0, 0, 177, 0, 3, 2],
        &[0, 17, 171, 0, 3, 176, 3, 3, 2],
        &[1, 0, 172, 1, 0, 177, 0, 4, 1],
        &[2, 18, 172, 3, 1, 177, 1, 3, 2],
        &[3, 1, 170, 2, 2, 176, 2, 3, 1],
        &[4, 2, 171, 0, 1, 176, 0, 3, 2],
        &[5, 19, 171, 0, 0, 176, 2, 3, 2],
        &[6, 20, 172, 0, 0, 177, 0, 3, 2],
        &[7, 3, 171, 0, 0, 176, 0, 3, 2],
        &[8, 5, 170, 0, 0, 176, 0, 3, 1],
        &[9, 6, 170, 0, 0, 176, 0, 3, 1],
        &[10, 7, 170, 0, 0, 176, 0, 3, 1],
        &[11, 8, 170, 0, 0, 176, 0, 3, 1],
        &[12, 9, 170, 0, 0, 176, 0, 3, 1],
        &[13, 10, 170, 0, 0, 176, 0, 3, 1],
        &[14, 11, 170, 0, 0, 176, 0, 3, 1],
        &[15, 12, 170, 0, 0, 176, 0, 3, 1],
        &[16, 13, 170, 0, 0, 176, 0, 3, 1],
        &[17, 14, 170, 0, 0, 176, 0, 3, 1],
        &[18, 15, 170, 0, 0, 176, 0, 3, 1],
        &[19, 16, 170, 0, 0, 176, 0, 3, 1],
        &[20, 0, 170, 0, 0, 176, 0, 3, 1],
    ],
};
const TREASURY_GUARDS: Corpus = Corpus {
    name: "agent-treasury-guard reserves and guards",
    app: 5,
    size: 82_944,
    dimensions: &[
        ("quote", &[0, 1, 2, 3, 4, 5]),
        ("base", &[0, 1, 2, 3]),
        ("direction", &[173, 174]),
        ("amount", &[1, 2, 3]),
        ("min_out", &[0, 1, 2, 3]),
        ("caller", &[178, 179]),
        ("model", &[180, 181, 182]),
        ("price_time", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        ("price", &[1, 2]),
    ],
    // Day boundary: last seen 3, clock 4.
    input: |d| {
        vec![
            d[0], d[1], 4, 3, 170, 0, 0, 175, d[2], d[3], d[4], 0, 0, d[5], 4, d[8], d[7], d[6],
        ]
    },
    outcomes: &[
        "accept",
        "refused Core(Law(Violated))",
        "reject 200",
        "reject 202",
        "reject 203",
        "reject 208",
        "reject 210",
        "reject 211",
    ],
    boundaries: &[
        ("price time equals clock", |d| {
            d[5] == 178 && d[6] == 180 && d[7] == 4
        }),
        ("price one tick old", |d| {
            d[5] == 178 && d[6] == 180 && d[7] == 3
        }),
        ("price two ticks old", |d| {
            d[5] == 178 && d[6] == 180 && d[7] == 2
        }),
        ("price one tick ahead", |d| {
            d[5] == 178 && d[6] == 180 && d[7] == 5
        }),
        ("buy leaves reserve 2", |d| {
            treasury_reserve_reached(d) && d[2] == 173 && d[0] - d[3] == 2
        }),
        ("buy leaves reserve 1", |d| {
            treasury_reserve_reached(d) && d[2] == 173 && d[0] - d[3] == 1
        }),
        ("sale uses all base", |d| {
            treasury_reserve_reached(d) && d[2] == 174 && d[1] == d[3]
        }),
        ("sale exceeds base by one", |d| {
            treasury_reserve_reached(d) && d[2] == 174 && d[1] + 1 == d[3]
        }),
    ],
    profile: &[
        &[0, 0, 173, 1, 0, 178, 180, 3, 1],
        &[0, 0, 173, 1, 0, 178, 180, 6, 1],
        &[0, 0, 173, 1, 0, 178, 180, 7, 1],
        &[0, 0, 173, 1, 0, 178, 180, 8, 1],
        &[0, 0, 173, 1, 0, 178, 180, 9, 1],
        &[0, 0, 173, 1, 0, 178, 180, 10, 1],
        &[0, 0, 173, 1, 0, 178, 180, 11, 1],
        &[0, 0, 174, 2, 0, 178, 180, 3, 2],
        &[0, 1, 174, 1, 1, 178, 180, 3, 1],
        &[1, 0, 173, 2, 0, 179, 181, 0, 2],
        &[2, 0, 173, 1, 1, 178, 180, 3, 1],
        &[2, 2, 174, 3, 3, 178, 180, 4, 1],
        &[3, 0, 173, 1, 1, 178, 180, 3, 1],
        &[3, 3, 173, 1, 2, 178, 180, 2, 1],
        &[4, 0, 173, 1, 0, 178, 182, 1, 1],
        &[5, 0, 173, 1, 0, 178, 180, 5, 1],
    ],
};

macro_rules! with_app {
    ($module:ident,$body:expr) => {{
        let contract = $module::Contract::new();
        let descriptor = contract.descriptor();
        let authority =
            $module::checked_authority(&descriptor).unwrap_or_else(|error| match error {
                $module::BindFailure::Catalog(error) => {
                    panic!("checked original catalog: {error:?}")
                }
                $module::BindFailure::Authority(error) => {
                    panic!("checked original authority: {error:?}")
                }
            });
        $body(&authority, &$module::FRAMING)
    }};
}
/// `zeno-fcis generate contract` writes v2/policy.zcve from its own model;
/// this recomputes the bytes from each compiled `v2_contract.rs`.
#[test]
fn compiled_contracts_reproduce_their_committed_policy_bytes() {
    macro_rules! check {
        ($module:ident,$name:literal) => {{
            let contract = $module::Contract::new();
            let d = contract.descriptor();
            assert!(c::bind(&d).is_ok(), concat!($name, " descriptor"));
            let bytes = a::policy_bytes(
                &d,
                $module::ORIGINAL_SCHEMA,
                &$module::FRAMING,
                $module::CHANNEL_ROOTS,
            )
            .expect("library policy encoding");
            assert!(
                bytes == $module::ORIGINAL_POLICY,
                concat!($name, " policy bytes")
            );
        }};
    }
    check!(counter, "durable-counter");
    check!(stock, "inventory-reservation");
    check!(order, "order-fulfillment");
    check!(account, "account-lockout");
    check!(vault, "withdrawal-queue");
    check!(treasury, "agent-treasury-guard");
    check!(prepared, "prepared-counter");
    check!(gateway, "compliance-gateway");
}
#[test]
fn retained_complete_examples_genesis_and_replay() {
    let counts = [
        with_app!(counter, |a, f| examples(0, a, f)),
        with_app!(stock, |a, f| examples(1, a, f)),
        with_app!(order, |a, f| examples(2, a, f)),
        with_app!(account, |a, f| examples(3, a, f)),
        with_app!(vault, |a, f| examples(4, a, f)),
        with_app!(treasury, |a, f| examples(5, a, f)),
    ];
    assert_eq!(counts, [12, 20, 23, 20, 26, 30]);
    macro_rules! genesis {
        ($module:ident,$index:expr) => {
            with_app!($module, |a: &a::Authority<'_>, f: &c::Framing| {
                let app = &APPS[$index];
                // The generated contract states the retained genesis.
                let retained: Vec<c::Field<'static>> = app
                    .state
                    .iter()
                    .zip(app.genesis)
                    .map(|((id, kind), n)| c::Field {
                        id: *id,
                        value: atom(*kind, *n),
                    })
                    .collect();
                assert_eq!($module::GENESIS, retained.as_slice(), "{}", app.name);
                let raw = frame(&f.state, record(app.state, app.genesis));
                let result = a.genesis(&raw);
                assert!(result.result().is_ok(), "{:?}", result.result());
                assert!(
                    a.replay_genesis(&raw, result.subject().unwrap())
                        .result()
                        .is_ok()
                );
                let mut bad = app.genesis.to_vec();
                bad[0] += 1;
                let raw = frame(&f.state, record(app.state, &bad));
                assert!(a.genesis(&raw).result().is_err());
            });
        };
    }
    genesis!(counter, 0);
    genesis!(stock, 1);
    genesis!(order, 2);
    genesis!(account, 3);
    genesis!(vault, 4);
    genesis!(treasury, 5);
    println!(
        "retained examples=131; genesis/replay=6; eight examples retain owner-review-pending provenance"
    );
}
#[test]
fn all_original_small_domains_and_unlawful_order_states() {
    let mut counts = [0; 3];
    let mut unlawful = 0;
    let counter = with_app!(counter, |a, f| {
        let mut observed = Observation::new(&COUNTER_CORPUS);
        for count in 0..=3 {
            for failed in 0..=3 {
                for action in 120..=121 {
                    for allowed in 0..=1 {
                        observed.exercise(a, f, &[count, failed, action, allowed]);
                        counts[0] += 1;
                    }
                }
            }
        }
        observed
    });
    let stock = with_app!(stock, |a, f| {
        let mut observed = Observation::new(&STOCK_CORPUS);
        for available in 0..=5 {
            for reserved in 0..=5 {
                for action in 150..=153 {
                    for quantity in 1..=3 {
                        for authorized in 0..=1 {
                            observed.exercise(
                                a,
                                f,
                                &[available, reserved, action, quantity, authorized],
                            );
                            counts[1] += 1;
                        }
                    }
                }
            }
        }
        observed
    });
    let order = with_app!(order, |a, f| {
        let mut observed = Observation::new(&ORDER_CORPUS);
        for status in 160..=165 {
            for attempts in 0..=3 {
                for action in 150..=155 {
                    for callback in 0..=3 {
                        for caller in 170..=172 {
                            if observed.exercise(
                                a,
                                f,
                                &[status, attempts, action, callback, caller],
                            ) {
                                unlawful += 1;
                            }
                            counts[2] += 1;
                        }
                    }
                }
            }
        }
        observed
    });
    check_observations(&[counter, stock, order], false);
    assert_eq!(counts, [64, 864, 1728]);
    assert!(unlawful > 0);
    println!("complete original domain counts={counts:?}; order technical refusals={unlawful}");
}

#[test]
fn account_complete_clock_boundaries_without_domain_narrowing() {
    let clocks = [
        0, 1, 899, 900, 901, 4102443899, 4102443900, 4102444799, 4102444800,
    ];
    let deadlines = [
        0, 1, 899, 900, 901, 4102444800, 4102444801, 4102445699, 4102445700,
    ];
    let mut count = 0;
    let observed = with_app!(account, |a, f| {
        let mut observed = Observation::new(&ACCOUNT_CORPUS);
        for failed in 0..=2 {
            for seen in clocks {
                for until in deadlines {
                    for now in clocks {
                        for action in 120..=122 {
                            for admin in 0..=1 {
                                observed.exercise(a, f, &[failed, until, seen, action, now, admin]);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
        observed
    });
    check_observations(&[observed], false);
    assert_eq!(count, 13122);
    println!(
        "account independent boundary tuples={count}; full original admitted domains retained"
    );
}

#[test]
fn withdrawal_complete_raw_domain_and_certified_controller() {
    let mut cases = 0;
    let mut refusals = 0;
    let mut unlawful_pre = 0;
    let observed = with_app!(vault, |a, f| {
        let mut observed = Observation::new(&WITHDRAWAL_CORPUS);
        for balance in 0..=4 {
            for lane_a in 180..=182 {
                for amount_a in 0..=4 {
                    for lane_b in 180..=182 {
                        for amount_b in 0..=4 {
                            for pause in 0..=2 {
                                for must in 0..=1 {
                                    for priority in 170..=171 {
                                        let state = [
                                            balance, lane_a, amount_a, lane_b, amount_b, pause,
                                            must, priority,
                                        ];
                                        for action in 160..=162 {
                                            for lane in 170..=171 {
                                                for amount in 1..=2 {
                                                    for caller in 190..=193 {
                                                        for alarm in 0..=1 {
                                                            let all = [
                                                                balance, lane_a, amount_a, lane_b,
                                                                amount_b, pause, must, priority,
                                                                action, lane, amount, caller,
                                                                alarm,
                                                            ];
                                                            if observed.exercise(a, f, &all) {
                                                                refusals += 1;
                                                            }
                                                            if !vault_invariant(&state) {
                                                                unlawful_pre += 1;
                                                            }
                                                            cases += 1;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        observed
    });
    check_observations(&[observed], false);
    assert_eq!(cases, 1_296_000);
    assert!(refusals > 0 && unlawful_pre > 0);
    println!(
        "withdrawal full raw-domain tuples={cases}; invariant-unlawful pre tuples={unlawful_pre}; technical refusals={refusals}"
    );
}

#[test]
fn treasury_guard_arithmetic_callbacks_and_unlawful_prestates() {
    let mut counts = [0; 3];
    let mut refusals = 0;
    let observed = with_app!(treasury, |a, f| {
        let mut observed = [
            Observation::new(&TREASURY_PROPOSALS),
            Observation::new(&TREASURY_CALLBACKS),
            Observation::new(&TREASURY_GUARDS),
        ];
        // All proposal arithmetic parameters, all clock pairs, all pending tags;
        // funded state deliberately isolates cap/day/rounding from reserves.
        for spent in 0..=4 {
            for seen in 0..=11 {
                for now in 0..=11 {
                    for direction in 173..=174 {
                        for amount in 1..=3 {
                            for min_out in 0..=3 {
                                for price in 1..=2 {
                                    for pending in 170..=172 {
                                        let case = [
                                            spent, seen, now, direction, amount, min_out, price,
                                            pending,
                                        ];
                                        if observed[0].exercise(a, f, &case) {
                                            refusals += 1;
                                        }
                                        counts[0] += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Complete balances, pending amounts/minimums and callback amount domain;
        // both callback kinds, wrong intent, all tags, same-day and reset-day clocks.
        for quote in 0..=20 {
            for base in 0..=20 {
                for pending in 170..=172 {
                    for held in 0..=3 {
                        for minimum in 0..=3 {
                            for action in 176..=177 {
                                for amount_out in 0..=3 {
                                    for now in [3, 4] {
                                        for intent in [1, 2] {
                                            let case = [
                                                quote, base, pending, held, minimum, action,
                                                amount_out, now, intent,
                                            ];
                                            if observed[1].exercise(a, f, &case) {
                                                refusals += 1;
                                            }
                                            counts[1] += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Every reserve boundary and every caller/model/price-time option;
        // cross all ordered authority/clock/oracle guards at a day boundary.
        for quote in 0..=5 {
            for base in 0..=3 {
                for direction in 173..=174 {
                    for amount in 1..=3 {
                        for min_out in 0..=3 {
                            for caller in 178..=179 {
                                for model in 180..=182 {
                                    for price_time in 0..=11 {
                                        for price in 1..=2 {
                                            let case = [
                                                quote, base, direction, amount, min_out, caller,
                                                model, price_time, price,
                                            ];
                                            if observed[2].exercise(a, f, &case) {
                                                refusals += 1;
                                            }
                                            counts[2] += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        observed
    });
    check_observations(&observed, false);
    assert_eq!(counts, [103_680, 677_376, 82_944]);
    assert!(refusals > 0);
    println!(
        "treasury exhaustive partitions={counts:?}; technical refusals={refusals}; not a full Cartesian-domain enumeration"
    );
}

/// Pinned deterministic cases from each native corpus above, run identically
/// natively and under Miri, which skips those four corpora. Every case passes the
/// same independent comparison; the bounded cases must reach every outcome
/// signature, every dimension value and every named boundary relation of the
/// complete native corpus. That is finite sampling, not path, interaction or
/// undefined-behaviour coverage of the omitted cases.
#[test]
fn miri_bounded_template_domain_profiles() {
    let mut observed = vec![
        with_app!(counter, |a, f| bounded(&COUNTER_CORPUS, a, f)),
        with_app!(stock, |a, f| bounded(&STOCK_CORPUS, a, f)),
        with_app!(order, |a, f| bounded(&ORDER_CORPUS, a, f)),
        with_app!(account, |a, f| bounded(&ACCOUNT_CORPUS, a, f)),
        with_app!(vault, |a, f| bounded(&WITHDRAWAL_CORPUS, a, f)),
    ];
    observed.extend(with_app!(treasury, |a, f| {
        [&TREASURY_PROPOSALS, &TREASURY_CALLBACKS, &TREASURY_GUARDS]
            .map(|corpus| bounded(corpus, a, f))
    }));
    check_observations(&observed, true);
    let counts: Vec<_> = observed.iter().map(|observed| observed.cases).collect();
    assert_eq!(counts, [5, 7, 11, 9, 10, 12, 22, 16]);
    for observed in &observed {
        println!(
            "bounded Miri profile: {} {} of {} native cases",
            observed.corpus.name, observed.cases, observed.corpus.size
        );
    }
    // The 131 retained examples run under Miri in their own test; they are not
    // drawn from these corpora and are not counted against them.
    println!(
        "bounded Miri profile cases={}; retained examples [12, 20, 23, 20, 26, 30] run in retained_complete_examples_genesis_and_replay",
        counts.iter().sum::<usize>()
    );
}

#[test]
fn policy_schema_law_and_meter_mutations_refuse() {
    let contract = stock::Contract::new();
    let mut d = contract.descriptor();
    let exact = stock::checked_catalog(&d).unwrap();
    let authority = a::bind(&exact).unwrap();
    let app = &APPS[1];
    let raw = app.wire(&stock::FRAMING, &[3, 0], &[150, 2], &[1]);
    let input = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    let good = authority.evaluate(input);
    assert!(good.result().is_ok());
    let mut changed = good.subject().unwrap().to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert!(authority.replay(input, &changed).result().is_err());
    let mut command = raw[1].clone();
    command[48 + 5 + 2] = 0x07; // Actual Sum changed to Enum, same original type/variant bytes.
    assert!(
        authority
            .evaluate(c::Raw {
                command: &command,
                ..input
            })
            .result()
            .is_err()
    );
    let mut schema = stock::ORIGINAL_SCHEMA.to_vec();
    *schema.last_mut().unwrap() ^= 1;
    let limits = || catalog::Limits {
        schema: zeno_fcis_synthesis::finite::canonical_v2::schema::Limits {
            bytes: 4096,
            types: 32,
            fields: 16,
            variants: 16,
        },
        contract_bytes: 1_000_000,
    };
    assert!(matches!(
        catalog::bind_original(
            &schema,
            &stock::DESCRIPTION,
            limits(),
            stock::ORIGINAL_POLICY,
            &d,
            &stock::FRAMING,
            stock::CHANNEL_ROOTS
        ),
        Err(catalog::Failure::Schema(_))
    ));
    let mut policy = stock::ORIGINAL_POLICY.to_vec();
    *policy.last_mut().unwrap() ^= 1;
    assert!(matches!(
        catalog::bind_original(
            stock::ORIGINAL_SCHEMA,
            &stock::DESCRIPTION,
            limits(),
            &policy,
            &d,
            &stock::FRAMING,
            stock::CHANNEL_ROOTS
        ),
        Err(catalog::Failure::Policy)
    ));
    let mut branches = stock::BRANCHES.to_vec();
    branches[0].reason = Some(201);
    d.branches = &branches;
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Policy)
    ));
    d = contract.descriptor();
    d.required = &[];
    assert!(stock::checked_catalog(&d).is_err());
    d = contract.descriptor();
    d.limits = d.limits.with_limit(Resource::Step, 0);
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Policy)
    ));
    // Rebinding changed bytes as a new reviewed contract is intentionally possible
    // in the lower-level API. Even there the original state law refuses a bad post.
    let bad_nodes = [l::Op::Literal(l::Atom::Bool(false))];
    let mut laws: Vec<_> = stock::LAWS
        .iter()
        .map(|law| l::Law {
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: l::Program {
                nodes: law.program.nodes,
                root: law.program.root,
            },
        })
        .collect();
    laws[0].program = l::Program {
        nodes: &bad_nodes,
        root: 0,
    };
    d = contract.descriptor();
    d.laws = &laws;
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Policy)
    ));
    let core = c::bind(&d).unwrap();
    let failed = core.frame(c::Kind::Transition, input, &stock::FRAMING);
    assert!(matches!(failed.result(), Err(c::Failure::Law(_))));
    assert_eq!(failed.diagnostics()[0].id, 500);
}

#[test]
fn original_conservation_and_required_delivery_checks_remain_independent() {
    let contract = stock::Contract::new();
    let mut d = contract.descriptor();
    let mut branches = stock::BRANCHES.to_vec();
    let mut assignments = branches[4].assignments.to_vec();
    assignments[0].value = c::Expr::Constant(c::Atom::I128(2));
    branches[4].assignments = &assignments;
    d.branches = &branches;
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Policy)
    ));
    let raw = APPS[1].wire(&stock::FRAMING, &[3, 0], &[150, 2], &[1]);
    let input = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    let core = c::bind(&d).unwrap();
    let out = core.frame(c::Kind::Transition, input, &stock::FRAMING);
    assert!(matches!(out.result(), Err(c::Failure::Law(_))));
    assert_eq!(out.diagnostics().last().unwrap().id, 501);
    assert_eq!(out.diagnostics()[0].verdict, l::Verdict::Satisfied);
    let mut branches = stock::BRANCHES.to_vec();
    branches[6].outbox = &[];
    d = contract.descriptor();
    d.branches = &branches;
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Policy)
    ));
    let raw = APPS[1].wire(&stock::FRAMING, &[0, 3], &[152, 2], &[1]);
    let input = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    let core = c::bind(&d).unwrap();
    let out = core.frame(c::Kind::Transition, input, &stock::FRAMING);
    assert!(matches!(out.result(), Err(c::Failure::Law(_))));
    assert_eq!(out.diagnostics().last().unwrap().id, 991);
    d = contract.descriptor();
    d.laws = &stock::LAWS[1..];
    assert!(matches!(
        stock::checked_catalog(&d),
        Err(catalog::Failure::Descriptor)
    ));
}
