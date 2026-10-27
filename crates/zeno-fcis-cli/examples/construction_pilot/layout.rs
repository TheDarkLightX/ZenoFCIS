//! Fixed codecs and read-only features. Operation outputs remain grammar holes.

use super::{
    common::*,
    reference::{Relation, State},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeno_fcis_loop_doors::expr::{self, Allow, Expr, Op, Vocabulary};
use zeno_fcis_synthesis::finite::Domain;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Layout {
    T,
    I,
}
impl Layout {
    pub(super) fn parse(text: &str) -> Result<Self> {
        match text {
            "T" => Ok(Self::T),
            "I" => Ok(Self::I),
            _ => fail(65, "layout"),
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::T => "T",
            Self::I => "I",
        }
    }
    pub(super) fn cells(self) -> usize {
        match self {
            Self::T => 2,
            Self::I => 3,
        }
    }
    pub(super) fn cell_name(self, j: usize) -> &'static str {
        match self {
            Self::T => ["s0", "s1"][j],
            Self::I => ["cA", "cB", "cC"][j],
        }
    }
    pub(super) fn radix(self) -> i128 {
        match self {
            Self::T => 7,
            Self::I => 5,
        }
    }
    pub(super) fn raw_count(self) -> usize {
        match self {
            Self::T => 147,
            Self::I => 375,
        }
    }
    pub(super) fn bounds(self) -> Vec<i128> {
        let mut v = vec![2];
        v.extend(vec![self.radix() - 1; self.cells()]);
        v
    }
    pub(super) fn raw(self, mut ordinal: usize) -> Vec<i128> {
        let mut row = vec![0; self.cells() + 1];
        for value in row[1..].iter_mut().rev() {
            *value = (ordinal % self.radix() as usize) as i128;
            ordinal /= self.radix() as usize;
        }
        row[0] = ordinal as i128;
        row
    }
    pub(super) fn ordinal(self, row: &[i128]) -> Option<usize> {
        if row.len() != self.cells() + 1
            || row
                .iter()
                .zip(self.bounds())
                .any(|(x, max)| *x < 0 || *x > max)
        {
            return None;
        }
        Some(row[1..].iter().fold(row[0] as usize, |n, c| {
            n * self.radix() as usize + *c as usize
        }))
    }
    pub(super) fn decode(self, row: &[i128]) -> Option<State> {
        self.ordinal(row)?;
        let n = row[0];
        let mut entries = Vec::new();
        for (j, c) in row[1..].iter().enumerate() {
            if *c != 0 {
                let (d, p, t) = match self {
                    Self::T => ((c - 1) / 2, (c - 1) % 2, j as i128),
                    Self::I => (j as i128, (c - 1) % 2, (c - 1) / 2),
                };
                if t >= n
                    || entries
                        .iter()
                        .any(|e: &(i128, i128, i128)| e.0 == d || e.2 == t)
                {
                    return None;
                }
                entries.push((d, p, t));
            }
        }
        if entries.len() > 2 {
            return None;
        }
        entries.sort();
        Some(State { n, entries })
    }
    pub(super) fn encode(self, state: &State) -> Result<Vec<i128>> {
        let mut raw = vec![0; self.cells() + 1];
        raw[0] = state.n;
        for (d, p, t) in &state.entries {
            let j = match self {
                Self::T => *t,
                Self::I => *d,
            } as usize;
            if j >= self.cells() {
                return fail(65, "encode-cell");
            }
            raw[j + 1] = match self {
                Self::T => 1 + 2 * d + p,
                Self::I => 1 + 2 * t + p,
            };
        }
        if self.decode(&raw).as_ref() != Some(state) {
            return fail(65, "encode-roundtrip");
        }
        Ok(raw)
    }
    pub(super) fn alpha(self, relation: &Relation) -> Result<Vec<Option<usize>>> {
        (0..self.raw_count())
            .map(|s| {
                self.decode(&self.raw(s))
                    .map(|state| {
                        relation
                            .states
                            .iter()
                            .position(|v| *v == state)
                            .ok_or_else(|| malformed("decoded-state-outside-carrier"))
                    })
                    .transpose()
            })
            .collect()
    }
}

