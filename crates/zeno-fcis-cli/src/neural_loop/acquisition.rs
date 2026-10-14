//! Pure invocation acquisition reservation, distinct from durable session work.
//! Reservation precedes bootstrap file reads, copies and admission. It survives
//! refusal; observed intake counts are not a measurement of all logical work.
//! Tariff coefficients remain subject to source coverage and formal review.

use super::limits::Limits;
use serde_json::{Value, json};

pub(crate) const POLICY: &str = "zeno-fcis/loop-acquisition/1";
pub(crate) const PROGRAM_BYTES: u64 = 65_536;
pub(crate) const JSON_BYTES: u64 = 1_048_576;
pub(crate) const LEDGER_BYTES: u64 = 4_194_304;
pub(crate) const HEAD_BYTES: u64 = 4_096;
pub(crate) const DIRECTORY_ENTRIES: u64 = Limits::CEILING.attempts as u64 + 1;
/// Staging directories left by interrupted contained runs that a listing
/// skips and reports; more count as ordinary entries. The resume directory
/// allowance covers them: four full listings hold at most 33 session files.
pub(crate) const STAGING_LEFTOVERS: u64 = 4;

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Open,
    Resume,
    Encode,
}

pub(crate) struct Acquisition {
    read_slots: u64,
    byte_slots: u64,
    directory_slots: u64,
    reserved: u64,
    attempted_reads: u64,
    observed_bytes: u64,
    observed_read_calls: u64,
    observed_entries: u64,
    leftover_staging: u64,
    read_bytes_left: u64,
    pending_read: bool,
    exhausted: bool,
}
impl Acquisition {
    pub(crate) fn reserve(mode: Mode) -> Self {
        let (read_slots, byte_slots, directory_slots, admissions, ledger_bytes) = match mode {
            Mode::Open => (1, PROGRAM_BYTES + 1, 0, 1, 0),
            Mode::Encode => (1, JSON_BYTES + 1, 0, 1, 0),
            Mode::Resume => {
                // Request+original, optional replacement+receipt, and at most
                // eight witness+candidate pairs; ledger and head once each.
                let programs = 4 + 2 * Limits::CEILING.attempts as u64;
                (
                    programs + 2,
                    programs * (PROGRAM_BYTES + 1) + LEDGER_BYTES + 1 + HEAD_BYTES + 1,
                    4 * (DIRECTORY_ENTRIES + 1),
                    2 + Limits::CEILING.attempts as u64,
                    LEDGER_BYTES,
                )
            }
        };
        // Byte acquisition/copy/hash/codec services, per-line ledger services,
        // bounded program admission, directory comparison/metadata and report.
        // These are a candidate complete service tariff, not a CPU-time bound.
        let admission = 16 * 2048 + 16 * (32 + 16 + 256 + 16) + 3 * 256 * 256;
        let reserved = 32 * byte_slots
            + 16 * ledger_bytes
            + admissions * admission
            + 16 * directory_slots * directory_slots
            + 32 * directory_slots
            + 8 * read_slots
            + 8192;
        Self {
            read_slots,
            byte_slots,
            directory_slots,
            reserved,
            attempted_reads: 0,
            observed_bytes: 0,
            observed_read_calls: 0,
            observed_entries: 0,
            leftover_staging: 0,
            read_bytes_left: 0,
            pending_read: false,
            exhausted: false,
        }
    }
    pub(crate) fn claim_read(&mut self, limit: u64) -> bool {
        if self.exhausted {
            return false;
        }
        let Some(bytes) = limit.checked_add(1) else {
            self.exhausted = true;
            return false;
        };
        let Some(attempted_reads) = self.attempted_reads.checked_add(1) else {
            self.exhausted = true;
            return false;
        };
        if self.pending_read || self.read_slots == 0 || bytes > self.byte_slots {
            self.exhausted = true;
            return false;
        }
        self.read_slots -= 1;
        self.byte_slots -= bytes;
        self.attempted_reads = attempted_reads;
        // Unused allowance from the preceding file is discarded, not refunded.
        // Each nonterminal call must produce a byte; EOF/error ends that file.
        self.read_bytes_left = bytes;
        true
    }
    pub(crate) fn read_call(&mut self) -> bool {
        if self.exhausted {
            return false;
        }
        let Some(calls) = self.observed_read_calls.checked_add(1) else {
            self.exhausted = true;
            return false;
        };
        if self.pending_read || self.read_bytes_left == 0 {
            self.exhausted = true;
            return false;
        }
        self.observed_read_calls = calls;
        self.pending_read = true;
        true
    }
    /// Settle one permitted read, including zero bytes on EOF or error.
    pub(crate) fn read_bytes(&mut self, bytes: usize) -> bool {
        if self.exhausted {
            return false;
        }
        let Ok(bytes) = u64::try_from(bytes) else {
            self.exhausted = true;
            return false;
        };
        let Some(observed) = self.observed_bytes.checked_add(bytes) else {
            self.exhausted = true;
            return false;
        };
        if !self.pending_read || bytes > self.read_bytes_left {
            self.exhausted = true;
            return false;
        }
        self.pending_read = false;
        self.observed_bytes = observed;
        self.read_bytes_left -= bytes;
        if bytes == 0 {
            self.read_bytes_left = 0;
        }
        true
    }
    pub(crate) fn directory_entry(&mut self) -> bool {
        if self.exhausted {
            return false;
        }
        let Some(entries) = self.observed_entries.checked_add(1) else {
            self.exhausted = true;
            return false;
        };
        if self.directory_slots == 0 {
            self.exhausted = true;
            return false;
        }
        self.directory_slots -= 1;
        self.observed_entries = entries;
        true
    }
    /// Skip one leftover staging directory, up to the fixed allowance.
    pub(crate) fn leftover_staging(&mut self) -> bool {
        if self.leftover_staging >= STAGING_LEFTOVERS {
            return false;
        }
        self.leftover_staging += 1;
        true
    }
    pub(crate) fn stop(&mut self) {
        self.exhausted = true;
    }
    pub(crate) fn exhausted(&self) -> bool {
        self.exhausted
    }
    pub(crate) fn json(&self) -> Value {
        json!({"policy":POLICY,"reserved_units":self.reserved,
            "observed_intake":{"attempted_reads":self.attempted_reads,
                "read_calls":self.observed_read_calls,"bytes":self.observed_bytes,
                "directory_entries":self.observed_entries},
            "leftover_checker_staging":self.leftover_staging,
            "exhausted":self.exhausted,"separate_from_session_check_work":true})
    }
}

