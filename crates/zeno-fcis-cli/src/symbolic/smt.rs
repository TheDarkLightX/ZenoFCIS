//! The SMT-LIB fragment the symbolic checks write: integer and Boolean terms
//! over SMT-LIB's unbounded `Int`, named definitions, and integer constants
//! with declared value sets.
//!
//! A [`Script`] prints as one SMT-LIB 2.6 query, and the same script
//! evaluates at a concrete assignment. The evaluation follows the printed
//! text's meaning, so tests compare the encodings with the library
//! evaluators on sampled points without running a solver.

#[cfg(test)]
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The sort of a term or definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sort {
    Int,
    Bool,
}

/// A term. Integer terms denote unbounded integers, as SMT-LIB's `Int` does.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Term {
    Int(i128),
    Bool(bool),
    /// A declared constant or an earlier definition.
    Name(String),
    Add(Box<Term>, Box<Term>),
    Sub(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
    /// SMT-LIB's `div`: the quotient whose remainder lies in `0..|divisor|`.
    /// Its value for a zero divisor is unspecified, so every user guards the
    /// divisor.
    Div(Box<Term>, Box<Term>),
    Eq(Box<Term>, Box<Term>),
    Lt(Box<Term>, Box<Term>),
    Le(Box<Term>, Box<Term>),
    Not(Box<Term>),
    And(Vec<Term>),
    Or(Vec<Term>),
    Ite(Box<Term>, Box<Term>, Box<Term>),
}

impl Term {
    pub(crate) fn name(name: &str) -> Self {
        Self::Name(name.to_owned())
    }

    pub(crate) fn add(left: Self, right: Self) -> Self {
        Self::Add(Box::new(left), Box::new(right))
    }

    pub(crate) fn sub(left: Self, right: Self) -> Self {
        Self::Sub(Box::new(left), Box::new(right))
    }

    pub(crate) fn mul(left: Self, right: Self) -> Self {
        Self::Mul(Box::new(left), Box::new(right))
    }

    /// The quotient rounded toward negative infinity: `div` for a positive
    /// divisor, and `floor(a / b) = floor(-a / -b)` otherwise.
    pub(crate) fn floor_div(left: Self, right: Self) -> Self {
        let negated = |term: &Self| Self::sub(Self::Int(0), term.clone());
        Self::ite(
            Self::lt(Self::Int(0), right.clone()),
            Self::Div(Box::new(left.clone()), Box::new(right.clone())),
            Self::Div(Box::new(negated(&left)), Box::new(negated(&right))),
        )
    }

    /// The quotient rounded toward positive infinity: `-floor(-a / b)`.
    pub(crate) fn ceil_div(left: Self, right: Self) -> Self {
        Self::sub(
            Self::Int(0),
            Self::floor_div(Self::sub(Self::Int(0), left), right),
        )
    }

    pub(crate) fn eq(left: Self, right: Self) -> Self {
        Self::Eq(Box::new(left), Box::new(right))
    }

    pub(crate) fn lt(left: Self, right: Self) -> Self {
        Self::Lt(Box::new(left), Box::new(right))
    }

    pub(crate) fn le(left: Self, right: Self) -> Self {
        Self::Le(Box::new(left), Box::new(right))
    }

    pub(crate) fn not(inner: Self) -> Self {
        match inner {
            Self::Bool(value) => Self::Bool(!value),
            Self::Not(inner) => *inner,
            inner => Self::Not(Box::new(inner)),
        }
    }

    /// The conjunction, without literal `true` items; `false` absorbs it.
    pub(crate) fn and(items: impl IntoIterator<Item = Self>) -> Self {
        let mut kept = Vec::new();
        for item in items {
            match item {
                Self::Bool(true) => {}
                Self::Bool(false) => return Self::Bool(false),
                item => kept.push(item),
            }
        }
        match kept.len() {
            0 => Self::Bool(true),
            1 => kept.pop().unwrap_or(Self::Bool(true)),
            _ => Self::And(kept),
        }
    }

