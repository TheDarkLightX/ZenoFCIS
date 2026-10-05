//! Support-local exact tables and fixed sample fingerprints for e-classes.
//!
//! A class's value and poison depend only on the inputs its instructions read,
//! and the declared domain is the full product of the input domains. So a
//! class is fixed by its values on the product of the domains of the inputs it
//! actually depends on, its *support*. A [`Table`] records the value or poison
//! on every tuple of that product, in the checker's order restricted to the
//! support (the last input fastest). Tables are always reduced to the minimal
//! support, the inputs whose change can change the value or the poison, so two
//! classes compute the same function on the whole domain exactly when their
//! kinds and tables are identical; no expansion to a common support is needed
//! to compare them.
//!
//! [`Samples`] hold the values at fixed tuples of the whole domain: every
//! tuple when the domain has at most [`EXHAUSTIVE_SAMPLES`], otherwise
//! [`SAMPLES`] tuples (the two corners and a fixed pseudo-random sequence).
//! They bucket candidates and refuse merges they can tell apart; a merge is
//! otherwise decided by tables or by the conservative interval guard.

use super::semantics::Kind;

/// Largest product of support domains, in tuples, for which a class keeps an
/// exact table.
pub(crate) const TABLE_TUPLES: u64 = 1 << 16;
/// Tuple evaluations one e-graph may spend on tables, including confirmations
/// that are discarded. Beyond it new classes keep conservative annotations.
pub(crate) const TABLE_WORK: u64 = 1 << 28;
/// Bytes of tables one e-graph may keep. Beyond it new classes keep
/// conservative annotations.
pub(crate) const TABLE_BYTES: u64 = 64 << 20;
/// Domains of at most this many tuples sample every tuple, so equal samples
/// mean equal functions there.
pub(crate) const EXHAUSTIVE_SAMPLES: u64 = 1_024;
/// Number of fixed sample tuples of a larger domain.
pub(crate) const SAMPLES: usize = 256;

/// Values of one table, by local tuple index; 0 where the tuple is poisoned.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Values {
    /// Bit `i` of the bitset is the value 1 on local tuple `i`.
    Bool(Vec<u64>),
    Int(Vec<i64>),
}

impl Values {
    fn new(kind: Kind, len: usize) -> Self {
        match kind {
            Kind::Bool => Self::Bool(vec![0; len.div_ceil(64)]),
            Kind::Int => Self::Int(vec![0; len]),
        }
    }

    fn get(&self, index: usize) -> i64 {
        match self {
            Self::Bool(bits) => i64::from(bit(bits, index)),
            Self::Int(values) => values[index],
        }
    }

    fn set(&mut self, index: usize, value: i64) {
        match self {
            Self::Bool(bits) => {
                if value == 1 {
                    bits[index / 64] |= 1 << (index % 64);
                }
            }
            Self::Int(values) => values[index] = value,
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Self::Bool(_) => Kind::Bool,
            Self::Int(_) => Kind::Int,
        }
    }
}

fn bit(bits: &[u64], index: usize) -> bool {
    bits[index / 64] >> (index % 64) & 1 == 1
}

/// The exact value and poison of a class on every tuple of the product of its
/// support's domains.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Table {
    /// Sorted input positions; exactly the inputs the class depends on.
    support: Vec<u16>,
    len: usize,
    values: Values,
    /// Bit `i` set when local tuple `i` is poisoned.
    poison: Vec<u64>,
}

impl Table {
    pub(crate) fn support(&self) -> &[u16] {
        &self.support
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Value on local tuple `index`; `None` where poisoned.
    pub(crate) fn get(&self, index: usize) -> Option<i64> {
        (!bit(&self.poison, index)).then(|| self.values.get(index))
    }

    pub(crate) fn poisoned(&self) -> bool {
        self.poison.iter().any(|word| *word != 0)
    }

    /// The one value of a constant, never-poisoned class. With a minimal
    /// support, a class is constant exactly when its support is empty.
    pub(crate) fn constant_value(&self) -> Option<i64> {
        if self.support.is_empty() {
            self.get(0)
        } else {
            None
        }
    }

    /// Smallest and largest defined value, when some tuple is defined.
    pub(crate) fn range(&self) -> Option<(i64, i64)> {
        (0..self.len)
            .filter_map(|index| self.get(index))
            .fold(None, |range, value| match range {
                None => Some((value, value)),
                Some((min, max)) => Some((value.min(min), value.max(max))),
            })
    }

    /// Bytes this table occupies, for the e-graph's memory budget.
    pub(crate) fn bytes(&self) -> u64 {
        let values = match &self.values {
            Values::Bool(bits) => bits.len() * 8,
            Values::Int(values) => values.len() * 8,
        };
        (values + self.poison.len() * 8 + self.support.len() * 2 + 64) as u64
    }

    /// Value and poison of local tuple `index`, comparable as one entry.
    fn entry(&self, index: usize) -> (bool, i64) {
        (bit(&self.poison, index), self.values.get(index))
    }

    /// Local index of a whole-domain tuple.
    #[cfg(test)]
    pub(crate) fn index_of(&self, tuple: &[i64], minima: &[i64], widths: &[u64]) -> usize {
        self.support.iter().fold(0, |index, input| {
            let input = usize::from(*input);
            let offset = tuple[input].abs_diff(minima[input]);
            index * widths[input] as usize + offset as usize
        })
    }
}

/// Work and memory the tables of one e-graph may still use. Spending is
/// deterministic: it depends only on the order of computations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Budget {
    pub(crate) work_left: u64,
    pub(crate) bytes_left: u64,
    /// Tuple evaluations spent so far.
    pub(crate) work_spent: u64,
    /// Whether some table was not computed or kept for lack of budget.
    pub(crate) exhausted: bool,
}

