//! The family door: one program for every checked instance of a
//! parameterized family, and transfer of a program between instances.
//!
//! A family program is one expression per output over field names and
//! family parameters (`$C`, `$K`, ...), possibly with holes. For each
//! instance, [`prepare`] binds and folds the parameters and, when holes
//! remain, completes them with the sketch solver. The `loop family` command
//! judges each instance's program with `transform check` and its replay.
//! [`fit`] finds, for every hole, a form `a * $P + b` (or a constant) that
//! reproduces its solved value on the fitted instances; [`apply_forms`] lifts
//! the sketch to a parametric program, which is judged again on every
//! instance, including those it was not fitted on. Coverage is always "the
//! checked instances", never every parameter value.
//!
//! [`transfer`] moves a concrete program from one instance to another:
//! directly; else with the constants near a changed parameter opened as
//! choice holes {unchanged, shifted with the parameter}; else by repairing
//! the direct transfer.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Program;

use crate::expr::{Allow, Expr, Fill, Hole, bind_params, fold, has_holes, parse, substitute};
use crate::lower::lower_outputs;
use crate::repair::{self, Status as RepairStatus, first_difference};
use crate::sketch::{self, SketchResult, Status as SketchStatus};
use crate::subject::Subject;
use crate::{Refusal, Result};

/// Coefficients tried for `a * $P + b`.
pub const FORM_COEFFICIENTS: [i128; 3] = [1, -1, 2];
/// How far from `a * P` a constant may sit to be opened in a transfer.
pub const NEAR: i128 = 2;

/// Binds an instance's parameters and folds literal arithmetic.
pub fn bind(outputs: &[Expr], parameters: &BTreeMap<String, i128>) -> Result<Vec<Expr>> {
    outputs
        .iter()
        .map(|node| fold(&bind_params(node, parameters)?))
        .collect()
}

/// One instance's untrusted candidate.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The bound expressions (holes still open).
    pub bound: Vec<Expr>,
    /// The sketch result, when the bound program had holes.
    pub sketch: Option<SketchResult>,
    /// The program to judge, when one was produced.
    pub program: Option<Program>,
    /// Why no program was produced.
    pub refusal: Option<Refusal>,
}

/// Binds the family program for one instance and, when holes remain,
/// completes them. A program is returned only when it lowered.
#[must_use]
pub fn prepare(outputs: &[Expr], subject: &Subject, settings: &sketch::Settings) -> Prepared {
    let bound = match bind(outputs, &subject.parameters) {
        Ok(bound) => bound,
        Err(refusal) => {
            return Prepared {
                bound: Vec::new(),
                sketch: None,
                program: None,
                refusal: Some(refusal),
            };
        }
    };
    if bound.iter().any(has_holes) {
        let result = sketch::solve(subject, &bound, settings);
        let program = result.program.clone();
        let refusal = if result.status == SketchStatus::Refused {
            result.reason.clone().map(Refusal)
        } else {
            None
        };
        return Prepared {
            bound,
            sketch: Some(result),
            program,
            refusal,
        };
    }
    let checked = bound
        .iter()
        .try_for_each(|node| subject.vocab.check_names(node));
    match checked.and_then(|()| lower_outputs(&bound, &subject.vocab, &subject.original)) {
        Ok(program) => Prepared {
            bound,
            sketch: None,
            program: Some(program),
            refusal: None,
        },
        Err(refusal) => Prepared {
            bound,
            sketch: None,
            program: None,
            refusal: Some(refusal),
        },
    }
}

/// A fitted form for one hole.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Form {
    /// The same value on every fitted instance.
    Constant(i128),
    /// `a * $parameter + b`.
    Linear {
        /// The parameter.
        parameter: String,
        /// Its coefficient: 1, -1 or 2.
        a: i128,
        /// The offset.
        b: i128,
    },
}

impl Form {
    /// The form in the expression language.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Constant(value) => format!("{value}"),
            Self::Linear { parameter, a, b } => {
                let term = match a {
                    1 => format!("${parameter}"),
                    -1 => format!("0 - ${parameter}"),
                    _ => format!("${parameter} + ${parameter}"),
                };
                match b.cmp(&0) {
                    core::cmp::Ordering::Equal => term,
                    core::cmp::Ordering::Greater => format!("{term} + {b}"),
                    core::cmp::Ordering::Less => format!("{term} - {}", b.unsigned_abs()),
                }
            }
        }
    }

    /// The form as an expression.
    pub fn expr(&self) -> Result<Expr> {
        match self {
            Self::Constant(value) => Ok(Expr::Int(*value)),
            Self::Linear { .. } => parse(&self.text(), 0, Allow::ALL),
        }
    }
}