    /// The disjunction, without literal `false` items; `true` absorbs it.
    pub(crate) fn or(items: impl IntoIterator<Item = Self>) -> Self {
        let mut kept = Vec::new();
        for item in items {
            match item {
                Self::Bool(false) => {}
                Self::Bool(true) => return Self::Bool(true),
                item => kept.push(item),
            }
        }
        match kept.len() {
            0 => Self::Bool(false),
            1 => kept.pop().unwrap_or(Self::Bool(false)),
            _ => Self::Or(kept),
        }
    }

    pub(crate) fn ite(condition: Self, then: Self, otherwise: Self) -> Self {
        match condition {
            Self::Bool(true) => then,
            Self::Bool(false) => otherwise,
            condition => Self::Ite(Box::new(condition), Box::new(then), Box::new(otherwise)),
        }
    }

    /// `min <= term <= max`.
    pub(crate) fn within(term: &Self, min: i128, max: i128) -> Self {
        Self::and([
            Self::le(Self::Int(min), term.clone()),
            Self::le(term.clone(), Self::Int(max)),
        ])
    }

    /// The term is one of `values`.
    pub(crate) fn member(term: &Self, values: &[i128]) -> Self {
        Self::or(
            values
                .iter()
                .map(|value| Self::eq(term.clone(), Self::Int(*value))),
        )
    }

    fn write(&self, out: &mut String) {
        let list = |operator: &str, items: &[&Self], out: &mut String| {
            out.push('(');
            out.push_str(operator);
            for item in items {
                out.push(' ');
                item.write(out);
            }
            out.push(')');
        };
        match self {
            Self::Int(value) if *value < 0 => {
                let _ = write!(out, "(- {})", value.unsigned_abs());
            }
            Self::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Name(name) => out.push_str(name),
            Self::Add(a, b) => list("+", &[a, b], out),
            Self::Sub(a, b) => list("-", &[a, b], out),
            Self::Mul(a, b) => list("*", &[a, b], out),
            Self::Div(a, b) => list("div", &[a, b], out),
            Self::Eq(a, b) => list("=", &[a, b], out),
            Self::Lt(a, b) => list("<", &[a, b], out),
            Self::Le(a, b) => list("<=", &[a, b], out),
            Self::Not(a) => list("not", &[a], out),
            Self::And(items) => list("and", &items.iter().collect::<Vec<_>>(), out),
            Self::Or(items) => list("or", &items.iter().collect::<Vec<_>>(), out),
            Self::Ite(c, a, b) => list("ite", &[c, a, b], out),
        }
    }