fn lit(n: i128) -> Expr {
    Expr::Int(n)
}
fn var(name: &str) -> Expr {
    Expr::Var(name.into())
}
fn bin(op: Op, a: Expr, b: Expr) -> Expr {
    Expr::bin(op, a, b)
}
fn eq(a: Expr, b: Expr) -> Expr {
    bin(Op::Eq, a, b)
}
fn lt(a: Expr, b: Expr) -> Expr {
    bin(Op::Lt, a, b)
}
fn and(a: Expr, b: Expr) -> Expr {
    bin(Op::And, a, b)
}
fn not(a: Expr) -> Expr {
    Expr::negation(a)
}
fn or(a: Expr, b: Expr) -> Expr {
    not(and(not(a), not(b)))
}
fn add(a: Expr, b: Expr) -> Expr {
    bin(Op::Add, a, b)
}
fn choose(c: Expr, a: Expr, b: Expr) -> Expr {
    Expr::select(c, a, b)
}
fn present(c: Expr) -> Expr {
    not(eq(c, lit(0)))
}
fn priority(layout: Layout, c: Expr) -> Expr {
    (1..layout.radix())
        .filter(|v| v % 2 == 0)
        .rev()
        .fold(lit(0), |tail, v| {
            choose(eq(c.clone(), lit(v)), lit(1), tail)
        })
}
fn identity(layout: Layout, c: Expr, j: usize) -> Expr {
    match layout {
        Layout::T => choose(
            lt(c.clone(), lit(3)),
            lit(0),
            choose(lt(c, lit(5)), lit(1), lit(2)),
        ),
        Layout::I => choose(present(c), lit(j as i128), lit(0)),
    }
}
fn stamp(layout: Layout, c: Expr, j: usize) -> Expr {
    match layout {
        Layout::T => choose(present(c), lit(j as i128), lit(0)),
        Layout::I => choose(lt(c, lit(3)), lit(0), lit(1)),
    }
}

// The baseline always uses the earlier arrival. Only the deliberate controls
// emitter calls this with reverse_tie=true; it grants no checker bypass.
fn minimum(layout: Layout, reverse_tie: bool) -> Expr {
    match layout {
        Layout::T => {
            let (s0, s1) = (var("s0"), var("s1"));
            let (p0, p1) = (priority(layout, s0.clone()), priority(layout, s1.clone()));
            let better = if reverse_tie {
                not(lt(p0, p1))
            } else {
                lt(p1, p0)
            };
            choose(
                and(present(s1.clone()), or(eq(s0.clone(), lit(0)), better)),
                identity(layout, s1, 1),
                identity(layout, s0, 0),
            )
        }
        Layout::I => {
            // Empty ranks last. Inline each key, preserving the first cell on
            // exact ties, including malformed raw states with equal stamps.
            let rank = |name| {
                let c = var(name);
                let occupied = if reverse_tie {
                    choose(
                        lt(c.clone(), lit(3)),
                        bin(Op::Sub, add(c.clone(), c.clone()), lit(1)),
                        bin(Op::Sub, add(c.clone(), c.clone()), lit(6)),
                    )
                } else {
                    choose(
                        eq(c.clone(), lit(2)),
                        lit(2),
                        choose(
                            eq(c.clone(), lit(3)),
                            lit(1),
                            bin(Op::Sub, c.clone(), lit(1)),
                        ),
                    )
                };
                choose(eq(c, lit(0)), lit(4), occupied)
            };
            choose(
                lt(rank("cB"), rank("cA")),
                choose(lt(rank("cC"), rank("cB")), lit(2), lit(1)),
                choose(lt(rank("cC"), rank("cA")), lit(2), lit(0)),
            )
        }
    }
}
pub(super) fn reversed_tie_definition(layout: Layout) -> Result<String> {
    policy_text(&minimum(layout, true))
}

