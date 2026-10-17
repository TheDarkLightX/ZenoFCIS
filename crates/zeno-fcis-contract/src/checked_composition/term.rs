//! Canonical terms over the scalar program language, for comparing an
//! aggregate's decision program with a leaf's without evaluating either.
//!
//! A term is hash-consed into one arena, so two terms are structurally equal
//! exactly when their IDs are equal. Construction folds constants and
//! simplifies by identities that hold for every input on which evaluation
//! does not trap: `select(true, a, b) = a`, `x == x`, and the like. The
//! identities `true && x = x`, `x && x = x` and `!!x = x` are applied only
//! to a Boolean `x`, whose value is 0 or 1: the evaluator's conjunction is 1
//! exactly when both operands are 1. The constructors are those of the
//! repository's Verus harness `checked_assembly_route.rs`, which proves that
//! on every input the evaluator specification executes without failure each
//! node's value is its canonical term's value. This implementation of them
//! is tested, not proved. The validator establishes separately, by an
//! interval analysis, that no node of the aggregate can trap on any input in
//! its declared domains.

use std::collections::{BTreeMap, BTreeSet};

/// An aggregate input: a state, command or context field by ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Key {
    State(u16),
    Command(u16),
    Context(u16),
}

/// One canonical term; operands are arena IDs.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Term {
    Input(Key),
    Int(i128),
    Bool(bool),
    Add(u32, u32),
    Sub(u32, u32),
    Eq(u32, u32),
    Lt(u32, u32),
    And(u32, u32),
    Not(u32),
    Select(u32, u32, u32),
}

/// The scalar operations both programs are made of, by node index.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Node {
    Input(usize),
    Int(i128),
    Bool(bool),
    Add(usize, usize),
    Sub(usize, usize),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

#[derive(Debug, Default)]
pub(crate) struct Arena {
    terms: Vec<Term>,
    /// Whether each term is Boolean: its value is 0 or 1 wherever defined.
    booleans: Vec<bool>,
    index: BTreeMap<Term, u32>,
    /// The inputs whose domain is Boolean.
    boolean_inputs: BTreeSet<Key>,
}

impl Arena {
    pub(crate) fn new(boolean_inputs: BTreeSet<Key>) -> Self {
        Self {
            boolean_inputs,
            ..Self::default()
        }
    }

    pub(crate) fn term(&self, id: u32) -> Term {
        self.terms[id as usize]
    }

    fn boolean(&self, id: u32) -> bool {
        self.booleans[id as usize]
    }

    fn intern(&mut self, term: Term) -> u32 {
        if let Some(id) = self.index.get(&term) {
            return *id;
        }
        let boolean = match term {
            Term::Input(key) => self.boolean_inputs.contains(&key),
            Term::Int(_) | Term::Add(..) | Term::Sub(..) => false,
            Term::Bool(_) | Term::Eq(..) | Term::Lt(..) | Term::And(..) | Term::Not(_) => true,
            Term::Select(_, then, otherwise) => self.boolean(then) && self.boolean(otherwise),
        };
        let id = u32::try_from(self.terms.len()).unwrap_or(u32::MAX);
        self.terms.push(term);
        self.booleans.push(boolean);
        self.index.insert(term, id);
        id
    }

    pub(crate) fn input(&mut self, key: Key) -> u32 {
        self.intern(Term::Input(key))
    }

    pub(crate) fn int(&mut self, value: i128) -> u32 {
        self.intern(Term::Int(value))
    }

    pub(crate) fn constant(&mut self, value: bool) -> u32 {
        self.intern(Term::Bool(value))
    }

    fn constant_bool(&self, id: u32) -> Option<bool> {
        match self.term(id) {
            Term::Bool(value) => Some(value),
            _ => None,
        }
    }