    /// Evaluates the term; `None` when a value it needs is unknown.
    #[cfg(test)]
    fn evaluate(&self, values: &BTreeMap<String, Value>) -> Option<Value> {
        let int = |term: &Self| match term.evaluate(values)? {
            Value::Int(wide) => Some(wide),
            Value::Bool(_) => None,
        };
        let boolean = |term: &Self| match term.evaluate(values)? {
            Value::Bool(value) => Some(value),
            Value::Int(_) => None,
        };
        Some(match self {
            Self::Int(value) => Value::Int(Wide::Exact(*value)),
            Self::Bool(value) => Value::Bool(*value),
            Self::Name(name) => *values.get(name)?,
            Self::Add(a, b) => Value::Int(int(a)?.add(int(b)?)?),
            Self::Sub(a, b) => Value::Int(int(a)?.add(int(b)?.negate()?)?),
            Self::Mul(a, b) => Value::Int(int(a)?.mul(int(b)?)?),
            Self::Div(a, b) => Value::Int(int(a)?.div(int(b)?)?),
            Self::Eq(a, b) => Value::Bool(match (a.evaluate(values)?, b.evaluate(values)?) {
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::Int(a), Value::Int(b)) => a.compare(b)? == std::cmp::Ordering::Equal,
                _ => return None,
            }),
            Self::Lt(a, b) => Value::Bool(int(a)?.compare(int(b)?)? == std::cmp::Ordering::Less),
            Self::Le(a, b) => Value::Bool(int(a)?.compare(int(b)?)? != std::cmp::Ordering::Greater),
            Self::Not(a) => Value::Bool(!boolean(a)?),
            // Three-valued: a known `false` decides a conjunction even when
            // another item is unknown, as it does for every assignment.
            Self::And(items) => {
                let mut unknown = false;
                for item in items {
                    match boolean(item) {
                        Some(false) => return Some(Value::Bool(false)),
                        Some(true) => {}
                        None => unknown = true,
                    }
                }
                if unknown {
                    return None;
                }
                Value::Bool(true)
            }
            Self::Or(items) => {
                let mut unknown = false;
                for item in items {
                    match boolean(item) {
                        Some(true) => return Some(Value::Bool(true)),
                        Some(false) => {}
                        None => unknown = true,
                    }
                }
                if unknown {
                    return None;
                }
                Value::Bool(false)
            }
            Self::Ite(c, a, b) => {
                if boolean(c)? {
                    a.evaluate(values)?
                } else {
                    b.evaluate(values)?
                }
            }
        })
    }
}

#[cfg(test)]
/// An integer as far as the evaluator can know it: exact within `i128`,
/// otherwise only its side. Every comparison the encodings make with such a
/// value is against an `i128` bound, which its side decides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Wide {
    Exact(i128),
    Above,
    Below,
}

#[cfg(test)]
impl Wide {
    fn from_sum(value: Option<i128>, positive: bool) -> Self {
        match value {
            Some(value) => Self::Exact(value),
            None if positive => Self::Above,
            None => Self::Below,
        }
    }

    fn add(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Exact(a), Self::Exact(b)) => Some(Self::from_sum(a.checked_add(b), a > 0)),
            (Self::Above, Self::Above) => Some(Self::Above),
            (Self::Above, Self::Exact(b)) | (Self::Exact(b), Self::Above) => {
                (b >= 0).then_some(Self::Above)
            }
            (Self::Below, Self::Below) => Some(Self::Below),
            (Self::Below, Self::Exact(b)) | (Self::Exact(b), Self::Below) => {
                (b <= 0).then_some(Self::Below)
            }
            (Self::Above, Self::Below) | (Self::Below, Self::Above) => None,
        }
    }

    /// The negation: `-(i128::MAX + 1)` is `i128::MIN`, so above negates to
    /// unknown.
    fn negate(self) -> Option<Self> {
        match self {
            Self::Exact(value) => Some(Self::from_sum(value.checked_neg(), value < 0)),
            Self::Below => Some(Self::Above),
            Self::Above => None,
        }
    }

    fn mul(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Exact(a), Self::Exact(b)) => {
                Some(Self::from_sum(a.checked_mul(b), (a < 0) == (b < 0)))
            }
            _ => None,
        }
    }

    /// SMT-LIB's `div`, which is Euclidean division; `i128::MIN / -1` is
    /// above the range, and a numerator beyond it divides only by one.
    fn div(self, divisor: Self) -> Option<Self> {
        match (self, divisor) {
            (_, Self::Exact(0)) => None,
            (Self::Exact(a), Self::Exact(b)) => {
                Some(a.checked_div_euclid(b).map_or(Self::Above, Self::Exact))
            }
            (beyond, Self::Exact(1)) => Some(beyond),
            _ => None,
        }
    }

    fn compare(self, other: Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::{Greater, Less};
        match (self, other) {
            (Self::Exact(a), Self::Exact(b)) => Some(a.cmp(&b)),
            (Self::Above, Self::Exact(_)) | (Self::Exact(_), Self::Below) => Some(Greater),
            (Self::Exact(_), Self::Above) | (Self::Below, Self::Exact(_)) => Some(Less),
            (Self::Above, Self::Below) => Some(Greater),
            (Self::Below, Self::Above) => Some(Less),
            _ => None,
        }
    }
}