impl Budget {
    pub(crate) fn new(work: u64, bytes: u64) -> Self {
        Self {
            work_left: work,
            bytes_left: bytes,
            work_spent: 0,
            exhausted: false,
        }
    }

    fn spend(&mut self, work: u64) -> bool {
        if work > self.work_left {
            self.exhausted = true;
            return false;
        }
        self.work_left -= work;
        self.work_spent += work;
        true
    }

    /// Reserves memory for a table that a class will keep.
    pub(crate) fn keep(&mut self, table: &Table) -> bool {
        let bytes = table.bytes();
        if bytes > self.bytes_left {
            self.exhausted = true;
            return false;
        }
        self.bytes_left -= bytes;
        true
    }
}

/// Strides of each support position in a table over `support`, last fastest.
fn strides(support: &[u16], widths: &[u64]) -> Vec<u64> {
    let mut strides = vec![0; support.len()];
    let mut stride = 1_u64;
    for (position, input) in support.iter().enumerate().rev() {
        strides[position] = stride;
        stride = stride.saturating_mul(widths[usize::from(*input)]);
    }
    strides
}

/// Product of the widths of `support`, when it is at most [`TABLE_TUPLES`].
fn product(support: &[u16], widths: &[u64]) -> Option<u64> {
    support
        .iter()
        .try_fold(1_u64, |product, input| {
            product.checked_mul(widths[usize::from(*input)])
        })
        .filter(|product| *product <= TABLE_TUPLES)
}

/// The table of an input with `width` values from `min`, or `None` when the
/// domain is wider than [`TABLE_TUPLES`].
pub(crate) fn input(
    position: u16,
    kind: Kind,
    min: i64,
    widths: &[u64],
    budget: &mut Budget,
) -> Option<Table> {
    let len = product(&[position], widths)?;
    if !budget.spend(len) {
        return None;
    }
    let len = len as usize;
    let mut values = Values::new(kind, len);
    for index in 0..len {
        values.set(index, min.wrapping_add(index as i64));
    }
    let table = Table {
        support: vec![position],
        len,
        values,
        poison: vec![0; len.div_ceil(64)],
    };
    Some(minimized(table, widths))
}

/// The table of a constant.
pub(crate) fn constant(kind: Kind, value: i64) -> Table {
    let mut values = Values::new(kind, 1);
    values.set(0, value);
    Table {
        support: Vec::new(),
        len: 1,
        values,
        poison: vec![0],
    }
}

/// What [`combine`] computed.
pub(crate) struct Combined {
    pub(crate) table: Table,
    /// Whether `op` failed on some tuple whose operands are all defined.
    pub(crate) trapped: bool,
}

