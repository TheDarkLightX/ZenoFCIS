#![no_main]

//! Worker channel wire frames (`ZFW1` requests, `ZFR1` replies) and the
//! channel's trusted-configuration bounds.
//!
//! Differential oracle: the harness carries its own reading of the documented
//! 16-byte layouts and requires the crate's encoders to agree with it byte for
//! byte, for every sequence and slot the fuzzer draws; a reply's status and
//! detail bytes must be the documented pair for its `Status`. `Channel::new`
//! must accept exactly the configurations the documented bounds admit
//! (at most `MAX_HANDLES` handles, a limit in `1..=MAX_REQUESTS`).
//!
//! The dispatch path (`Channel::dispatch`) needs a `TrustedDispatcher` over a
//! seeded SQLite owner, which only the crate's integration tests can build;
//! its frame admission (format, sequence, slot, limit, in that precedence) is
//! covered by `tests/effects_owner.rs`. Exposing that admission as a pure
//! function would let this harness compare it with the reference below.

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell_sqlite::effects::channel::{
    Channel, Error, FRAME_BYTES, MAX_HANDLES, MAX_REQUESTS, Reply, Status, request,
};
use zeno_fcis_shell_sqlite::effects::owner::{IntentHandle, Refusal};

fn reference_request(sequence: u64, slot: u32) -> [u8; FRAME_BYTES] {
    let mut out = [0; FRAME_BYTES];
    out[..4].copy_from_slice(b"ZFW1");
    out[4..12].copy_from_slice(&sequence.to_le_bytes());
    out[12..16].copy_from_slice(&slot.to_le_bytes());
    out
}

fn reference_reply(sequence: u64, status: u8, detail: u8) -> [u8; FRAME_BYTES] {
    let mut out = [0; FRAME_BYTES];
    out[..4].copy_from_slice(b"ZFR1");
    out[4..12].copy_from_slice(&sequence.to_le_bytes());
    out[12] = status;
    out[13] = detail;
    out
}

/// The documented status/detail pairs, independent of `Reply::encode`.
const STATUSES: [(Status, u8, u8); 12] = [
    (Status::Sent, 0, 0),
    (Status::Unknown, 1, 0),
    (Status::NotSent, 2, 0),
    (Status::PolicyRefused, 3, 0),
    (Status::DispatcherRefused(Refusal::Owner), 4, 1),
    (Status::DispatcherRefused(Refusal::Tenant), 4, 2),
    (Status::DispatcherRefused(Refusal::Principal), 4, 3),
    (Status::DispatcherRefused(Refusal::Grant), 4, 4),
    (Status::DispatcherRefused(Refusal::Intent), 4, 5),
    (Status::DispatcherRefused(Refusal::Pending), 4, 6),
    (Status::DispatcherRefused(Refusal::Content), 4, 7),
    (Status::DispatcherUnavailable, 5, 0),
];

fn take_u64(input: &[u8], at: usize) -> u64 {
    let mut bytes = [0; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = input.get(at + index).copied().unwrap_or(0);
    }
    u64::from_le_bytes(bytes)
}

fuzz_target!(|input: &[u8]| {
    let sequence = take_u64(input, 0);
    let slot = u32::try_from(take_u64(input, 8) & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    let status_index = usize::from(input.get(16).copied().unwrap_or(0)) % STATUSES.len();
    let limit = u32::try_from(take_u64(input, 17) & 0xFFFF).unwrap_or(u32::MAX);

    // Request frames: the encoder is the documented layout and nothing else.
    let frame = request(sequence, slot);
    assert_eq!(
        frame,
        reference_request(sequence, slot),
        "request layout differs from ZFW1"
    );
    assert_eq!(&frame[..4], b"ZFW1");
    assert_eq!(
        u64::from_le_bytes(frame[4..12].try_into().unwrap_or([0; 8])),
        sequence
    );
    assert_eq!(
        u32::from_le_bytes(frame[12..16].try_into().unwrap_or([0; 4])),
        slot
    );

    // Reply frames: status and detail bytes are the documented pair; the trailing
    // two bytes stay zero; the sequence is echoed unchanged.
    let (status, status_byte, detail) = STATUSES[status_index];
    let reply = Reply { sequence, status }.encode();
    assert_eq!(
        reply,
        reference_reply(sequence, status_byte, detail),
        "reply layout differs from ZFR1"
    );
    assert_eq!(&reply[14..], &[0, 0], "reply padding must stay zero");
    for (other, other_status, other_detail) in STATUSES {
        if other != status {
            assert_ne!(
                (other_status, other_detail),
                (status_byte, detail),
                "two statuses share a wire pair"
            );
        }
    }

    // Trusted configuration: the empty handle table is the only one this harness can
    // build (handles are owner grants); the limit bound is the fuzzed dimension.
    let handles: [IntentHandle; 0] = [];
    let channel = Channel::new(Hash32::ZERO, Hash32::new([1; 32]), &handles, limit);
    let admitted = (1..=MAX_REQUESTS).contains(&limit) && handles.len() <= MAX_HANDLES;
    match channel {
        Ok(_) => assert!(
            admitted,
            "a limit outside 1..={MAX_REQUESTS} was accepted: {limit}"
        ),
        Err(error) => {
            assert!(!admitted, "an in-bounds limit was refused: {limit}");
            assert_eq!(error, Error::Configuration);
        }
    }
});
