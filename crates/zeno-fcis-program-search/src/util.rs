//! Small deterministic helpers: fingerprints, distinct counts, bit sets and a
//! seeded generator. None of them reads ambient state.

use alloc::vec;
use alloc::vec::Vec;

/// FNV-1a over the little-endian bytes of `values`, from `basis`.
pub(crate) fn fingerprint(basis: u64, values: impl IntoIterator<Item = i128>) -> u64 {
    let mut hash = basis;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

pub(crate) const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// A second, independent basis for 128-bit memo keys.
pub(crate) const FNV_BASIS_2: u64 = 0x6c62_272e_07bb_0142;

/// True when every value equals the first (and there is at least one).
pub(crate) fn single_value(values: &[i128]) -> Option<i128> {
    let first = *values.first()?;
    values.iter().all(|value| *value == first).then_some(first)
}

/// The distinct values of `values`, ascending, or `None` once more than
/// `limit` are seen.
pub(crate) fn distinct_up_to(values: &[i128], limit: usize) -> Option<Vec<i128>> {
    let mut seen: Vec<i128> = Vec::new();
    for value in values {
        if let Err(at) = seen.binary_search(value) {
            if seen.len() == limit {
                return None;
            }
            seen.insert(at, *value);
        }
    }
    Some(seen)
}

/// Number of distinct values.
pub(crate) fn distinct_count(values: &[i128]) -> usize {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.len()
}

/// A fixed-size set of node indices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Bits {
    words: Vec<u64>,
}

impl Bits {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            words: vec![0; size.div_ceil(64)],
        }
    }

    pub(crate) fn insert(&mut self, index: usize) {
        if let Some(word) = self.words.get_mut(index / 64) {
            *word |= 1 << (index % 64);
        }
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        self.words
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }

    pub(crate) fn union_with(&mut self, other: &Self) {
        for (word, more) in self.words.iter_mut().zip(&other.words) {
            *word |= more;
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    pub(crate) fn intersection_len(&self, other: &Self) -> usize {
        self.words
            .iter()
            .zip(&other.words)
            .map(|(a, b)| (a & b).count_ones() as usize)
            .sum()
    }
}

/// SplitMix64: a fixed, seeded sequence for counterexample-guided sampling.
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (`bound > 0`).
    fn below(&mut self, bound: usize) -> usize {
        let wide = (u128::from(self.next()) * bound as u128) >> 64;
        usize::try_from(wide).unwrap_or(0)
    }

    /// `count` distinct items of `items`, chosen by a partial Fisher-Yates
    /// shuffle of a copy.
    pub(crate) fn sample(&mut self, items: &[u32], count: usize) -> Vec<u32> {
        let mut copy = items.to_vec();
        let count = count.min(copy.len());
        for index in 0..count {
            let pick = index + self.below(copy.len() - index);
            copy.swap(index, pick);
        }
        copy.truncate(count);
        copy
    }
}
