//! Pure quantity admission and exact scale lowering, before graph erasure.
//! All scratch is invocation-owned; no evaluator, policy, or caller is mutated.

use super::declarations::{Declarations, Form, Kind};
use super::expr::{Ast, Binary, Rounding};
use zeno_fcis_spec::Quantity;

pub const MAX_CHAIN: u8 = 16;
const MAX_VISITS: usize = 65_536;

struct Value {
    ast: Ast,
    quantity: Option<Quantity>,
    bounds: Option<(i128, i128)>,
    steps: u8,
    literal: bool,
    boolean: bool,
    unsigned: bool,
    unsigned_bounds: Option<(u128, u128)>,
}

pub fn typed(declarations: &Declarations) -> bool {
    declarations.types.values().any(|t| t.quantity.is_some())
}

/// Check the destination before erasing annotations. Literals are raw atoms
/// of the declared destination, not casts of an already typed expression.
pub fn assignment(declarations: &Declarations, ast: &Ast, destination: u32) -> Result<(), String> {
    if !typed(declarations) && !declarations.unsigned() {
        return Ok(());
    }
    let mut fuel = MAX_VISITS;
    let value = infer(declarations, ast, &mut fuel, true)?;
    if value.unsigned
        != matches!(
            declarations.kind(destination).map_err(|e| e.to_string())?,
            Kind::U128 { .. }
        )
    {
        return Err("U128AssignmentKindMismatch".into());
    }
    if typed(declarations) {
        let target = &declarations
            .get(destination)
            .map_err(|e| e.to_string())?
            .quantity;
        if !value.literal && &value.quantity != target {
            return Err("assignment unit, scale or width mismatch".into());
        }
        if value.boolean
            != matches!(
                declarations.kind(destination).map_err(|e| e.to_string())?,
                Kind::Bool
            )
        {
            return Err("assignment scalar kind mismatch".into());
        }
    }
    Ok(())
}

pub fn condition(declarations: &Declarations, ast: &Ast) -> Result<(), String> {
    if !typed(declarations) && !declarations.unsigned() {
        return Ok(());
    }
    let mut fuel = MAX_VISITS;
    let value = infer(declarations, ast, &mut fuel, true)?;
    if typed(declarations) && !value.boolean {
        return Err("quantity cannot be used as a boolean".into());
    }
    Ok(())
}

pub fn lower(declarations: &Declarations, ast: &Ast) -> Result<Ast, String> {
    lower_with_strict(declarations, ast, false)
}

/// Policy /4 opts into strict inference even without unsigned declarations.
/// Legacy callers retain declaration-based inference through `lower`.
pub fn lower_with_strict(
    declarations: &Declarations,
    ast: &Ast,
    strict: bool,
) -> Result<Ast, String> {
    let mut fuel = MAX_VISITS;
    let strict = strict || typed(declarations) || declarations.unsigned();
    Ok(infer(declarations, ast, &mut fuel, strict)?.ast)
}

fn numeric(value: &Value) -> Result<&Quantity, String> {
    value
        .quantity
        .as_ref()
        .ok_or_else(|| "quantity arithmetic requires integer operands".into())
}

fn same(a: &Value, b: &Value) -> Result<Option<Quantity>, String> {
    if a.unsigned != b.unsigned {
        return Err("U128KindMismatch".into());
    }
    // An unbound literal can take the other operand's raw atom type.
    if a.literal && b.quantity.is_some() {
        return Ok(b.quantity.clone());
    }
    if b.literal && a.quantity.is_some() {
        return Ok(a.quantity.clone());
    }
    if a.quantity != b.quantity || a.boolean != b.boolean {
        return Err("unit, scale or width mismatch".into());
    }
    Ok(a.quantity.clone())
}

fn exponent(value: i16) -> Result<u8, String> {
    u8::try_from(value)
        .ok()
        .filter(|p| *p <= 38)
        .ok_or_else(|| "derived scale outside e0..e38".into())
}

fn combine(a: &Value, b: &Value, divide: bool) -> Result<Quantity, String> {
    let a = numeric(a)?;
    let b = numeric(b)?;
    let p = exponent(
        i16::from(a.scale())
            + if divide {
                -i16::from(b.scale())
            } else {
                i16::from(b.scale())
            },
    )?;
    a.combine(b, divide, p)
        .ok_or_else(|| "unit product exceeds representation".into())
}