/// Fill values that a form can explain: constants and choice indices.
fn integer(fill: &Fill) -> Option<i128> {
    match fill {
        Fill::Const(value) => Some(*value),
        Fill::Choice(index) => i128::try_from(*index).ok(),
        Fill::Ast(_) => None,
    }
}

fn form(points: &[(&BTreeMap<String, i128>, i128)]) -> Option<Form> {
    let first = points.first()?;
    if points.iter().all(|(_, v)| *v == first.1) {
        return Some(Form::Constant(first.1));
    }
    for name in first.0.keys() {
        for a in FORM_COEFFICIENTS {
            let mut offsets = points.iter().map(|(p, v)| {
                p.get(name)
                    .and_then(|x| a.checked_mul(*x))
                    .and_then(|ax| v.checked_sub(ax))
            });
            let Some(Some(b)) = offsets.next() else {
                continue;
            };
            if offsets.all(|other| other == Some(b)) {
                return Some(Form::Linear {
                    parameter: name.clone(),
                    a,
                    b,
                });
            }
        }
    }
    None
}

/// One fitted instance: its parameter values and its hole fills.
pub type Sample = (BTreeMap<String, i128>, BTreeMap<usize, Fill>);

/// For each hole, a form reproducing its value on every sample
/// (parameters, fills). A hole filled with an expression, missing on some
/// sample, or explained by no single form gets `None`.
#[must_use]
pub fn fit(samples: &[Sample]) -> BTreeMap<usize, Option<Form>> {
    let mut holes: Vec<usize> = samples
        .iter()
        .flat_map(|(_, h)| h.keys().copied())
        .collect();
    holes.sort_unstable();
    holes.dedup();
    let mut out = BTreeMap::new();
    for k in holes {
        let points: Option<Vec<(&BTreeMap<String, i128>, i128)>> = samples
            .iter()
            .map(|(p, h)| h.get(&k).and_then(integer).map(|v| (p, v)))
            .collect();
        out.insert(k, points.and_then(|points| form(&points)));
    }
    out
}

/// The sketch with each constant or expression hole replaced by its form
/// (choice holes stay open).
pub fn apply_forms(node: &Expr, forms: &BTreeMap<usize, Option<Form>>) -> Result<Expr> {
    substitute(node, &mut |n| match n {
        Expr::Hole(_, Hole::Choice(_)) => Ok(None),
        Expr::Hole(k, _) => match forms.get(k) {
            Some(Some(form)) => form.expr().map(Some),
            _ => Ok(None),
        },
        _ => Ok(None),
    })
}

/// A constant opened by [`sketchify`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Opened {
    /// The source constant.
    pub value: i128,
    /// The choices: unchanged first, then shifted values.
    pub options: Vec<i128>,
}

/// Opens the source program's constants within [`NEAR`] of `a * P` (for `a`
/// in 1, 2, -1 and a parameter `P` that differs at the target) as choice
/// holes {unchanged, shifted with `P`}.
pub fn sketchify(
    outputs: &[Expr],
    source: &BTreeMap<String, i128>,
    target: &BTreeMap<String, i128>,
) -> Result<(Vec<Expr>, BTreeMap<usize, Opened>)> {
    let deltas: Vec<(&String, i128, i128)> = source
        .iter()
        .filter_map(|(name, value)| {
            let to = target.get(name)?;
            (to != value).then(|| (name, *value, to.wrapping_sub(*value)))
        })
        .collect();
    let mut opened = BTreeMap::new();
    let mut counter = 0usize;
    let mut sketched = Vec::with_capacity(outputs.len());
    for node in outputs {
        sketched.push(substitute(node, &mut |n| {
            let Expr::Int(v) = n else {
                return Ok(None);
            };
            let mut options = vec![*v];
            for (_, value, delta) in &deltas {
                for a in [1i128, 2, -1] {
                    let centre = a.wrapping_mul(*value);
                    if v.wrapping_sub(centre).unsigned_abs() <= NEAR.unsigned_abs() {
                        let shifted = v.wrapping_add(a.wrapping_mul(*delta));
                        if !options.contains(&shifted) {
                            options.push(shifted);
                        }
                    }
                }
            }
            if options.len() == 1 {
                return Ok(None);
            }
            let k = counter;
            counter += 1;
            opened.insert(
                k,
                Opened {
                    value: *v,
                    options: options.clone(),
                },
            );
            Ok(Some(Expr::Hole(
                k,
                Hole::Choice(options.into_iter().map(Expr::Int).collect()),
            )))
        })?);
    }
    Ok((sketched, opened))
}