#[cfg(test)]
mod tests {
    use super::{Acquisition, Mode};

    #[test]
    fn acquisition_enforces_claim_call_result_order_and_terminal_exhaustion() {
        let mut no_claim = Acquisition::reserve(Mode::Resume);
        assert!(!no_claim.read_call());
        assert!(!no_claim.claim_read(3));
        assert!(!no_claim.directory_entry());
        assert_eq!(no_claim.json()["observed_intake"]["attempted_reads"], 0);

        let mut no_call = Acquisition::reserve(Mode::Resume);
        assert!(no_call.claim_read(3));
        assert!(!no_call.read_bytes(1));
        assert_eq!(no_call.json()["observed_intake"]["bytes"], 0);

        let mut pending = Acquisition::reserve(Mode::Resume);
        assert!(pending.claim_read(3));
        assert!(pending.read_call());
        assert!(!pending.read_call());
        assert_eq!(pending.json()["observed_intake"]["read_calls"], 1);

        let mut reclaim = Acquisition::reserve(Mode::Resume);
        assert!(reclaim.claim_read(3));
        assert!(reclaim.read_call());
        assert!(!reclaim.claim_read(3));
        assert_eq!(reclaim.json()["observed_intake"]["attempted_reads"], 1);
    }

    #[test]
    fn acquisition_one_byte_progress_and_sentinel_fit_the_paid_allowance() {
        let mut meter = Acquisition::reserve(Mode::Open);
        assert!(meter.claim_read(3));
        for _ in 0..4 {
            assert!(meter.read_call());
            assert!(meter.read_bytes(1));
        }
        let before = meter.json()["observed_intake"].clone();
        assert_eq!(before["read_calls"], 4);
        assert_eq!(before["bytes"], 4);
        assert!(!meter.read_call());
        assert_eq!(meter.json()["observed_intake"], before);

        let mut excess = Acquisition::reserve(Mode::Open);
        assert!(excess.claim_read(3));
        assert!(excess.read_call());
        assert!(!excess.read_bytes(5));
        assert_eq!(excess.json()["observed_intake"]["bytes"], 0);
        assert!(excess.exhausted());
    }

    #[test]
    fn acquisition_eof_or_error_closes_only_the_current_file_without_refund() {
        let mut meter = Acquisition::reserve(Mode::Resume);
        let initial = meter.byte_slots;
        for _ in 0..2 {
            assert!(meter.claim_read(3));
            assert!(meter.read_call());
            assert!(meter.read_bytes(0));
            assert_eq!(meter.read_bytes_left, 0);
        }
        assert!(!meter.exhausted());
        assert_eq!(meter.byte_slots, initial - 8);
        assert_eq!(meter.json()["observed_intake"]["attempted_reads"], 2);
        assert_eq!(meter.json()["observed_intake"]["read_calls"], 2);
        assert_eq!(meter.json()["observed_intake"]["bytes"], 0);
        assert!(!meter.read_call());
    }

    #[test]
    fn acquisition_counter_overflow_refuses_before_mutating_observations() {
        let mut claim = Acquisition::reserve(Mode::Resume);
        claim.attempted_reads = u64::MAX;
        let slots = (claim.read_slots, claim.byte_slots);
        assert!(!claim.claim_read(3));
        assert_eq!((claim.read_slots, claim.byte_slots), slots);

        let mut limit = Acquisition::reserve(Mode::Resume);
        assert!(!limit.claim_read(u64::MAX));
        assert_eq!(limit.attempted_reads, 0);

        let mut calls = Acquisition::reserve(Mode::Resume);
        assert!(calls.claim_read(3));
        calls.observed_read_calls = u64::MAX;
        assert!(!calls.read_call());
        assert_eq!(calls.observed_read_calls, u64::MAX);
        assert_eq!(calls.read_bytes_left, 4);

        let mut bytes = Acquisition::reserve(Mode::Resume);
        assert!(bytes.claim_read(3));
        assert!(bytes.read_call());
        bytes.observed_bytes = u64::MAX;
        assert!(!bytes.read_bytes(1));
        assert_eq!(bytes.observed_bytes, u64::MAX);
        assert_eq!(bytes.read_bytes_left, 4);

        let mut entries = Acquisition::reserve(Mode::Resume);
        entries.observed_entries = u64::MAX;
        let slots = entries.directory_slots;
        assert!(!entries.directory_entry());
        assert_eq!(entries.observed_entries, u64::MAX);
        assert_eq!(entries.directory_slots, slots);
    }
}