/// Applies `op` on every tuple of the union of the operands' supports, with
/// eager poison: a tuple is poisoned when any operand is poisoned there or
/// `op` fails there. Returns the table over its essential inputs, or `None`
/// when the union is larger than [`TABLE_TUPLES`] or the budget is spent.
pub(crate) fn combine(
    kind: Kind,
    operands: &[&Table],
    widths: &[u64],
    budget: &mut Budget,
    mut op: impl FnMut(&[i64]) -> Option<i64>,
) -> Option<Combined> {
    let mut support: Vec<u16> = operands
        .iter()
        .flat_map(|table| table.support.iter().copied())
        .collect();
    support.sort_unstable();
    support.dedup();
    let len = product(&support, widths)?;
    // One pass to evaluate, then one per support position to minimize.
    if !budget.spend(len.saturating_mul(support.len() as u64 + 1)) {
        return None;
    }
    let len = len as usize;
    // Stride of each union position in each operand's own layout.
    let operand_strides: Vec<Vec<u64>> = operands
        .iter()
        .map(|table| {
            let own = strides(&table.support, widths);
            support
                .iter()
                .map(|input| {
                    table
                        .support
                        .iter()
                        .position(|other| other == input)
                        .map_or(0, |position| own[position])
                })
                .collect()
        })
        .collect();
    let mut values = Values::new(kind, len);
    let mut poison = vec![0_u64; len.div_ceil(64)];
    let mut digits = vec![0_u64; support.len()];
    let mut indices = vec![0_u64; operands.len()];
    let mut arguments = [0_i64; 3];
    let mut trapped = false;
    for local in 0..len {
        let mut poisoned = false;
        for (slot, table) in operands.iter().enumerate() {
            match table.get(indices[slot] as usize) {
                Some(value) => arguments[slot] = value,
                None => poisoned = true,
            }
        }
        if !poisoned {
            match op(&arguments[..operands.len()]) {
                Some(value) => values.set(local, value),
                None => {
                    trapped = true;
                    poisoned = true;
                }
            }
        }
        if poisoned {
            poison[local / 64] |= 1 << (local % 64);
        }
        // Advance the odometer, last position fastest.
        for position in (0..support.len()).rev() {
            let width = widths[usize::from(support[position])];
            digits[position] += 1;
            if digits[position] < width {
                for (slot, index) in indices.iter_mut().enumerate() {
                    *index += operand_strides[slot][position];
                }
                break;
            }
            for (slot, index) in indices.iter_mut().enumerate() {
                *index -= operand_strides[slot][position] * (width - 1);
            }
            digits[position] = 0;
        }
    }
    let table = Table {
        support,
        len,
        values,
        poison,
    };
    Some(Combined {
        table: minimized(table, widths),
        trapped,
    })
}

/// Drops every support position the table does not depend on.
fn minimized(table: Table, widths: &[u64]) -> Table {
    let strides = strides(&table.support, widths);
    let essential: Vec<bool> = (0..table.support.len())
        .map(|position| {
            let (stride, width) = (
                strides[position] as usize,
                widths[usize::from(table.support[position])] as usize,
            );
            (0..table.len).any(|index| {
                let digit = index / stride % width;
                digit != 0 && table.entry(index) != table.entry(index - digit * stride)
            })
        })
        .collect();
    if essential.iter().all(|kept| *kept) {
        return table;
    }
    let kept: Vec<usize> = (0..table.support.len())
        .filter(|position| essential[*position])
        .collect();
    let support: Vec<u16> = kept
        .iter()
        .map(|position| table.support[*position])
        .collect();
    let len = support
        .iter()
        .map(|input| widths[usize::from(*input)] as usize)
        .product::<usize>();
    let mut values = Values::new(table.values.kind(), len);
    let mut poison = vec![0_u64; len.div_ceil(64)];
    let mut digits = vec![0_usize; kept.len()];
    let mut source = 0_usize;
    for local in 0..len {
        let (poisoned, value) = table.entry(source);
        if poisoned {
            poison[local / 64] |= 1 << (local % 64);
        } else {
            values.set(local, value);
        }
        for slot in (0..kept.len()).rev() {
            let position = kept[slot];
            let width = widths[usize::from(table.support[position])] as usize;
            let stride = strides[position] as usize;
            digits[slot] += 1;
            if digits[slot] < width {
                source += stride;
                break;
            }
            source -= stride * (width - 1);
            digits[slot] = 0;
        }
    }
    Table {
        support,
        len,
        values,
        poison,
    }
}

/// Values of a class at the domain's sample tuples. Boolean values are kept
/// as bits; a poisoned sample holds no value (0 or a clear bit).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Samples {
    Bool {
        count: usize,
        /// Bit `k` set when sample `k` is defined and 1.
        ones: Vec<u64>,
        poison: Vec<u64>,
    },
    Int {
        values: Vec<i64>,
        poison: Vec<u64>,
    },
}

impl Samples {
    /// The same value at every one of `count` samples.
    pub(crate) fn constant(kind: Kind, value: i64, count: usize) -> Self {
        Self::from_values(kind, vec![value; count])
    }

    pub(crate) fn from_values(kind: Kind, values: Vec<i64>) -> Self {
        let poison = vec![0; values.len().div_ceil(64)];
        match kind {
            Kind::Bool => Self::Bool {
                count: values.len(),
                ones: words(values.iter().map(|value| *value == 1), values.len()),
                poison,
            },
            Kind::Int => Self::Int { values, poison },
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Bool { count, .. } => *count,
            Self::Int { values, .. } => values.len(),
        }
    }