/// A term's value at an assignment.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Value {
    Int(Wide),
    Bool(bool),
}

/// The values one declared constant may take.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Values {
    /// Every integer from `min` to `max` inclusive.
    Range { min: i128, max: i128 },
    /// Exactly these integers.
    Set(Vec<i128>),
}

impl Values {
    #[cfg(test)]
    pub(crate) fn contains(&self, value: i128) -> bool {
        match self {
            Self::Range { min, max } => (*min..=*max).contains(&value),
            Self::Set(values) => values.contains(&value),
        }
    }

    fn term(&self, name: &str) -> Term {
        let constant = Term::name(name);
        match self {
            Self::Range { min, max } => Term::within(&constant, *min, *max),
            Self::Set(values) => Term::member(&constant, values),
        }
    }
}

/// One query: declared integer constants with their value sets, named
/// definitions in order, and assertions.
#[derive(Clone, Debug, Default)]
pub(crate) struct Script {
    comments: Vec<String>,
    constants: Vec<(String, Values)>,
    definitions: Vec<(String, Sort, Term)>,
    /// Integer constants asserted equal to a term, so that a model reports
    /// the term's value.
    observations: Vec<(String, Term)>,
    assertions: Vec<Term>,
}

impl Script {
    /// A comment line at the top of the query.
    pub(crate) fn comment(&mut self, text: &str) {
        self.comments.extend(text.lines().map(str::to_owned));
    }

    /// Declares an integer constant restricted to `values`. Solvers print
    /// the value of every declared constant in their models.
    pub(crate) fn constant(&mut self, name: &str, values: Values) -> Term {
        self.constants.push((name.to_owned(), values));
        Term::name(name)
    }

    /// Defines `name` as `term` and returns a reference to it.
    pub(crate) fn define(&mut self, name: &str, sort: Sort, term: Term) -> Term {
        self.definitions.push((name.to_owned(), sort, term));
        Term::name(name)
    }

    pub(crate) fn assert(&mut self, term: Term) {
        self.assertions.push(term);
    }

    /// Declares an integer constant equal to `term`, so that a model reports
    /// the term's value for replay to compare with the library's.
    pub(crate) fn observe(&mut self, name: &str, term: Term) {
        self.observations.push((name.to_owned(), term));
    }

    /// The SMT-LIB text: the constants, their value sets, the definitions,
    /// the assertions and `(check-sat)`.
    pub(crate) fn source(&self) -> String {
        let mut out = String::new();
        for comment in &self.comments {
            let _ = writeln!(out, "; {comment}");
        }
        out.push_str("(set-logic ALL)\n(set-option :produce-models true)\n");
        for (name, values) in &self.constants {
            let _ = writeln!(out, "(declare-const {name} Int)");
            out.push_str("(assert ");
            values.term(name).write(&mut out);
            out.push_str(")\n");
        }
        for (name, _) in &self.observations {
            let _ = writeln!(out, "(declare-const {name} Int)");
        }
        for (name, sort, term) in &self.definitions {
            let sort = match sort {
                Sort::Int => "Int",
                Sort::Bool => "Bool",
            };
            let _ = write!(out, "(define-fun {name} () {sort} ");
            term.write(&mut out);
            out.push_str(")\n");
        }
        for (name, term) in &self.observations {
            let _ = write!(out, "(assert (= {name} ");
            term.write(&mut out);
            out.push_str("))\n");
        }
        for assertion in &self.assertions {
            out.push_str("(assert ");
            assertion.write(&mut out);
            out.push_str(")\n");
        }
        out.push_str("(check-sat)\n");
        out
    }