/// How a transfer succeeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferStatus {
    /// The program carried over unchanged.
    Direct,
    /// Opening constants near a parameter completed it.
    Sketch,
    /// Repairing the direct transfer did.
    Repair,
    /// None of the three.
    Failed,
}

impl TransferStatus {
    /// The status's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Sketch => "sketch",
            Self::Repair => "repair",
            Self::Failed => "failed",
        }
    }
}

/// The direct transfer's untrusted verdict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Direct {
    /// Agrees with the target's original everywhere.
    Agrees,
    /// Differs, first at this ordinal.
    Differs(usize),
    /// Did not lower.
    Refused(Refusal),
}

/// One transfer.
#[derive(Clone, Debug)]
pub struct Transfer {
    /// How it succeeded.
    pub status: TransferStatus,
    /// The direct transfer.
    pub direct: Direct,
    /// Constants opened as choice holes.
    pub opened: BTreeMap<usize, Opened>,
    /// The sketch, when one ran.
    pub sketch: Option<SketchResult>,
    /// The chosen value per opened constant, when the sketch completed.
    pub choices: Vec<(usize, i128, i128)>,
    /// The repair, when one ran.
    pub repair: Option<repair::Repair>,
    /// The resulting expressions and program, unless failed.
    pub expressions: Vec<Expr>,
    /// The resulting program, unless failed.
    pub program: Option<Program>,
}

/// Moves `outputs` (written for an instance with `source` parameters) to the
/// `target` instance: directly, then by sketch, then by repair.
#[must_use]
pub fn transfer(
    outputs: &[Expr],
    source: &BTreeMap<String, i128>,
    target: &Subject,
    sketch_settings: &sketch::Settings,
    repair_settings: &repair::Settings,
) -> Transfer {
    let mut out = Transfer {
        status: TransferStatus::Failed,
        direct: Direct::Refused(Refusal::new("not-tried")),
        opened: BTreeMap::new(),
        sketch: None,
        choices: Vec::new(),
        repair: None,
        expressions: Vec::new(),
        program: None,
    };
    let resolved: Result<Vec<Expr>> = outputs
        .iter()
        .map(|node| {
            target.vocab.check_names(node)?;
            target.vocab.resolve_variants(node)
        })
        .collect();
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(refusal) => {
            out.direct = Direct::Refused(refusal);
            return out;
        }
    };
    let direct = lower_outputs(&resolved, &target.vocab, &target.original);
    let direct_program = match direct {
        Ok(program) => match first_difference(target, &program) {
            None => {
                out.direct = Direct::Agrees;
                out.status = TransferStatus::Direct;
                out.expressions = resolved;
                out.program = Some(program);
                return out;
            }
            Some(ordinal) => {
                out.direct = Direct::Differs(ordinal);
                Some(program)
            }
        },
        Err(refusal) => {
            out.direct = Direct::Refused(refusal);
            None
        }
    };
    if let Ok((sketched, opened)) = sketchify(outputs, source, &target.parameters)
        && !opened.is_empty()
    {
        let result = sketch::solve(target, &sketched, sketch_settings);
        if result.status == SketchStatus::Completed && result.program.is_some() {
            out.choices = opened
                .iter()
                .filter_map(|(k, o)| match result.holes.get(k) {
                    Some(Fill::Choice(index)) => o.options.get(*index).map(|to| (*k, o.value, *to)),
                    _ => None,
                })
                .collect();
            out.status = TransferStatus::Sketch;
            out.expressions = result.expressions.clone();
            out.program = result.program.clone();
            out.opened = opened;
            out.sketch = Some(result);
            return out;
        }
        out.opened = opened;
        out.sketch = Some(result);
    }
    // The structure may still carry over with a few local fixes.
    if let Some(program) = direct_program
        && let Ok(explained) = repair::explain(target, &resolved, &program, true, repair_settings)
        && let Some(repaired) = explained.repair
    {
        if repaired.status == RepairStatus::Repaired && repaired.program.is_some() {
            out.status = TransferStatus::Repair;
            out.expressions = repaired.expressions.clone();
            out.program = repaired.program.clone();
        }
        out.repair = Some(repaired);
    }
    out
}
