//! Ordered preparation of a closed finite fold. Results confer no authority.
//!
//! All inputs are admitted and owned at start. A chunk is selected by offset
//! and count, not by accepting replacement input from a worker. Only a complete
//! result can be read, and the caller must still perform normal authorization.

use super::{Error as EvaluationError, MAX_INPUTS, MAX_STEPS, Program, finite_tuple, ir};
use crate::{SynthesisError, hash_canonical};
use alloc::{vec, vec::Vec};
use core::fmt;
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_core::{Budget, BudgetExceeded, BudgetLimits, BudgetUsed, Resource};
use zeno_fcis_value::Value;

const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Application-supplied context; its provenance is not established by this module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationContext {
    /// Exact starting semantic state root.
    pub state_root: Hash32,
    /// Exact starting version, including when two versions share a root.
    pub state_version: u64,
    /// Complete command/context/principal/profile binding owned by the application.
    pub invocation_hash: Hash32,
}

/// Structural bounds in addition to the existing logical resource budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationLimits {
    /// Maximum number of owned input items, at most the finite input ceiling.
    pub max_items: u32,
    /// Maximum items processed by one call; must be nonzero.
    pub max_chunk_items: u32,
    /// Maximum bytes of the canonical `(initial accumulator, items)` tuple.
    pub max_input_bytes: u64,
    /// Maximum bytes of the final canonical accumulator tuple.
    pub max_output_bytes: u64,
}

impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            max_items: 4_096,
            max_chunk_items: 64,
            max_input_bytes: 4 * 1024 * 1024,
            max_output_bytes: 4_096,
        }
    }
}

/// A rejected operation never exposes a partial result or advances the cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparationError {
    /// Invalid limits, tuple shape, domain, or missing context binding.
    Invalid(&'static str),
    /// A structural input, output, count, or work bound was exceeded.
    Capacity {
        /// Exceeded structural resource.
        resource: &'static str,
        /// Complete requirement for admitted tuples, including framing.
        required: u64,
        /// Declared maximum.
        declared: u64,
    },
    /// An input item does not match the declared finite item domains.
    InvalidItem {
        /// Absolute zero-based input position; no input contents are exposed.
        item: u32,
    },
    /// The complete logical cost cannot fit the supplied immutable budget.
    Budget(BudgetExceeded),
    /// Canonical encoding or commitment failed.
    Encoding(SynthesisError),
    /// The requested cursor differs from the actual cursor.
    WrongOffset {
        /// Actual cursor.
        expected: u32,
        /// Requested cursor.
        supplied: u32,
    },
    /// A chunk is empty, exceeds the per-call limit, or extends past the input.
    InvalidChunk,
    /// The existing eager interpreter rejected an item.
    Evaluation {
        /// Absolute zero-based item index, independent of chunk partition.
        item: u32,
        /// Exact interpreter failure.
        source: EvaluationError,
    },
    /// Not every admitted item has been processed.
    Incomplete,
    /// The caller's current context differs from the starting context.
    StaleContext,
}

impl fmt::Display for PreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[cfg(feature = "std")]
impl std::error::Error for PreparationError {}
impl From<SynthesisError> for PreparationError {
    fn from(error: SynthesisError) -> Self {
        Self::Encoding(error)
    }
}

/// Exclusively owned preparation, never an authoritative state or candidate.
///
/// Dropping it cancels preparation. Recovery starts again with the same admitted
/// program, inputs, context, limits and budget; it never trusts a saved accumulator.
///
/// ```compile_fail
/// use zeno_fcis_synthesis::finite::preparation::PreparedFold;
/// fn read_partial(prepared: &PreparedFold) -> &[i64] {
///     &prepared.accumulator
/// }
/// ```
#[must_use]
pub struct PreparedFold {
    program: Program,
    items: Vec<Vec<i64>>,
    accumulator: Vec<i64>,
    context: PreparationContext,
    limits: PreparationLimits,
    processed: u32,
    operation_hash: Hash32,
    reserved: BudgetUsed,
}

impl fmt::Debug for PreparedFold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedFold")
            .field("operation_hash", &self.operation_hash)
            .field("processed", &self.processed)
            .field("remaining", &self.remaining_items())
            .finish_non_exhaustive()
    }
}