    /// Every definition's value at an assignment of the constants; `None`
    /// when a constant is unassigned or outside its value set.
    #[cfg(test)]
    pub(crate) fn evaluate(&self, assignment: &BTreeMap<String, i128>) -> Option<Evaluation> {
        let mut values = BTreeMap::new();
        for (name, set) in &self.constants {
            let value = *assignment.get(name)?;
            if !set.contains(value) {
                return None;
            }
            values.insert(name.clone(), Value::Int(Wide::Exact(value)));
        }
        for (name, _, term) in &self.definitions {
            if let Some(value) = term.evaluate(&values) {
                values.insert(name.clone(), value);
            }
        }
        let mut observed = BTreeMap::new();
        for (name, term) in &self.observations {
            if let Some(Value::Int(Wide::Exact(value))) = term.evaluate(&values) {
                values.insert(name.clone(), Value::Int(Wide::Exact(value)));
                observed.insert(name.clone(), value);
            }
        }
        let complete = observed.len() == self.observations.len();
        let holds = self
            .assertions
            .iter()
            .map(|assertion| match assertion.evaluate(&values) {
                Some(Value::Bool(value)) => Some(value),
                _ => None,
            })
            .try_fold(true, |all, value| Some(all && value?))
            .filter(|_| complete);
        Some(Evaluation {
            values,
            observed,
            holds,
        })
    }

    /// The first assignment of the constants, in declaration order with the
    /// last constant fastest, at which every assertion holds, with every
    /// observed value: what a solver's model reports. `None` when the
    /// constants have more than `limit` assignments, or when an assertion is
    /// unknown at one of them. A test's reference solver.
    #[cfg(test)]
    pub(crate) fn search(&self, limit: u128) -> Option<Option<BTreeMap<String, i128>>> {
        let count = self
            .constants
            .iter()
            .try_fold(1u128, |count, (_, values)| {
                let size = match values {
                    Values::Range { min, max } => u128::try_from(max.checked_sub(*min)?)
                        .ok()?
                        .checked_add(1)?,
                    Values::Set(values) => values.len() as u128,
                };
                count.checked_mul(size)
            })?;
        if count > limit {
            return None;
        }
        // A zero product has no assignment; return before any range beside an
        // empty set is collected.
        if count == 0 {
            return Some(None);
        }
        let sets: Vec<Vec<i128>> = self
            .constants
            .iter()
            .map(|(_, values)| match values {
                Values::Range { min, max } => (*min..=*max).collect(),
                Values::Set(values) => values.clone(),
            })
            .collect();
        let mut indices = vec![0usize; sets.len()];
        loop {
            let assignment: BTreeMap<String, i128> = self
                .constants
                .iter()
                .zip(&indices)
                .zip(&sets)
                .map(|(((name, _), index), set)| (name.clone(), set[*index]))
                .collect();
            let evaluation = self.evaluate(&assignment)?;
            if evaluation.holds()? {
                let mut model = assignment;
                model.extend(evaluation.observed);
                return Some(Some(model));
            }
            let mut position = sets.len();
            loop {
                if position == 0 {
                    return Some(None);
                }
                position -= 1;
                indices[position] += 1;
                if indices[position] < sets[position].len() {
                    break;
                }
                indices[position] = 0;
            }
        }
    }
}

/// A script evaluated at one assignment.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct Evaluation {
    values: BTreeMap<String, Value>,
    observed: BTreeMap<String, i128>,
    holds: Option<bool>,
}

#[cfg(test)]
impl Evaluation {
    /// Whether every assertion holds; `None` when one is unknown.
    pub(crate) fn holds(&self) -> Option<bool> {
        self.holds
    }

    /// A named value; `None` when unknown.
    pub(crate) fn value(&self, name: &str) -> Option<Value> {
        self.values.get(name).copied()
    }

    /// A named integer known exactly.
    pub(crate) fn int(&self, name: &str) -> Option<i128> {
        match self.value(name)? {
            Value::Int(Wide::Exact(value)) => Some(value),
            _ => None,
        }
    }

