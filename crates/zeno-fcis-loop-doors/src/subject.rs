//! The subject of a door: one admitted original program, its vocabulary and
//! its complete value table.
//!
//! Tuples are enumerated in the checker's order
//! (`ordered-product-last-input-fastest-v1`: the last input advances
//! fastest, `Bool` takes 0 then 1, integers increase), so an ordinal here is
//! the ordinal `transform check` reports. The original's outputs on every
//! tuple come from the library evaluator (`Program::evaluate`).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Program};

use crate::expr::{Env, Kind, Vocabulary};
use crate::{Refusal, Result};

/// Largest declared domain any door enumerates by default.
pub const DEFAULT_MAX_TUPLES: u64 = 1 << 17;

/// One original program with everything the doors read from it.
#[derive(Clone, Debug)]
pub struct Subject {
    /// Names and kinds.
    pub vocab: Vocabulary,
    /// The admitted original.
    pub original: Program,
    /// Each input's value on every tuple.
    pub columns: Vec<Vec<i128>>,
    /// Each output's value on every tuple.
    pub targets: Vec<Vec<i128>>,
    /// Tuples in the declared domain.
    pub size: usize,
    /// Family parameter values of this instance, if any.
    pub parameters: BTreeMap<String, i128>,
    /// How many cases the contract's table has, when known (out0 is the
    /// index of the first matching case in exported contract programs).
    pub cases: Option<i128>,
}

/// Domain cardinality capped at u128::MAX; huge domains refuse enumeration.
#[must_use]
pub fn domain_size(domain: Domain) -> u128 {
    let (lo, hi) = domain.bounds();
    if lo > hi {
        return 0;
    }
    let lower = (lo as u128) ^ (1u128 << 127);
    let upper = (hi as u128) ^ (1u128 << 127);
    (upper - lower).saturating_add(1)
}

/// Tuples in the product of the domains (saturating).
#[must_use]
pub fn tuples(domains: &[Domain]) -> u128 {
    domains.iter().fold(1u128, |acc, domain| {
        acc.saturating_mul(domain_size(*domain))
    })
}

/// Every tuple's values, one column per input, in the checker's order.
pub fn columns(domains: &[Domain], max_tuples: u64) -> Result<Vec<Vec<i128>>> {
    let total = tuples(domains);
    if total > u128::from(max_tuples) {
        return Err(Refusal::new(format!(
            "domain-over-budget:{total}>{max_tuples}"
        )));
    }
    let total = usize::try_from(total).map_err(|_| Refusal::new("domain-over-budget"))?;
    let mut columns: Vec<Vec<i128>> = domains.iter().map(|_| Vec::with_capacity(total)).collect();
    let mut repeat = total;
    let mut cycle = 1usize;
    for (k, domain) in domains.iter().enumerate() {
        let (lo, hi) = domain.bounds();
        let width = usize::try_from(domain_size(*domain)).unwrap_or(1).max(1);
        repeat /= width;
        for _ in 0..cycle {
            for value in lo..=hi {
                for _ in 0..repeat {
                    columns[k].push(value);
                }
            }
        }
        cycle = cycle.saturating_mul(width);
    }
    Ok(columns)
}

impl Subject {
    /// Builds a subject. The vocabulary must describe `original`'s inputs and
    /// outputs, the domain must have at most `max_tuples` tuples, and the
    /// original must give outputs (no failure) on every tuple.
    pub fn new(
        original: Program,
        vocab: Vocabulary,
        parameters: BTreeMap<String, i128>,
        cases: Option<i128>,
        max_tuples: u64,
    ) -> Result<Self> {
        if vocab.inputs() != original.inputs() || vocab.outputs() != original.outputs() {
            return Err(Refusal::new("vocabulary-abi"));
        }
        let columns = columns(original.inputs(), max_tuples)?;
        let size = columns.first().map_or(1, Vec::len);
        let width = original.outputs().len();
        let mut targets: Vec<Vec<i128>> = (0..width).map(|_| Vec::with_capacity(size)).collect();
        let mut row = Vec::with_capacity(columns.len());
        for ordinal in 0..size {
            row.clear();
            row.extend(columns.iter().map(|column| column[ordinal]));
            let values = original
                .evaluate(&row)
                .map_err(|_| Refusal::new("original-program-fails-on-its-domain"))?;
            for (j, value) in values.into_iter().enumerate() {
                targets[j].push(value);
            }
        }
        Ok(Self {
            vocab,
            original,
            columns,
            targets,
            size,
            parameters,
            cases,
        })
    }

    /// Input values of one tuple.
    #[must_use]
    pub fn point(&self, ordinal: usize) -> Vec<i128> {
        self.columns.iter().map(|column| column[ordinal]).collect()
    }

    /// The ordinal of a tuple of input values (`None` outside the domain).
    #[must_use]
    pub fn ordinal(&self, point: &[i128]) -> Option<usize> {
        let mut ordinal: u128 = 0;
        for (value, domain) in point.iter().zip(self.original.inputs()) {
            if !domain.contains(*value) {
                return None;
            }
            let (lo, _) = domain.bounds();
            let offset = value.checked_sub(lo).and_then(|n| u128::try_from(n).ok())?;
            ordinal = ordinal
                .checked_mul(domain_size(*domain))?
                .checked_add(offset)?;
        }
        usize::try_from(ordinal).ok()
    }

    /// An evaluation environment over every tuple, with `keep` standing for
    /// output `j` when given.
    #[must_use]
    pub fn env(&self, keep: Option<usize>) -> Env<'_> {
        Env {
            vocab: &self.vocab,
            columns: &self.columns,
            keep: keep.and_then(|j| self.targets.get(j)).map(Vec::as_slice),
            len: self.size,
        }
    }

    /// The kind of output `j`.
    #[must_use]
    pub fn output_kind(&self, j: usize) -> Kind {
        self.vocab
            .output_kinds()
            .get(j)
            .copied()
            .unwrap_or(Kind::Int)
    }

    /// Integer constants of the original program, in node order.
    #[must_use]
    pub fn original_constants(&self) -> Vec<i128> {
        self.original
            .nodes()
            .iter()
            .filter_map(|op| match op {
                zeno_fcis_synthesis::finite::Op::Int(value) => Some(*value),
                _ => None,
            })
            .collect()
    }
}