impl PreparedFold {
    /// Admits `(accumulator, item) -> accumulator` and reserves complete costs.
    ///
    /// Read/write units are scalar fields consumed/produced by each evaluation;
    /// candidates are item evaluations; bytes cover the canonical input and
    /// final output tuples. These modeled costs exclude hashing work, allocator
    /// overhead and any later application candidate, witness, receipt or outbox.
    pub fn start(
        program: Program,
        initial: Vec<i64>,
        items: Vec<Vec<i64>>,
        context: PreparationContext,
        limits: PreparationLimits,
        budget: BudgetLimits,
    ) -> Result<Self, PreparationError> {
        if u64::from(limits.max_items) > MAX_INPUTS
            || limits.max_chunk_items == 0
            || u64::from(limits.max_chunk_items) > MAX_INPUTS
            || limits.max_input_bytes > MAX_BYTES
            || limits.max_output_bytes > MAX_BYTES
        {
            return Err(PreparationError::Invalid("limits"));
        }
        if context.state_root == Hash32::ZERO || context.invocation_hash == Hash32::ZERO {
            return Err(PreparationError::Invalid("context"));
        }
        let fields = program.outputs().len();
        if program.inputs().get(..fields) != Some(program.outputs())
            || !ir::admitted(program.outputs(), &initial)
        {
            return Err(PreparationError::Invalid("accumulator"));
        }
        let count =
            u64::try_from(items.len()).map_err(|_| PreparationError::Invalid("item-count"))?;
        require_capacity("items", count, u64::from(limits.max_items))?;
        let nodes = u64::try_from(program.nodes().len())
            .map_err(|_| PreparationError::Invalid("node-count"))?;
        let steps = count
            .checked_mul(nodes)
            .ok_or(PreparationError::Invalid("step-count"))?;
        require_capacity("steps", steps, MAX_STEPS)?;
        let output_bytes = encoded_size(&initial)?;
        require_capacity("output-bytes", output_bytes, limits.max_output_bytes)?;
        // Both finite scalar kinds use fixed-width I128 wire fields. Every
        // admitted accumulator has this exact length, including the final one.
        let tuple_header = encoded_size(&[])?;
        let mut input_bytes = output_bytes + 2 * tuple_header;
        let item_domains = &program.inputs()[fields..];
        let item_bytes = encoded_size(
            &item_domains
                .iter()
                .map(|domain| domain.bounds().0)
                .collect::<Vec<_>>(),
        )?;
        let complete_input_bytes = count
            .checked_mul(item_bytes)
            .and_then(|bytes| bytes.checked_add(input_bytes))
            .ok_or(PreparationError::Invalid("input-size"))?;
        require_input_capacity(input_bytes, complete_input_bytes, limits.max_input_bytes)?;
        for (index, item) in (0..limits.max_items).zip(&items) {
            if !ir::admitted(item_domains, item) {
                return Err(PreparationError::InvalidItem { item: index });
            }
            input_bytes = input_bytes
                .checked_add(item_bytes)
                .ok_or(PreparationError::Invalid("input-size"))?;
            require_input_capacity(input_bytes, complete_input_bytes, limits.max_input_bytes)?;
        }
        let reads = count
            * u64::try_from(program.inputs().len())
                .map_err(|_| PreparationError::Invalid("read-count"))?;
        let writes =
            count * u64::try_from(fields).map_err(|_| PreparationError::Invalid("write-count"))?;
        let mut meter = Budget::new(budget);
        for (resource, amount) in [
            (Resource::Read, reads),
            (Resource::Write, writes),
            (Resource::Candidate, count),
            (Resource::Byte, input_bytes + output_bytes),
        ] {
            meter
                .charge(resource, amount)
                .map_err(PreparationError::Budget)?;
        }
        let input = ir::tuple(vec![
            finite_tuple(&initial),
            ir::tuple(items.iter().map(|item| finite_tuple(item)).collect()),
        ]);
        let input_hash = hash_canonical("zeno-fcis/fold-input", &input)?;
        let resources = [
            Resource::Read,
            Resource::Write,
            Resource::Candidate,
            Resource::Effect,
            Resource::Byte,
            Resource::WitnessByte,
            Resource::Depth,
        ];
        let operation_hash = hash_canonical(
            "zeno-fcis/prepared-fold",
            &ir::tuple(vec![
                program.value(),
                Value::Bytes(input_hash.as_bytes().to_vec().into_boxed_slice()),
                Value::Bytes(context.state_root.as_bytes().to_vec().into_boxed_slice()),
                Value::U128(context.state_version.into()),
                Value::Bytes(
                    context
                        .invocation_hash
                        .as_bytes()
                        .to_vec()
                        .into_boxed_slice(),
                ),
                ir::tuple(vec![
                    Value::U128(limits.max_items.into()),
                    Value::U128(limits.max_chunk_items.into()),
                    Value::U128(limits.max_input_bytes.into()),
                    Value::U128(limits.max_output_bytes.into()),
                ]),
                ir::tuple(
                    resources
                        .into_iter()
                        .map(|resource| Value::U128(budget.limit(resource).into()))
                        .collect(),
                ),
            ]),
        )?;
        Ok(Self {
            program,
            items,
            accumulator: initial,
            context,
            limits,
            processed: 0,
            operation_hash,
            reserved: meter.used(),
        })
    }