    /// A named Boolean.
    pub(crate) fn boolean(&self, name: &str) -> Option<bool> {
        match self.value(name)? {
            Value::Bool(value) => Some(value),
            Value::Int(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(script: &Script, values: &[(&str, i128)]) -> Evaluation {
        let assignment = values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value))
            .collect();
        script
            .evaluate(&assignment)
            .unwrap_or_else(|| panic!("assignment outside the declared values"))
    }

    #[test]
    fn rounded_division_prints_and_evaluates_with_floor_and_ceiling() {
        let mut script = Script::default();
        let a = script.constant("a", Values::Range { min: -9, max: 9 });
        let b = script.constant("b", Values::Range { min: -9, max: 9 });
        script.define("f", Sort::Int, Term::floor_div(a.clone(), b.clone()));
        script.define("c", Sort::Int, Term::ceil_div(a, b));
        let source = script.source();
        assert!(
            source.contains("(define-fun f () Int (ite (< 0 b) (div a b) (div (- 0 a) (- 0 b))))")
        );
        assert!(source.contains(
            "(define-fun c () Int (- 0 (ite (< 0 b) (div (- 0 a) b) (div (- 0 (- 0 a)) (- 0 b)))))"
        ));
        for a in -9..=9_i128 {
            for b in (-9..=9_i128).filter(|b| *b != 0) {
                let evaluation = at(&script, &[("a", a), ("b", b)]);
                let floor = (a as f64 / b as f64).floor() as i128;
                let ceil = (a as f64 / b as f64).ceil() as i128;
                assert_eq!(evaluation.int("f"), Some(floor), "{a} / {b}");
                assert_eq!(evaluation.int("c"), Some(ceil), "{a} / {b}");
            }
        }
    }

    #[test]
    fn values_beyond_i128_decide_comparisons_with_bounds_only() {
        let mut script = Script::default();
        let a = script.constant(
            "a",
            Values::Range {
                min: i128::MIN,
                max: i128::MAX,
            },
        );
        let sum = script.define("sum", Sort::Int, Term::add(a.clone(), Term::Int(1)));
        script.define("fits", Sort::Bool, Term::within(&sum, i128::MIN, i128::MAX));
        script.define("again", Sort::Int, Term::sub(sum, a));
        let top = at(&script, &[("a", i128::MAX)]);
        assert_eq!(top.boolean("fits"), Some(false));
        assert_eq!(top.value("again"), None);
        let inside = at(&script, &[("a", 5)]);
        assert_eq!(inside.boolean("fits"), Some(true));
        assert_eq!(inside.int("again"), Some(1));
        // A known false item decides a conjunction with an unknown one.
        script.assert(Term::and([
            Term::name("fits"),
            Term::eq(Term::name("again"), Term::Int(1)),
        ]));
        assert_eq!(at(&script, &[("a", i128::MAX)]).holds(), Some(false));
        assert_eq!(at(&script, &[("a", 5)]).holds(), Some(true));
    }

    #[test]
    fn literals_and_value_sets_print_as_smt_lib() {
        let mut script = Script::default();
        script.comment("one query");
        let x = script.constant("x", Values::Set(vec![150, 152]));
        script.observe("o", Term::sub(x.clone(), Term::Int(i128::MIN)));
        script.assert(Term::not(Term::eq(x, Term::Int(-3))));
        let source = script.source();
        assert_eq!(
            source,
            "; one query\n(set-logic ALL)\n(set-option :produce-models true)\n\
             (declare-const x Int)\n(assert (or (= x 150) (= x 152)))\n(declare-const o Int)\n\
             (assert (= o (- x (- 170141183460469231731687303715884105728))))\n\
             (assert (not (= x (- 3))))\n(check-sat)\n"
        );
        assert!(
            script
                .evaluate(&BTreeMap::from([("x".to_owned(), 151)]))
                .is_none()
        );
    }
}