    fn constant_int(&self, id: u32) -> Option<i128> {
        match self.term(id) {
            Term::Int(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn add(&mut self, left: u32, right: u32) -> u32 {
        if let (Some(a), Some(b)) = (self.constant_int(left), self.constant_int(right))
            && let Some(sum) = a.checked_add(b)
        {
            return self.int(sum);
        }
        self.intern(Term::Add(left, right))
    }

    pub(crate) fn sub(&mut self, left: u32, right: u32) -> u32 {
        if let (Some(a), Some(b)) = (self.constant_int(left), self.constant_int(right))
            && let Some(difference) = a.checked_sub(b)
        {
            return self.int(difference);
        }
        self.intern(Term::Sub(left, right))
    }

    pub(crate) fn eq(&mut self, left: u32, right: u32) -> u32 {
        if left == right {
            return self.constant(true);
        }
        match (self.term(left), self.term(right)) {
            (Term::Int(a), Term::Int(b)) => self.constant(a == b),
            (Term::Bool(a), Term::Bool(b)) => self.constant(a == b),
            _ => self.intern(Term::Eq(left, right)),
        }
    }

    pub(crate) fn lt(&mut self, left: u32, right: u32) -> u32 {
        if let (Some(a), Some(b)) = (self.constant_int(left), self.constant_int(right)) {
            return self.constant(a < b);
        }
        if left == right {
            return self.constant(false);
        }
        self.intern(Term::Lt(left, right))
    }

    pub(crate) fn and(&mut self, left: u32, right: u32) -> u32 {
        let (a, b) = (self.constant_bool(left), self.constant_bool(right));
        if a == Some(false) || b == Some(false) {
            self.constant(false)
        } else if a == Some(true) && self.boolean(right) {
            right
        } else if (b == Some(true) || left == right) && self.boolean(left) {
            left
        } else {
            self.intern(Term::And(left, right))
        }
    }

    pub(crate) fn not(&mut self, operand: u32) -> u32 {
        match self.term(operand) {
            Term::Bool(value) => self.constant(!value),
            Term::Not(inner) if self.boolean(inner) => inner,
            _ => self.intern(Term::Not(operand)),
        }
    }

    pub(crate) fn select(&mut self, condition: u32, then: u32, otherwise: u32) -> u32 {
        match self.constant_bool(condition) {
            Some(true) => then,
            Some(false) => otherwise,
            None if then == otherwise => then,
            None => self.intern(Term::Select(condition, then, otherwise)),
        }
    }
}

#[cfg(test)]
impl Arena {
    /// A term's value where `input` gives each input's value, operator by
    /// operator as the evaluator specification computes it and evaluating
    /// every operand; `None` where evaluation fails.
    pub(crate) fn evaluate(&self, id: u32, input: &dyn Fn(Key) -> Option<i128>) -> Option<i128> {
        let mut memo = BTreeMap::new();
        self.value(id, input, &mut memo)
    }

    fn value(
        &self,
        id: u32,
        input: &dyn Fn(Key) -> Option<i128>,
        memo: &mut BTreeMap<u32, Option<i128>>,
    ) -> Option<i128> {
        if let Some(value) = memo.get(&id) {
            return *value;
        }
        let mut get = |id: u32| self.value(id, input, memo);
        let value = match self.term(id) {
            Term::Input(key) => input(key),
            Term::Int(value) => Some(value),
            Term::Bool(value) => Some(i128::from(value)),
            Term::Add(a, b) => get(a).zip(get(b)).and_then(|(a, b)| a.checked_add(b)),
            Term::Sub(a, b) => get(a).zip(get(b)).and_then(|(a, b)| a.checked_sub(b)),
            Term::Eq(a, b) => get(a).zip(get(b)).map(|(a, b)| i128::from(a == b)),
            Term::Lt(a, b) => get(a).zip(get(b)).map(|(a, b)| i128::from(a < b)),
            Term::And(a, b) => get(a)
                .zip(get(b))
                .map(|(a, b)| i128::from(a == 1 && b == 1)),
            Term::Not(a) => get(a).map(|a| i128::from(a == 0)),
            Term::Select(c, a, b) => {
                let (c, a, b) = (get(c), get(a), get(b));
                c.zip(a)
                    .zip(b)
                    .map(|((c, a), b)| if c == 1 { a } else { b })
            }
        };
        memo.insert(id, value);
        value
    }
}

/// Canonicalizes every node of a program, mapping each input through
/// `input`, which may give a constant for a specialized input.
pub(crate) fn canonical(
    arena: &mut Arena,
    nodes: &[Node],
    mut input: impl FnMut(&mut Arena, usize) -> Option<u32>,
) -> Option<Vec<u32>> {
    let mut ids: Vec<u32> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let get = |index: usize| ids.get(index).copied();
        let id = match *node {
            Node::Input(index) => input(arena, index)?,
            Node::Int(value) => arena.int(value),
            Node::Bool(value) => arena.constant(value),
            Node::Add(a, b) => {
                let (a, b) = (get(a)?, get(b)?);
                arena.add(a, b)
            }
            Node::Sub(a, b) => {
                let (a, b) = (get(a)?, get(b)?);
                arena.sub(a, b)
            }
            Node::Eq(a, b) => {
                let (a, b) = (get(a)?, get(b)?);
                arena.eq(a, b)
            }
            Node::Lt(a, b) => {
                let (a, b) = (get(a)?, get(b)?);
                arena.lt(a, b)
            }
            Node::And(a, b) => {
                let (a, b) = (get(a)?, get(b)?);
                arena.and(a, b)
            }
            Node::Not(a) => {
                let a = get(a)?;
                arena.not(a)
            }
            Node::Select(c, a, b) => {
                let (c, a, b) = (get(c)?, get(a)?, get(b)?);
                arena.select(c, a, b)
            }
        };
        ids.push(id);
    }
    Some(ids)
}

/// Whether two selection outputs choose corresponding cases: the same
/// conditions, in the same order, with the aggregate's case numbers mapped
/// to the leaf's. The map is extended as the comparison proceeds and must
/// stay a function.
pub(crate) fn same_selection(
    arena: &Arena,
    aggregate: u32,
    leaf: u32,
    cases: &mut BTreeMap<i128, i128>,
) -> bool {
    match (arena.term(aggregate), arena.term(leaf)) {
        (Term::Int(mine), Term::Int(theirs)) => *cases.entry(mine).or_insert(theirs) == theirs,
        (Term::Select(condition, then, otherwise), Term::Select(theirs, leaf_then, leaf_else)) => {
            condition == theirs
                && same_selection(arena, then, leaf_then, cases)
                && same_selection(arena, otherwise, leaf_else, cases)
        }
        _ => false,
    }
}

/// The inclusive value interval of each node, as `i128`, for inputs within
/// `domains`; `None` for a node whose value can leave the `i128` range, which
/// the eager evaluator refuses as a trap.
pub(crate) fn intervals(nodes: &[Node], domains: &[(i128, i128)]) -> Vec<Option<(i128, i128)>> {
    let mut out: Vec<Option<(i128, i128)>> = Vec::with_capacity(nodes.len());
    let fits = |(low, high): (i128, i128)| {
        (i128::from(i128::MIN)..=i128::from(i128::MAX)).contains(&low)
            && (i128::from(i128::MIN)..=i128::from(i128::MAX)).contains(&high)
    };
    for node in nodes {
        let get = |index: usize| out.get(index).copied().flatten();
        let interval = match *node {
            Node::Input(index) => domains.get(index).copied(),
            Node::Int(value) => Some((i128::from(value), i128::from(value))),
            Node::Bool(value) => Some((i128::from(value), i128::from(value))),
            Node::Add(a, b) => get(a)
                .zip(get(b))
                .and_then(|((al, ah), (bl, bh))| Some((al.checked_add(bl)?, ah.checked_add(bh)?)))
                .filter(|interval| fits(*interval)),
            Node::Sub(a, b) => get(a)
                .zip(get(b))
                .and_then(|((al, ah), (bl, bh))| Some((al.checked_sub(bh)?, ah.checked_sub(bl)?)))
                .filter(|interval| fits(*interval)),
            Node::Eq(a, b) | Node::Lt(a, b) | Node::And(a, b) => get(a).zip(get(b)).map(|_| (0, 1)),
            Node::Not(a) => get(a).map(|_| (0, 1)),
            Node::Select(c, a, b) => get(c)
                .and(get(a).zip(get(b)))
                .map(|((al, ah), (bl, bh))| (al.min(bl), ah.max(bh))),
        };
        out.push(interval);
    }
    out
}