#[cfg(test)]
#[path = "../../tests/construction_pilot/minimum.rs"]
mod minimum_tests;

/// Translate the shared loop AST into ordinary policy syntax, without a second
/// parser, evaluator or lowerer. No hole is permitted in materialized sources.
fn boolean(node: &Expr) -> bool {
    match node {
        Expr::Not(_) => true,
        Expr::Bin(op, ..) => op.is_boolean() || op.is_compare(),
        Expr::Var(name) => {
            matches!(
                name.as_str(),
                "insert" | "cancel" | "change" | "remove" | "target_present"
            ) || name
                .strip_prefix("present")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        }
        Expr::If(_, a, b) => boolean(a) || boolean(b),
        _ => false,
    }
}
pub(super) fn policy_text(node: &Expr) -> Result<String> {
    policy_as(node, boolean(node))
}
pub(super) fn policy_guard(node: &Expr) -> Result<String> {
    policy_as(node, true)
}
fn policy_as(node: &Expr, want_bool: bool) -> Result<String> {
    Ok(match node {
        Expr::Int(n) if want_bool => match n {
            0 => "false".into(),
            1 => "true".into(),
            _ => return fail(65, "policy-boolean-literal"),
        },
        Expr::Int(n) => n.to_string(),
        Expr::Var(name) => name.clone(),
        Expr::Not(a) => format!("!({})", policy_as(a, true)?),
        Expr::Bin(op, a, b) => {
            let side_bool = op.is_boolean() || (op.is_compare() && (boolean(a) || boolean(b)));
            format!(
                "({} {} {})",
                policy_as(a, side_bool)?,
                op.symbol(),
                policy_as(b, side_bool)?
            )
        }
        Expr::If(c, a, b) => format!(
            "choose({}, {}, {})",
            policy_as(c, true)?,
            policy_as(a, want_bool)?,
            policy_as(b, want_bool)?
        ),
        _ => return fail(65, "unfilled-or-unsupported-policy-expression"),
    })
}