    /// Processes exactly the next nonempty range, or leaves preparation unchanged.
    pub fn advance(&mut self, offset: u32, count: u32) -> Result<(), PreparationError> {
        if offset != self.processed {
            return Err(PreparationError::WrongOffset {
                expected: self.processed,
                supplied: offset,
            });
        }
        let end = offset
            .checked_add(count)
            .ok_or(PreparationError::InvalidChunk)?;
        let end_index = usize::try_from(end).map_err(|_| PreparationError::InvalidChunk)?;
        if count == 0 || count > self.limits.max_chunk_items || end_index > self.items.len() {
            return Err(PreparationError::InvalidChunk);
        }
        let start = usize::try_from(offset).map_err(|_| PreparationError::InvalidChunk)?;
        let mut next = self.accumulator.clone();
        let mut environment = Vec::new();
        let mut nodes = Vec::new();
        let mut output = Vec::new();
        for (position, item) in (offset..end).zip(&self.items[start..end_index]) {
            environment.clear();
            environment.extend_from_slice(&next);
            environment.extend_from_slice(item);
            self.program
                .evaluate_into(&environment, &mut nodes, &mut output)
                .map_err(|source| PreparationError::Evaluation {
                    item: position,
                    source,
                })?;
            core::mem::swap(&mut next, &mut output);
        }
        self.accumulator = next;
        self.processed = end;
        Ok(())
    }

    /// Returns a complete, still-untrusted tuple only for the exact current context.
    pub fn finish(&self, current: PreparationContext) -> Result<Vec<i64>, PreparationError> {
        if self.remaining_items() != 0 {
            return Err(PreparationError::Incomplete);
        }
        if current != self.context {
            return Err(PreparationError::StaleContext);
        }
        Ok(self.accumulator.clone())
    }
    /// Exact prefix successfully processed.
    #[must_use]
    pub const fn processed_items(&self) -> u32 {
        self.processed
    }
    /// Remaining input items; every successful advance strictly reduces this count.
    #[must_use]
    pub fn remaining_items(&self) -> u32 {
        u32::try_from(self.items.len()).unwrap_or(u32::MAX) - self.processed
    }
    /// Identity of the owned operation, independent of the selected chunk partition.
    #[must_use]
    pub const fn operation_hash(&self) -> Hash32 {
        self.operation_hash
    }
    /// Complete conservative reservation, not a report of actual elapsed work.
    #[must_use]
    pub const fn reserved_budget(&self) -> BudgetUsed {
        self.reserved
    }
}

fn encoded_size(values: &[i64]) -> Result<u64, PreparationError> {
    let bytes = finite_tuple(values)
        .canonical_bytes()
        .map_err(SynthesisError::Encode)?;
    u64::try_from(bytes.len()).map_err(|_| PreparationError::Invalid("encoding-size"))
}

fn require_capacity(
    resource: &'static str,
    required: u64,
    declared: u64,
) -> Result<(), PreparationError> {
    if required > declared {
        return Err(PreparationError::Capacity {
            resource,
            required,
            declared,
        });
    }
    Ok(())
}

// Preserve prefix/domain error order while reporting the complete retry limit.
fn require_input_capacity(prefix: u64, total: u64, declared: u64) -> Result<(), PreparationError> {
    if prefix > declared {
        return Err(PreparationError::Capacity {
            resource: "input-bytes",
            required: total,
            declared,
        });
    }
    Ok(())
}
