//! Independent complete-decision checks against the original applications.
//! Numeric reference rules and retained owner examples do not call the emitter,
//! its expression trees, or the generated scalar/law programs.
#![allow(clippy::too_many_lines, clippy::unwrap_used, clippy::expect_used)]
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/account-lockout/src/v2_contract.rs"]
mod account;
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
mod counter;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/compliance-gateway/src/v2_contract.rs"]
mod gateway;
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/order-fulfillment/src/v2_contract.rs"]
mod order;
#[allow(dead_code)]
#[allow(unreachable_pub)]
#[path = "../../../verification/kernel-laws/src/oracle/templates/withdrawal-queue/original/src/controller.rs"]
mod original_controller;
#[allow(dead_code, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/prepared-counter/src/v2_contract.rs"]
mod prepared;
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/inventory-reservation/src/v2_contract.rs"]
mod stock;
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/agent-treasury-guard/src/v2_contract.rs"]
mod treasury;
#[allow(unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/withdrawal-queue/src/v2_contract.rs"]
mod vault;

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
fn exercise(index: usize, authority: &a::Authority<'_>, framing: &c::Framing, all: &[i64]) -> bool {
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
    } else {
        // The checked Authority's very same immutable descriptor and protected
        // core are used for the million-tuple comparisons. Re-serializing its
        // complete embedded library sources on every tuple would add terabytes
        // of redundant seal work. Every retained example separately exercises
        // the production Authority and exact whole-subject replay.
        let core = c::bind(authority.descriptor()).unwrap();
        let output = core.frame(c::Kind::Transition, input, framing);
        check_report(
            app,
            output.result().map_err(a::Refusal::Core),
            output.raw(),
            output.usage(),
            output.reads(),
            output.diagnostics(),
            s,
            &expected,
        );
    }
    expected.refusal.is_some()
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
    with_app!(counter, |a, f| {
        for count in 0..=3 {
            for failed in 0..=3 {
                for action in 120..=121 {
                    for allowed in 0..=1 {
                        exercise(0, a, f, &[count, failed, action, allowed]);
                        counts[0] += 1;
                    }
                }
            }
        }
    });
    with_app!(stock, |a, f| {
        for available in 0..=5 {
            for reserved in 0..=5 {
                for action in 150..=153 {
                    for quantity in 1..=3 {
                        for authorized in 0..=1 {
                            exercise(
                                1,
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
    });
    with_app!(order, |a, f| {
        for status in 160..=165 {
            for attempts in 0..=3 {
                for action in 150..=155 {
                    for callback in 0..=3 {
                        for caller in 170..=172 {
                            if exercise(2, a, f, &[status, attempts, action, callback, caller]) {
                                unlawful += 1;
                            }
                            counts[2] += 1;
                        }
                    }
                }
            }
        }
    });
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
    with_app!(account, |a, f| {
        for failed in 0..=2 {
            for seen in clocks {
                for until in deadlines {
                    for now in clocks {
                        for action in 120..=122 {
                            for admin in 0..=1 {
                                exercise(3, a, f, &[failed, until, seen, action, now, admin]);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    });
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
    with_app!(vault, |a, f| {
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
                                                            if exercise(4, a, f, &all) {
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
    });
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
    with_app!(treasury, |a, f| {
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
                                        let all = [
                                            20, 20, spent, seen, pending, 1, 1, 175, direction,
                                            amount, min_out, 0, 0, 178, now, price, now, 180,
                                        ];
                                        if exercise(5, a, f, &all) {
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
                                            let all = [
                                                quote, base, 4, 2, pending, held, minimum, action,
                                                173, 1, 0, intent, amount_out, 179, now, 1, now,
                                                180,
                                            ];
                                            if exercise(5, a, f, &all) {
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
                                            let all = [
                                                quote, base, 4, 3, 170, 0, 0, 175, direction,
                                                amount, min_out, 0, 0, caller, 4, price,
                                                price_time, model,
                                            ];
                                            if exercise(5, a, f, &all) {
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
    });
    assert_eq!(counts, [103_680, 677_376, 82_944]);
    assert!(refusals > 0);
    println!(
        "treasury exhaustive partitions={counts:?}; technical refusals={refusals}; not a full Cartesian-domain enumeration"
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