pub(super) struct Features {
    pub(super) names: Vec<String>,
    pub(super) domains: Vec<Domain>,
    pub(super) helpers: BTreeMap<String, Expr>,
    pub(super) layout: Layout,
}
impl Features {
    pub(super) fn new(layout: Layout) -> Self {
        let mut names = vec!["n".into()];
        names.extend((0..layout.cells()).map(|j| layout.cell_name(j).to_string()));
        names.extend(["q".into(), "unit".into()]);
        let mut domains: Vec<_> = layout
            .bounds()
            .into_iter()
            .map(|max| Domain::Int { min: 0, max })
            .collect();
        domains.extend([
            Domain::Int { min: 0, max: 15 },
            Domain::Int { min: 0, max: 0 },
        ]);
        let mut helpers = BTreeMap::new();
        let mut put = |name: &str, domain: Domain, expression: Expr| {
            names.push(name.into());
            domains.push(domain);
            helpers.insert(name.into(), expression);
        };
        put("insert", Domain::Bool, lt(var("q"), lit(6)));
        put(
            "cancel",
            Domain::Bool,
            and(not(lt(var("q"), lit(6))), lt(var("q"), lit(9))),
        );
        put(
            "change",
            Domain::Bool,
            and(not(lt(var("q"), lit(9))), lt(var("q"), lit(15))),
        );
        put("remove", Domain::Bool, eq(var("q"), lit(15)));
        let request = choose(
            lt(var("q"), lit(6)),
            choose(
                lt(var("q"), lit(2)),
                lit(0),
                choose(lt(var("q"), lit(4)), lit(1), lit(2)),
            ),
            choose(
                lt(var("q"), lit(9)),
                bin(Op::Sub, var("q"), lit(6)),
                choose(
                    lt(var("q"), lit(15)),
                    choose(
                        lt(var("q"), lit(11)),
                        lit(0),
                        choose(lt(var("q"), lit(13)), lit(1), lit(2)),
                    ),
                    lit(0),
                ),
            ),
        );
        put("requested_id", Domain::Int { min: 0, max: 2 }, request);
        let requested_p = [1, 3, 5, 10, 12, 14].iter().rev().fold(lit(0), |tail, q| {
            choose(eq(var("q"), lit(*q)), lit(1), tail)
        });
        put(
            "requested_priority",
            Domain::Int { min: 0, max: 1 },
            requested_p,
        );
        let mut target = lit(0);
        let mut size = lit(0);
        for j in 0..layout.cells() {
            let c = var(layout.cell_name(j));
            let has = present(c.clone());
            let id = identity(layout, c.clone(), j);
            target = or(
                target,
                and(has.clone(), eq(id.clone(), var("requested_id"))),
            );
            size = add(size, choose(has.clone(), lit(1), lit(0)));
        }
        put("target_present", Domain::Bool, target);
        put(
            "size",
            Domain::Int {
                min: 0,
                max: layout.cells() as i128,
            },
            size,
        );
        put(
            "minimum_id",
            Domain::Int { min: 0, max: 2 },
            minimum(layout, false),
        );
        put(
            "next_n",
            Domain::Int { min: 0, max: 2 },
            choose(lt(var("n"), lit(2)), add(var("n"), lit(1)), lit(2)),
        );
        let encoding_identity = match layout {
            Layout::T => var("requested_id"),
            Layout::I => choose(lt(var("n"), lit(1)), var("n"), lit(1)),
        };
        put(
            "insert_token",
            Domain::Int {
                min: 1,
                max: layout.radix() - 1,
            },
            add(
                add(add(lit(1), encoding_identity.clone()), encoding_identity),
                var("requested_priority"),
            ),
        );
        for j in 0..layout.cells() {
            let c = var(layout.cell_name(j));
            put(&format!("present{j}"), Domain::Bool, present(c.clone()));
            put(
                &format!("identity{j}"),
                Domain::Int { min: 0, max: 2 },
                identity(layout, c.clone(), j),
            );
            let preserved = match layout {
                Layout::T => identity(layout, c.clone(), j),
                Layout::I => stamp(layout, c, j),
            };
            put(
                &format!("change_token{j}"),
                Domain::Int {
                    min: 1,
                    max: layout.radix() - 1,
                },
                add(
                    add(add(lit(1), preserved.clone()), preserved),
                    var("requested_priority"),
                ),
            );
        }
        Self {
            names,
            domains,
            helpers,
            layout,
        }
    }
    fn expand(&self, node: &Expr) -> Result<Expr> {
        expr::substitute(node, &mut |n| {
            if let Expr::Var(name) = n {
                self.helpers
                    .get(name)
                    .map(|value| {
                        self.expand(value)
                            .map_err(|e| zeno_fcis_loop_doors::Refusal::new(e.reason))
                    })
                    .transpose()
            } else {
                Ok(None)
            }
        })
        .map_err(|e| malformed(e.to_string()))
    }
    pub(super) fn values(&self, raw: &[i128], q: i128) -> Result<Vec<i128>> {
        let width = self.layout.cells() + 3;
        let vocab = Vocabulary::new(
            self.names[..width].to_vec(),
            self.domains[..width].to_vec(),
            vec![],
            &BTreeMap::new(),
        )
        .map_err(|e| malformed(e.to_string()))?;
        let mut base = raw.to_vec();
        base.extend([q, 0]);
        let columns: Vec<_> = base.iter().map(|n| vec![*n]).collect();
        let env = expr::Env {
            vocab: &vocab,
            columns: &columns,
            keep: None,
            len: 1,
        };
        let mut values = base;
        for (j, name) in self.names.iter().enumerate().skip(width) {
            let expanded = self.expand(
                self.helpers
                    .get(name)
                    .ok_or_else(|| malformed("missing-helper"))?,
            )?;
            let got = expr::evaluate(&expanded, &env, &BTreeMap::new())
                .map_err(|e| gate(e.to_string()))?;
            let value = got[0];
            if !self.domains[j].contains(value) {
                return fail(1, format!("feature-range:{name}"));
            }
            values.push(value);
        }
        Ok(values)
    }
    pub(super) fn variables(&self) -> Result<BTreeMap<String, String>> {
        let mut variables = BTreeMap::from([
            ("n".into(), "pre.100.110".into()),
            ("q".into(), "command.101.120".into()),
            ("unit".into(), "context.102.130".into()),
        ]);
        for j in 0..self.layout.cells() {
            variables.insert(
                self.layout.cell_name(j).to_string(),
                format!("pre.100.{}", 111 + j),
            );
        }
        for (name, node) in &self.helpers {
            variables.insert(name.clone(), policy_text(node)?);
        }
        Ok(variables)
    }
    pub(super) fn check_features(&self, relation: &Relation) -> Result<Value> {
        let alpha = self.layout.alpha(relation)?;
        let vocab = Vocabulary::new(
            self.names.clone(),
            self.domains.clone(),
            vec![],
            &BTreeMap::new(),
        )
        .map_err(|e| gate(e.to_string()))?;
        let events = self.event()?;
        let mut checked = 0;
        let mut event_rows = 0;
        for (s, mapped) in alpha.iter().enumerate() {
            let raw = self.layout.raw(s);
            let cells: Vec<_> = raw[1..]
                .iter()
                .enumerate()
                .map(|(j, c)| {
                    let has = *c != 0;
                    let id = if !has {
                        0
                    } else if self.layout == Layout::T {
                        (c - 1) / 2
                    } else {
                        j as i128
                    };
                    let p = if has { (c - 1) % 2 } else { 0 };
                    let t = if !has {
                        0
                    } else if self.layout == Layout::T {
                        j as i128
                    } else {
                        (c - 1) / 2
                    };
                    (has, id, p, t)
                })
                .collect();
            let mut best = None;
            for cell in &cells {
                if cell.0
                    && best.is_none_or(|b: (bool, i128, i128, i128)| (cell.2, cell.3) < (b.2, b.3))
                {
                    best = Some(*cell);
                }
            }
            let minimum = best.map_or(0, |b| b.1);
            for q in 0..16 {
                let insert = q < 6;
                let cancel = (6..9).contains(&q);
                let change = (9..15).contains(&q);
                let remove = q == 15;
                let id = if insert {
                    q / 2
                } else if cancel {
                    q - 6
                } else if change {
                    (q - 9) / 2
                } else {
                    0
                };
                let p = if insert {
                    q % 2
                } else if change {
                    (q - 9) % 2
                } else {
                    0
                };
                let mut expected = raw.clone();
                expected.extend([
                    q,
                    0,
                    i128::from(insert),
                    i128::from(cancel),
                    i128::from(change),
                    i128::from(remove),
                    id,
                    p,
                    i128::from(cells.iter().any(|c| c.0 && c.1 == id)),
                    cells.iter().filter(|c| c.0).count() as i128,
                    minimum,
                    (raw[0] + 1).min(2),
                    1 + 2 * if self.layout == Layout::T {
                        id
                    } else {
                        raw[0].min(1)
                    } + p,
                ]);
                for cell in &cells {
                    expected.extend([
                        i128::from(cell.0),
                        cell.1,
                        1 + 2 * if self.layout == Layout::T {
                            cell.1
                        } else {
                            cell.3
                        } + p,
                    ]);
                }
                let values = self.values(&raw, q)?;
                if values != expected {
                    return fail(
                        1,
                        format!(
                            "integration-mismatch:features:{}/{s}/{q}",
                            self.layout.name()
                        ),
                    );
                }
                checked += 1;
                if let Some(a) = mapped {
                    let row = &relation.rows[a * 16 + q as usize];
                    if row["class"] == "Accept" {
                        let columns: Vec<_> = values.iter().map(|v| vec![*v]).collect();
                        let env = expr::Env {
                            vocab: &vocab,
                            columns: &columns,
                            keep: None,
                            len: 1,
                        };
                        for (j, event) in events.iter().enumerate() {
                            let got = expr::evaluate(event, &env, &BTreeMap::new())
                                .map_err(|e| gate(e.to_string()))?;
                            if row["outbox"][0]["payload"][j][1].as_i64().map(i128::from)
                                != Some(got[0])
                            {
                                return fail(
                                    1,
                                    format!(
                                        "integration-mismatch:fixed-event:{}/{s}/{q}",
                                        self.layout.name()
                                    ),
                                );
                            }
                        }
                        event_rows += 1;
                    }
                }
            }
        }
        Ok(
            json!({"raw_inputs":checked,"columns":self.names.len(),"valid_accept_events":event_rows,"status":"features-agree"}),
        )
    }
    pub(super) fn validity(&self, root: &str) -> Result<String> {
        let n = var(&format!("{root}.100.110"));
        let mut conditions = Vec::new();
        for j in 0..self.layout.cells() {
            let c = var(&format!("{root}.100.{}", 111 + j));
            let before_n = match self.layout {
                Layout::T => lt(lit(j as i128), n.clone()),
                Layout::I => or(
                    and(lt(c.clone(), lit(3)), lt(lit(0), n.clone())),
                    and(not(lt(c.clone(), lit(3))), lt(lit(1), n.clone())),
                ),
            };
            conditions.push(or(not(present(c)), before_n));
        }
        for i in 0..self.layout.cells() {
            for j in i + 1..self.layout.cells() {
                let a = var(&format!("{root}.100.{}", 111 + i));
                let b = var(&format!("{root}.100.{}", 111 + j));
                let distinct = match self.layout {
                    Layout::T => {
                        let same = (0..3)
                            .map(|d| {
                                and(
                                    and(
                                        not(lt(a.clone(), lit(1 + 2 * d))),
                                        lt(a.clone(), lit(3 + 2 * d)),
                                    ),
                                    and(
                                        not(lt(b.clone(), lit(1 + 2 * d))),
                                        lt(b.clone(), lit(3 + 2 * d)),
                                    ),
                                )
                            })
                            .reduce(or)
                            .ok_or_else(|| malformed("identity-domains"))?;
                        not(same)
                    }
                    Layout::I => or(
                        and(lt(a.clone(), lit(3)), not(lt(b.clone(), lit(3)))),
                        and(not(lt(a.clone(), lit(3))), lt(b.clone(), lit(3))),
                    ),
                };
                conditions.push(or(not(and(present(a), present(b))), distinct));
            }
        }
        policy_text(
            &conditions
                .into_iter()
                .reduce(and)
                .ok_or_else(|| malformed("empty-validity"))?,
        )
    }
    pub(super) fn sketches(&self) -> Result<Vec<Expr>> {
        let predicates = [
            "insert && target_present",
            "insert && size == 2",
            "insert && n == 2",
            "!(!cancel && !change) && !target_present",
            "remove && size == 0",
        ];
        let mut selector = "5".to_string();
        for (j, predicate) in predicates.iter().enumerate().rev() {
            selector = format!("?{{false | true | {predicate}}} ? {j} : ({selector})");
        }
        let mut text = vec![selector, "?{n | next_n | insert ? next_n : n}".into()];
        for j in 0..self.layout.cells() {
            let destination = match self.layout {
                Layout::T => format!("n == {j}"),
                Layout::I => format!("requested_id == {j}"),
            };
            let cell = self.layout.cell_name(j);
            text.push(format!("insert && {destination} ? ?{{{cell} | 0 | insert_token}} : (!(!cancel && !remove) && present{j} && identity{j} == (remove ? minimum_id : requested_id) ? ?{{{cell} | 0}} : (change && present{j} && identity{j} == requested_id ? ?{{{cell} | 0 | change_token{j}}} : {cell}))"));
        }
        text.iter()
            .map(|t| {
                expr::parse(
                    t,
                    0,
                    Allow {
                        holes: true,
                        params: false,
                        keep: false,
                    },
                )
                .map_err(|e| gate(e.to_string()))
            })
            .collect()
    }
    pub(super) fn event(&self) -> Result<Vec<Expr>> {
        [
            "insert ? 0 : cancel ? 1 : change ? 2 : 3",
            "remove ? minimum_id : requested_id",
            "!(!insert && !change) ? requested_priority : 2",
            "insert ? n : 2",
        ]
        .iter()
        .map(|s| {
            expr::parse(
                s,
                0,
                Allow {
                    holes: false,
                    params: false,
                    keep: false,
                },
            )
            .map_err(|e| gate(e.to_string()))
        })
        .collect()
    }
    pub(super) fn grammar(&self, alpha: &[Option<usize>]) -> Result<Value> {
        let sketches = self.sketches()?;
        let mut literals: std::collections::BTreeSet<_> = self
            .helpers
            .values()
            .chain(sketches.iter())
            .chain(self.event()?.iter())
            .flat_map(expr::constants)
            .collect();
        literals.extend(0..=self.layout.radix());
        let outputs:Vec<_>=sketches.iter().enumerate().map(|(j,node)|json!({"output":j,"sketch":expr::print_symbolic(node),
            "holes":expr::holes_of(node).iter().enumerate().map(|(k,h)|json!({"id":format!("out{j}/hole{k}"),"expression":expr::print_symbolic(h)})).collect::<Vec<_>>()})).collect();
        let codec = json!({"layout":self.layout.name(),"fields":self.layout.bounds(),"raw_order":"state-major-last-field-fastest","alpha":alpha});
        Ok(
            json!({"schema":"zeno-fcis/priority-construction-grammar/1","revision":REVISION,"layout":self.layout.name(),
            "fields":self.layout.bounds().iter().enumerate().map(|(j,b)|json!({"id":110+j,"min":0,"max":b})).collect::<Vec<_>>(),
            "columns":self.names.iter().zip(&self.domains).map(|(name,domain)|{let(lo,hi)=domain.bounds();json!({"name":name,"kind":if *domain==Domain::Bool{"Bool"}else{"Int"},"min":lo,"max":hi})}).collect::<Vec<_>>(),
            "codec_sha256":sha(&canonical(&codec)?),"alpha_sha256":sha(&canonical(&alpha)?),
            "helpers":self.helpers.iter().map(|(name,e)|Ok((name.clone(),policy_text(e)?))).collect::<Result<BTreeMap<_,_>>>()?,
            "laws":{"499":{"kind":"DecisionConformance","scope":"any","formula":self.validity("pre")?},"500":{"kind":"StateInvariant","scope":"commit, genesis","formula":self.validity("post")?}},
            "cases":[{"class":"Reject","reason":201},{"class":"Reject","reason":202},{"class":"Reject","reason":203},{"class":"Reject","reason":204},{"class":"Reject","reason":205},{"class":"Accept","when":"true"}],
            "outputs":outputs,"literals":literals,
            "limits":{"max_nodes":224,"max_inputs":6,"max_outputs":if self.layout==Layout::T{8}else{9},"max_attempts":32,"max_examples":48,"max_assignments":20000,"max_work":50000000,"max_layout_work":1600000000,"sample":512,"seed":0},
            "syntactic_assignments":if self.layout==Layout::T{236196}else{4251528}}),
        )
    }
}