fn node(
    ast: Ast,
    quantity: Option<Quantity>,
    bounds: Option<(i128, i128)>,
    steps: u8,
    literal: bool,
    boolean: bool,
) -> Result<Value, String> {
    if steps > MAX_CHAIN {
        return Err("dependent mul-div chain exceeds 16 steps".into());
    }
    Ok(Value {
        ast,
        quantity,
        bounds,
        steps,
        literal,
        boolean,
        unsigned: false,
        unsigned_bounds: None,
    })
}

fn interval(
    a: Option<(i128, i128)>,
    b: Option<(i128, i128)>,
    op: fn(i128, i128) -> Option<i128>,
) -> Option<(i128, i128)> {
    let (lo, hi) = a?;
    let (low, high) = b?;
    let values = [op(lo, low)?, op(lo, high)?, op(hi, low)?, op(hi, high)?];
    Some((*values.iter().min()?, *values.iter().max()?))
}

fn infer(d: &Declarations, ast: &Ast, fuel: &mut usize, strict: bool) -> Result<Value, String> {
    *fuel = fuel
        .checked_sub(1)
        .ok_or("expanded expression exceeds admission work cap")?;
    let plain = || Some(Quantity::dimensionless());
    match ast {
        Ast::Circuit(c) => {
            let c = c.try_map(|f| {
                let v = infer(d, f, fuel, strict)?;
                if !v.boolean {
                    return Err("circuit leaves must be Boolean".to_owned());
                }
                Ok(v.ast)
            })?;
            node(Ast::Circuit(Box::new(c)), None, None, 0, false, true)
        }
        Ast::U128(n) => mark(
            node(ast.clone(), plain(), None, 0, true, false),
            true,
            Some((*n, *n)),
        ),
        Ast::ToI128(v) | Ast::ToU128(v) => {
            let mut value = infer(d, v, fuel, strict)?;
            let signed = matches!(ast, Ast::ToI128(_));
            if value.boolean || value.unsigned != signed {
                return Err("U128CastKindMismatch".into());
            }
            value.ast = if signed {
                Ast::ToI128(Box::new(value.ast))
            } else {
                Ast::ToU128(Box::new(value.ast))
            };
            value.bounds = None;
            value.unsigned_bounds = None;
            value.unsigned = !signed;
            value.literal = false;
            Ok(value)
        }
        Ast::Bool(_) => node(ast.clone(), None, None, 0, false, true),
        Ast::Int(n) => node(ast.clone(), plain(), Some((*n, *n)), 0, true, false),
        Ast::DecisionReason => node(ast.clone(), plain(), None, 0, false, false),
        Ast::Name(name) => {
            if !strict {
                return node(ast.clone(), plain(), None, 0, false, false);
            }
            let parts: Vec<_> = name.split('.').collect();
            let root = parts
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| format!("untyped quantity name `{name}`"))?;
            if !matches!(parts[0], "pre" | "post" | "command" | "context" | "outbox") {
                return Err(format!("untyped quantity name `{name}`"));
            }
            if parts[0] == "outbox" && parts.len() == 2 {
                return node(ast.clone(), plain(), Some((0, i128::MAX)), 0, false, false);
            }
            let type_id = if parts.len() == 3 {
                let field = parts[2].parse::<u16>().map_err(|_| "invalid field")?;
                d.fields(root)
                    .map_err(|e| e.to_string())?
                    .iter()
                    .find(|f| f.id == field)
                    .ok_or("unknown quantity field")?
                    .type_id
            } else if parts.len() == 2 {
                root
            } else {
                return Err("invalid quantity projection".into());
            };
            let declared = d.get(type_id).map_err(|e| e.to_string())?;
            // An outbox field name binds a sum over matching deliveries.
            // Its checked i128 total need not lie in one payload's bounds.
            let bounds = if parts[0] == "outbox" {
                None
            } else {
                match declared.form {
                    Form::Leaf(super::declarations::Leaf::I128 { min, max }) => Some((min, max)),
                    _ => None,
                }
            };
            node(
                ast.clone(),
                declared.quantity.clone().or_else(|| {
                    if !typed(d) && !matches!(d.kind(type_id).ok(), Some(Kind::Bool)) {
                        plain()
                    } else {
                        None
                    }
                }),
                bounds,
                0,
                false,
                matches!(d.kind(type_id).map_err(|e| e.to_string())?, Kind::Bool),
            )
            .and_then(|v| {
                mark(
                    Ok(v),
                    matches!(
                        declared.form,
                        Form::Leaf(super::declarations::Leaf::U128 { .. })
                    ),
                    if parts[0] == "outbox" {
                        None
                    } else {
                        match declared.form {
                            Form::Leaf(super::declarations::Leaf::U128 { min, max }) => {
                                Some((min, max))
                            }
                            _ => None,
                        }
                    },
                )
            })
        }
        Ast::Neg(inner) | Ast::Not(inner) => {
            let v = infer(d, inner, fuel, strict)?;
            let not = matches!(ast, Ast::Not(_));
            if !not && v.unsigned {
                return Err("U128NegationUnsupported".into());
            }
            if strict && (not && !v.boolean || !not && v.quantity.is_none()) {
                return Err("invalid quantity unary operand".into());
            }
            let bounds = v
                .bounds
                .and_then(|(lo, hi)| Some((hi.checked_neg()?, lo.checked_neg()?)));
            let tree = if not {
                Ast::Not(Box::new(v.ast))
            } else {
                Ast::Neg(Box::new(v.ast))
            };
            node(
                tree,
                if not { None } else { v.quantity },
                bounds,
                v.steps,
                v.literal && !not,
                not,
            )
        }
        Ast::Choose(c, a, b) => {
            let c = infer(d, c, fuel, strict)?;
            let a = infer(d, a, fuel, strict)?;
            let b = infer(d, b, fuel, strict)?;
            if strict && !c.boolean {
                return Err("quantity select condition is not boolean".into());
            }
            let q = if strict {
                same(&a, &b)?
            } else {
                a.quantity.clone()
            };
            let bounds = a
                .bounds
                .zip(b.bounds)
                .map(|((lo, hi), (low, high))| (lo.min(low), hi.max(high)));
            let steps = c.steps.max(a.steps).max(b.steps);
            let boolean = a.boolean;
            let unsigned = a.unsigned;
            let ub = a
                .unsigned_bounds
                .zip(b.unsigned_bounds)
                .map(|((a, b), (c, d))| (a.min(c), b.max(d)));
            node(
                Ast::Choose(Box::new(c.ast), Box::new(a.ast), Box::new(b.ast)),
                q,
                bounds,
                steps,
                false,
                boolean,
            )
            .and_then(|v| mark(Ok(v), unsigned, ub))
        }
        Ast::Binary(op, a, b) => {
            let a = infer(d, a, fuel, strict)?;
            let b = infer(d, b, fuel, strict)?;
            let logic = matches!(op, Binary::And | Binary::Or | Binary::Implies);
            let compare = matches!(
                op,
                Binary::Eq | Binary::Ne | Binary::Lt | Binary::Le | Binary::Gt | Binary::Ge
            );
            if a.unsigned != b.unsigned {
                return Err("U128KindMismatch".into());
            }
            let unsigned = a.unsigned && !compare && !logic;
            let ub = match op {
                Binary::Add => {
                    unsigned_interval(a.unsigned_bounds, b.unsigned_bounds, u128::checked_add)
                }
                Binary::Sub => {
                    unsigned_interval(a.unsigned_bounds, b.unsigned_bounds, u128::checked_sub)
                }
                Binary::Mul => {
                    unsigned_interval(a.unsigned_bounds, b.unsigned_bounds, u128::checked_mul)
                }
                _ => None,
            };
            let mut q = a.quantity.clone();
            if strict {
                if logic {
                    if !a.boolean || !b.boolean {
                        return Err("quantity used as logic".into());
                    }
                    q = None;
                } else if *op == Binary::Mul {
                    q = Some(combine(&a, &b, false)?);
                } else {
                    q = same(&a, &b)?;
                    if !compare && q.is_none() {
                        return Err("integer quantity required".into());
                    }
                }
            }
            let bounds = match op {
                Binary::Add => interval(a.bounds, b.bounds, i128::checked_add),
                Binary::Sub => interval(a.bounds, b.bounds, i128::checked_sub),
                Binary::Mul => interval(a.bounds, b.bounds, i128::checked_mul),
                _ => None,
            };
            let steps = a.steps.max(b.steps);
            node(
                Ast::Binary(*op, Box::new(a.ast), Box::new(b.ast)),
                if compare || logic { None } else { q },
                bounds,
                steps,
                false,
                compare || logic,
            )
            .and_then(|v| mark(Ok(v), unsigned, ub))
        }
        Ast::Div(r, a, b) => {
            let a = infer(d, a, fuel, strict)?;
            let b = infer(d, b, fuel, strict)?;
            if a.unsigned != b.unsigned {
                return Err("U128KindMismatch".into());
            }
            let unsigned = a.unsigned;
            let q = if strict {
                Some(combine(&a, &b, true)?)
            } else {
                plain()
            };
            let steps = a.steps.max(b.steps);
            node(
                Ast::Div(*r, Box::new(a.ast), Box::new(b.ast)),
                q,
                None,
                steps,
                false,
                false,
            )
            .and_then(|v| mark(Ok(v), unsigned, None))
        }
        Ast::MulDiv(r, a, b, h) => fused(d, *r, false, (a, b, h), None, fuel, strict),
        Ast::SignedMulDivFloor(a, b, h) => {
            fused(d, Rounding::Floor, true, (a, b, h), None, fuel, strict)
        }
        Ast::ScaledMulDiv(r, signed, a, b, h, q) => {
            if !typed(d) {
                return Err("scaled call requires declared units and scales".into());
            }
            fused(d, *r, *signed, (a, b, h), Some(*q), fuel, strict)
        }
        Ast::RepeatMulDiv(signed, x, a, h, p) => {
            if *p > 16 {
                return Err("repeat count outside 0..16".into());
            }
            let x = infer(d, x, fuel, strict)?;
            let a = infer(d, a, fuel, strict)?;
            let h = infer(d, h, fuel, strict)?;
            let unsigned = x.unsigned;
            if x.unsigned != a.unsigned || x.unsigned != h.unsigned || (*signed && unsigned) {
                return Err("U128KindMismatch".into());
            }
            let steps = x
                .steps
                .max(a.steps)
                .max(h.steps)
                .checked_add(*p)
                .ok_or("chain length overflow")?;
            let mut q = x.quantity.clone();
            if strict {
                let xq = numeric(&x)?;
                let aq = numeric(&a)?;
                let hq = numeric(&h)?;
                let scale = exponent(
                    i16::from(xq.scale())
                        + i16::from(*p) * (i16::from(aq.scale()) - i16::from(hq.scale())),
                )?;
                let mut result = xq.clone();
                for _ in 0..*p {
                    result = result
                        .combine(aq, false, 0)
                        .and_then(|v| v.combine(hq, true, scale))
                        .ok_or("unit recurrence overflow")?;
                }
                // Each prefix must also be a legal declared exponent.
                for i in 0..=*p {
                    exponent(
                        i16::from(xq.scale())
                            + i16::from(i) * (i16::from(aq.scale()) - i16::from(hq.scale())),
                    )?;
                }
                q = Some(result);
            }
            node(
                Ast::RepeatMulDiv(
                    *signed,
                    Box::new(x.ast),
                    Box::new(a.ast),
                    Box::new(h.ast),
                    *p,
                ),
                q,
                None,
                steps,
                false,
                false,
            )
            .and_then(|v| mark(Ok(v), unsigned, None))
        }
        Ast::ProductsGe(args) => {
            let values = args
                .iter()
                .map(|a| infer(d, a, fuel, strict))
                .collect::<Result<Vec<_>, _>>()?;
            if values.iter().any(|v| v.unsigned != values[0].unsigned) {
                return Err("U128KindMismatch".into());
            }
            if strict {
                let half = values.len() / 2;
                let product = |slice: &[Value]| -> Result<Quantity, String> {
                    let mut q = Quantity::dimensionless();
                    for v in slice {
                        let v = numeric(v)?;
                        q = q
                            .combine(
                                v,
                                false,
                                exponent(i16::from(q.scale()) + i16::from(v.scale()))?,
                            )
                            .ok_or("unit product overflow")?;
                    }
                    Ok(q)
                };
                if product(&values[..half])? != product(&values[half..])? {
                    return Err("product comparison unit, scale or width mismatch".into());
                }
            }
            let steps = values.iter().map(|v| v.steps).max().unwrap_or(0);
            node(
                Ast::ProductsGe(values.into_iter().map(|v| v.ast).collect()),
                None,
                None,
                steps,
                false,
                true,
            )
        }
    }
}

