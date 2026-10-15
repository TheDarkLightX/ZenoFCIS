//! Pure bounded operational plan. Units are logical services, never CPU time.
//! This policy does not change evaluator Steps or canonical receipt semantics.

// Version 3 bound branch-specific candidate provenance intake. Version 4 binds
// the G12 work ceilings scaled to the original (with checked-i64-v1 as the
// default profile); tariffs, checker reservation formulas and canonical
// receipt semantics are version 3's.
pub(crate) const WORK_POLICY: &str = "zeno-fcis/checker-work/4";
pub(crate) const MAX_WORKERS: u64 = 8;
pub(crate) const MAX_CHUNK: u64 = 4096;
pub(crate) const MAX_CHUNKS: u64 = 65_536;
pub(crate) const MAX_TUPLES: u64 = 100_000_000;
pub(crate) const MAX_BYTES: u64 = 65_536;
/// Entire bounded acquisition allowance, retained even on short reads/EOF.
pub(crate) const ACQUISITION_WORK: u64 = 2 * (MAX_BYTES + 16 + 4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stop {
    Limits,
    Work,
    Accounting,
    Dimensions,
    Chunks,
    Deadline,
    Spawn,
    Worker,
    Coverage,
}

impl Stop {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Limits => "operational-limits",
            Self::Work => "work-exhausted",
            Self::Accounting => "work-accounting-exceeded",
            Self::Dimensions => "operational-dimensions",
            Self::Chunks => "chunk-cap",
            Self::Deadline => "deadline",
            Self::Spawn => "worker-unavailable",
            Self::Worker => "worker-failed",
            Self::Coverage => "range-coverage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RunLimits {
    pub(crate) workers: u64,
    pub(crate) chunk_tuples: u64,
    pub(crate) max_chunks: u64,
    pub(crate) work_units: u64,
    pub(crate) deadline_ms: u64,
}

impl RunLimits {
    /// Explicit offline profile. This is not the loop's 2s/64MiB profile.
    pub(crate) const fn offline() -> Self {
        Self {
            workers: 4,
            chunk_tuples: 4096,
            max_chunks: MAX_CHUNKS,
            work_units: 1_000_000_000_000,
            deadline_ms: 60_000,
        }
    }

    pub(crate) fn validate(self) -> Result<(), Stop> {
        if self.workers == 0
            || self.workers > MAX_WORKERS
            || self.chunk_tuples == 0
            || self.chunk_tuples > MAX_CHUNK
            || self.max_chunks == 0
            || self.max_chunks > MAX_CHUNKS
            || self.deadline_ms == 0
            || self.deadline_ms > 60_000
        {
            return Err(Stop::Limits);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dimensions {
    pub(crate) bytes: [u64; 2],
    pub(crate) nodes: [u64; 2],
    pub(crate) inputs: u64,
    pub(crate) outputs: u64,
    pub(crate) tuples: u64,
}

/// No verdict capability. Private fields bind a checked bounded partition and
/// immutable full reservation, retained even if report merge/finalization fails.
#[derive(Debug)]
pub(crate) struct RunPlan {
    dimensions: Dimensions,
    limits: RunLimits,
    chunks: u64,
    setup: u64,
    reserved: u64,
}

/// Reserve bytes, bounded read services and worst-case canonical import before
/// decoding. This deliberately retains unused slack on malformed/short input.
pub(crate) fn setup_work(bytes: [u64; 2]) -> Result<u64, Stop> {
    if bytes[0] > MAX_BYTES || bytes[1] > MAX_BYTES {
        return Err(Stop::Dimensions);
    }
    let mut charge = ACQUISITION_WORK + 8192; // bounded refusal rendering/control
    for b in bytes {
        // Acquisition byte services + open/metadata/read/close; decoder tree,
        // canonical rebuild/comparison/hashing and complete admission slots.
        charge += 8 * b + 16 * 2048 + 16 * (32 + 16 + 256 + 16) + 256;
    }
    Ok(charge)
}

pub(crate) fn plan(d: Dimensions, limits: RunLimits) -> Result<RunPlan, Stop> {
    limits.validate()?;
    let setup = setup_work(d.bytes)?;
    if d.inputs > 32
        || d.outputs > 16
        || d.nodes[0] > 256
        || d.nodes[1] > 256
        || d.tuples == 0
        || d.tuples > MAX_TUPLES
    {
        return Err(Stop::Dimensions);
    }
    let chunks = d.tuples.div_ceil(limits.chunk_tuples);
    if chunks > limits.max_chunks {
        return Err(Stop::Chunks);
    }
    // Every operand is bounded above before arithmetic. Maximum total <2^40.
    // Exact source-bound arithmetic verification and tariff audit are release
    // obligations; this comment is not a proof certificate.
    let a = d.bytes[0] + d.bytes[1];
    let v = d.nodes[0] + d.nodes[1];
    let p = 2 * d.inputs + d.outputs + 8;
    let seek = 3 * d.inputs + 8;
    let tuple = 3 * d.inputs + 2 * v + 7 * d.outputs + 40;
    let merge = 2 * d.inputs + 4 * d.outputs + v + 32;
    let services = 128 * limits.workers + 32 * chunks + 64;
    let receipt_bytes = 4096 + 96 * (d.inputs + d.outputs);
    let finalize = p + merge + 2 * a + 8 * receipt_bytes + 8192;
    let reserved = setup
        + 2*p // prepared admission and scheduler's independent shape binding
        + chunks * (p + seek)
        + d.tuples * tuple
        + (chunks - 1) * merge
        + services
        + finalize;
    if reserved > limits.work_units {
        return Err(Stop::Work);
    }
    Ok(RunPlan {
        dimensions: d,
        limits,
        chunks,
        setup,
        reserved,
    })
}

impl RunPlan {
    pub(crate) fn dimensions(&self) -> Dimensions {
        self.dimensions
    }
    pub(crate) fn limits(&self) -> RunLimits {
        self.limits
    }
    pub(crate) fn chunks(&self) -> u64 {
        self.chunks
    }
    pub(crate) fn setup(&self) -> u64 {
        self.setup
    }
    pub(crate) fn reserved(&self) -> u64 {
        self.reserved
    }
    pub(crate) fn interval(&self, chunk: u64) -> Option<(u64, u64, u64)> {
        if chunk >= self.chunks {
            return None;
        }
        let lo = chunk * self.limits.chunk_tuples;
        let hi = self.dimensions.tuples.min(lo + self.limits.chunk_tuples);
        // Sub-account of the full plan, not its entire work bound.
        Some((
            lo,
            hi,
            (hi - lo) * (self.dimensions.nodes[0] + self.dimensions.nodes[1] + 2),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dimensions() -> Dimensions {
        Dimensions {
            bytes: [100, 100],
            nodes: [2, 1],
            inputs: 2,
            outputs: 1,
            tuples: 17,
        }
    }
    #[test]
    fn exact_reservation_one_short_and_total_partition() {
        let limits = RunLimits {
            chunk_tuples: 3,
            ..RunLimits::offline()
        };
        let p = plan(dimensions(), limits).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(p.chunks(), 6);
        // Pin every tariff term: a dropped term must fail here, not only
        // shift both sides of the relative one-short comparison below.
        assert_eq!(p.setup(), 217_192);
        assert_eq!(p.reserved(), 263_086);
        let mut end = 0;
        let mut core_work = 0;
        for k in 0..p.chunks() {
            let (lo, hi, work) = p.interval(k).unwrap_or_else(|| panic!("planned interval"));
            assert_eq!(lo, end);
            assert!(hi > lo);
            end = hi;
            core_work += work;
        }
        assert_eq!(end, 17);
        assert_eq!(core_work, 85);
        assert_eq!(p.interval(6), None);
        assert!(
            plan(
                dimensions(),
                RunLimits {
                    work_units: p.reserved(),
                    ..limits
                }
            )
            .is_ok()
        );
        assert_eq!(
            plan(
                dimensions(),
                RunLimits {
                    work_units: p.reserved() - 1,
                    ..limits
                }
            )
            .err()
            .unwrap_or_else(|| panic!("expected refusal")),
            Stop::Work
        );
    }
    #[test]
    fn invalid_extreme_dimensions_limits_and_chunk_cap_refuse() {
        let l = RunLimits::offline();
        for d in [
            Dimensions {
                tuples: 0,
                ..dimensions()
            },
            Dimensions {
                tuples: u64::MAX,
                ..dimensions()
            },
            Dimensions {
                nodes: [u64::MAX, 1],
                ..dimensions()
            },
            Dimensions {
                inputs: 33,
                ..dimensions()
            },
            Dimensions {
                outputs: 17,
                ..dimensions()
            },
            Dimensions {
                bytes: [65537, 0],
                ..dimensions()
            },
        ] {
            assert_eq!(
                plan(d, l)
                    .err()
                    .unwrap_or_else(|| panic!("expected refusal")),
                Stop::Dimensions
            );
        }
        for workers in [0, 9, u64::MAX] {
            assert_eq!(
                plan(dimensions(), RunLimits { workers, ..l })
                    .err()
                    .unwrap_or_else(|| panic!("expected refusal")),
                Stop::Limits
            );
        }
        assert_eq!(
            plan(
                dimensions(),
                RunLimits {
                    chunk_tuples: 1,
                    max_chunks: 16,
                    ..l
                }
            )
            .err()
            .unwrap_or_else(|| panic!("expected refusal")),
            Stop::Chunks
        );
        let p = plan(
            Dimensions {
                inputs: 0,
                tuples: 1,
                ..dimensions()
            },
            l,
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(p.interval(0), Some((0, 1, 5)));
    }
}