    fn poison_words(&self) -> &[u64] {
        match self {
            Self::Bool { poison, .. } | Self::Int { poison, .. } => poison,
        }
    }

    /// Value at sample `k`; `None` where poisoned.
    pub(crate) fn get(&self, sample: usize) -> Option<i64> {
        if bit(self.poison_words(), sample) {
            return None;
        }
        Some(match self {
            Self::Bool { ones, .. } => i64::from(bit(ones, sample)),
            Self::Int { values, .. } => values[sample],
        })
    }

    /// Applies `op` at every sample with eager poison, like [`combine`].
    /// Returns the samples and whether `op` failed at a defined sample.
    pub(crate) fn combine(
        kind: Kind,
        operands: &[&Samples],
        mut op: impl FnMut(&[i64]) -> Option<i64>,
    ) -> (Self, bool) {
        let count = operands.first().map_or(0, |samples| samples.len());
        let mut values = vec![0; count];
        let mut poisoned = vec![false; count];
        let mut trapped = false;
        let mut arguments = [0_i64; 3];
        for sample in 0..count {
            let mut defined = true;
            for (slot, samples) in operands.iter().enumerate() {
                match samples.get(sample) {
                    Some(value) => arguments[slot] = value,
                    None => defined = false,
                }
            }
            if !defined {
                poisoned[sample] = true;
                continue;
            }
            match op(&arguments[..operands.len()]) {
                Some(result) => values[sample] = result,
                None => {
                    trapped = true;
                    poisoned[sample] = true;
                }
            }
        }
        let mut samples = Self::from_values(kind, values);
        let poison = words(poisoned.into_iter(), count);
        match &mut samples {
            Self::Bool {
                ones, poison: own, ..
            } => {
                for (word, mask) in ones.iter_mut().zip(&poison) {
                    *word &= !mask;
                }
                *own = poison;
            }
            Self::Int {
                values,
                poison: own,
            } => {
                for (sample, value) in values.iter_mut().enumerate() {
                    if bit(&poison, sample) {
                        *value = 0;
                    }
                }
                *own = poison;
            }
        }
        (samples, trapped)
    }

    /// The values, 0 where poisoned.
    #[cfg(test)]
    pub(crate) fn values(&self) -> Vec<i64> {
        (0..self.len())
            .map(|sample| self.get(sample).unwrap_or(0))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn poisoned(&self) -> bool {
        self.poison_words().iter().any(|word| *word != 0)
    }

    /// The bits of the defined samples whose value is 1.
    pub(crate) fn bits(&self) -> Vec<u64> {
        match self {
            Self::Bool { ones, .. } => ones.clone(),
            Self::Int { values, poison } => words(
                values
                    .iter()
                    .enumerate()
                    .map(|(sample, value)| *value == 1 && !bit(poison, sample)),
                values.len(),
            ),
        }
    }

    /// Whether both agree, poison included, at every sample whose bit is set
    /// in `mask`.
    pub(crate) fn agrees_on(&self, other: &Self, mask: &[u64]) -> bool {
        self.len() == other.len()
            && (0..self.len())
                .filter(|sample| bit(mask, *sample))
                .all(|sample| self.get(sample) == other.get(sample))
    }
}

/// Packs `count` flags into 64-bit words, flag `k` at bit `k % 64` of word
/// `k / 64`.
pub(crate) fn words(flags: impl Iterator<Item = bool>, count: usize) -> Vec<u64> {
    let mut words = vec![0_u64; count.div_ceil(64)];
    for (index, flag) in flags.enumerate() {
        if flag {
            words[index / 64] |= 1 << (index % 64);
        }
    }
    words
}

/// The [`SAMPLES`] fixed sample tuples of a domain larger than
/// [`EXHAUSTIVE_SAMPLES`]: every input at its minimum, every input at its
/// maximum, then tuples drawn per input from a fixed SplitMix64 sequence.
pub(crate) fn sample_tuples(bounds: &[(i64, i64)]) -> Vec<Vec<i64>> {
    let mut tuples = vec![
        bounds.iter().map(|(min, _)| *min).collect(),
        bounds.iter().map(|(_, max)| *max).collect(),
    ];
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    while tuples.len() < SAMPLES {
        tuples.push(
            bounds
                .iter()
                .map(|(min, max)| {
                    let width = (i128::from(*max) - i128::from(*min) + 1) as u128;
                    let offset = (u128::from(split_mix(&mut state)) * width) >> 64;
                    (i128::from(*min) + offset as i128) as i64
                })
                .collect(),
        );
    }
    tuples
}

fn split_mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