fn fused(
    d: &Declarations,
    r: Rounding,
    signed: bool,
    (a, b, h): (&Ast, &Ast, &Ast),
    target: Option<u8>,
    fuel: &mut usize,
    strict: bool,
) -> Result<Value, String> {
    if signed && r != Rounding::Floor {
        return Err("signed fused ceiling is unsupported".into());
    }
    let a = infer(d, a, fuel, strict)?;
    let b = infer(d, b, fuel, strict)?;
    let h = infer(d, h, fuel, strict)?;
    let unsigned = a.unsigned;
    if a.unsigned != b.unsigned || a.unsigned != h.unsigned || (signed && unsigned) {
        return Err("U128KindMismatch".into());
    }
    let steps = a
        .steps
        .max(b.steps)
        .max(h.steps)
        .checked_add(1)
        .ok_or("chain length overflow")?;
    let mut q = Some(Quantity::dimensionless());
    let mut k = 0i16;
    if typed(d) {
        let aq = numeric(&a)?;
        let bq = numeric(&b)?;
        let hq = numeric(&h)?;
        let raw = i16::from(aq.scale()) + i16::from(bq.scale()) - i16::from(hq.scale());
        let scale = match target {
            Some(scale) => scale,
            None => exponent(raw)?,
        };
        if scale > 38 {
            return Err("target scale outside e0..e38".into());
        }
        k = i16::from(scale) - raw;
        q = Some(
            aq.combine(bq, false, scale)
                .and_then(|v| v.combine(hq, true, scale))
                .ok_or("unit quotient overflow")?,
        );
    }
    let mut aa = a.ast.clone();
    let mut bb = b.ast.clone();
    let mut hh = h.ast.clone();
    if k != 0 {
        let power = u32::from(k.unsigned_abs());
        let factor = 10i128
            .checked_pow(power)
            .filter(|_| power <= 38)
            .ok_or("unsupported scale lowering: remaining exponent exceeds 38")?;
        let fits = |v: &Value| {
            if unsigned {
                unsigned_interval(
                    v.unsigned_bounds,
                    Some((factor as u128, factor as u128)),
                    u128::checked_mul,
                )
                .is_some()
            } else {
                interval(v.bounds, Some((factor, factor)), i128::checked_mul).is_some()
            }
        };
        let scale = |v| {
            Ast::Binary(
                Binary::Mul,
                Box::new(v),
                Box::new(if unsigned {
                    Ast::U128(factor as u128)
                } else {
                    Ast::Int(factor)
                }),
            )
        };
        if k < 0 {
            if !fits(&h) {
                return Err(
                    "unsupported scale lowering: denominator fit is not established".into(),
                );
            }
            hh = scale(hh);
        } else if fits(&a) {
            aa = scale(aa);
        } else if fits(&b) {
            bb = scale(bb);
        } else {
            return Err("unsupported scale lowering: numerator fit is not established".into());
        }
    }
    let ast = if signed {
        Ast::SignedMulDivFloor(Box::new(aa), Box::new(bb), Box::new(hh))
    } else {
        Ast::MulDiv(r, Box::new(aa), Box::new(bb), Box::new(hh))
    };
    mark(node(ast, q, None, steps, false, false), unsigned, None)
}

fn mark(
    value: Result<Value, String>,
    unsigned: bool,
    bounds: Option<(u128, u128)>,
) -> Result<Value, String> {
    let mut value = value?;
    value.unsigned = unsigned;
    value.unsigned_bounds = bounds;
    Ok(value)
}
fn unsigned_interval(
    a: Option<(u128, u128)>,
    b: Option<(u128, u128)>,
    op: fn(u128, u128) -> Option<u128>,
) -> Option<(u128, u128)> {
    let (a, bounds) = a?;
    let (c, d) = b?;
    let v = [op(a, c)?, op(a, d)?, op(bounds, c)?, op(bounds, d)?];
    Some((*v.iter().min()?, *v.iter().max()?))
}
