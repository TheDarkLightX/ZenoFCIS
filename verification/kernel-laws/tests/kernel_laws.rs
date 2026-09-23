//! Kernel law harnesses.
//!
//! Each law is one predicate, checked two ways:
//!
//! - `*_holds_on_every_small_input` enumerates a small, stated domain with
//!   plain loops. It is deterministic and complete for that domain.
//! - `*_holds_on_random_inputs` runs the same predicate under bolero over a
//!   broad domain. Each run draws fresh inputs, and a failure prints the seed
//!   that replays it. The same harness can run under a fuzzing engine.
//!
//! The predicates read only public kernel APIs, so a change inside a frozen
//! kernel crate is checked without editing it.

use bolero::check;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Hash32, decode_value};
use zeno_fcis_core::{
    Accepted, Budget, BudgetLimits, Decision, Resource, StableReason, collect_and_choose,
    first_reason,
};
use zeno_fcis_patch::{CanonicalPatch, PatchError, PatchOp, PathSegment, ValuePath};
use zeno_fcis_value::{AdmittedValue, Field, MapEntry, Value};

// ---------------------------------------------------------------------------
// Law budget-charge: a charge either succeeds within its limit or fails with
// an exact report and leaves every resource's consumption unchanged.
// ---------------------------------------------------------------------------

const RESOURCES: [Resource; 7] = [
    Resource::Read,
    Resource::Write,
    Resource::Candidate,
    Resource::Effect,
    Resource::Byte,
    Resource::WitnessByte,
    Resource::Depth,
];

fn check_budget(limits: [u64; 7], charges: &[(Resource, u64)]) {
    let budget_limits = RESOURCES
        .iter()
        .zip(limits)
        .fold(BudgetLimits::zero(), |acc, (resource, limit)| {
            acc.with_limit(*resource, limit)
        });
    let mut budget = Budget::new(budget_limits);
    let mut model = [0_u64; 7];
    for &(resource, amount) in charges {
        let index = resource as usize;
        let before = budget.used();
        let result = budget.charge(resource, amount);
        match model[index].checked_add(amount) {
            Some(next) if next <= limits[index] => {
                assert!(result.is_ok(), "{resource:?} +{amount} within {limits:?}");
                model[index] = next;
            }
            expected => {
                let error = result.expect_err("a charge beyond the limit must fail");
                assert_eq!(error.resource(), resource);
                assert_eq!(error.limit(), limits[index]);
                assert_eq!(error.attempted(), expected.unwrap_or(u64::MAX));
                assert_eq!(budget.used(), before, "a failed charge must change nothing");
            }
        }
        for (position, other) in RESOURCES.iter().enumerate() {
            assert_eq!(budget.used().used(*other), model[position]);
            assert!(model[position] <= limits[position]);
        }
    }
    let finished = budget.finish(Decision::<(), (), ()>::Accept(Accepted::new(())));
    assert_eq!(finished.limits(), budget_limits);
    for (position, resource) in RESOURCES.iter().enumerate() {
        assert_eq!(finished.used().used(*resource), model[position]);
    }
}

#[test]
fn budget_charge_holds_on_every_small_input() {
    // Two limited resources and one other, amounts at both ends of u64, and
    // every sequence of up to three charges.
    const AMOUNTS: [u64; 5] = [0, 1, 2, u64::MAX - 1, u64::MAX];
    let charged = [Resource::Read, Resource::Write, Resource::Depth];
    let alphabet: Vec<(Resource, u64)> = charged
        .iter()
        .flat_map(|resource| AMOUNTS.iter().map(move |amount| (*resource, *amount)))
        .collect();
    let mut cases = 0_u64;
    for read_limit in AMOUNTS {
        for write_limit in AMOUNTS {
            let limits = [read_limit, write_limit, 0, 0, 0, 0, 0];
            for_each_sequence(&alphabet, 3, &mut |charges| {
                check_budget(limits, charges);
                cases += 1;
            });
        }
    }
    assert_eq!(cases, 25 * (1 + 15 + 225 + 3375));
}

#[test]
fn budget_charge_holds_on_random_inputs() {
    check!()
        .with_type::<([u64; 7], Vec<(u8, u64)>)>()
        .for_each(|(limits, raw)| {
            let charges: Vec<(Resource, u64)> = raw
                .iter()
                .take(32)
                .map(|(index, amount)| (RESOURCES[usize::from(*index) % 7], *amount))
                .collect();
            check_budget(*limits, &charges);
        });
}

// ---------------------------------------------------------------------------
// Law reason-choice: the chosen reason is the least by (precedence, code
// bytes), whatever order the applicable reasons were collected in.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
struct Reason {
    precedence: u16,
    code: &'static str,
}

impl StableReason for Reason {
    fn code(&self) -> &'static str {
        self.code
    }

    fn precedence(&self) -> u16 {
        self.precedence
    }
}

const CODES: [&str; 4] = ["", "a", "ab", "b"];

fn least(reasons: &[Reason]) -> Option<Reason> {
    let mut best: Option<&Reason> = None;
    for reason in reasons {
        let better = best.is_none_or(|current| {
            (reason.precedence, reason.code.as_bytes())
                < (current.precedence, current.code.as_bytes())
        });
        if better {
            best = Some(reason);
        }
    }
    best.cloned()
}

fn check_reason_choice(reasons: &[Reason]) {
    let expected = least(reasons);
    assert_eq!(first_reason(reasons.iter().cloned()), expected);
    let (collected, chosen) = collect_and_choose(reasons.iter().cloned());
    assert_eq!(
        collected, reasons,
        "collection must keep every reason in order"
    );
    assert_eq!(chosen, expected);
    let mut reversed = reasons.to_vec();
    reversed.reverse();
    assert_eq!(first_reason(reversed), expected);
}

#[test]
fn reason_choice_holds_on_every_small_input() {
    // Every list of up to four reasons over three precedences and three codes,
    // each checked in every order.
    let alphabet: Vec<Reason> = (0..3_u16)
        .flat_map(|precedence| {
            CODES[..3]
                .iter()
                .map(move |code| Reason { precedence, code })
        })
        .collect();
    let mut cases = 0_u64;
    for_each_sequence(&alphabet, 4, &mut |reasons| {
        let expected = least(reasons);
        for_each_permutation(reasons, &mut |order| {
            assert_eq!(first_reason(order.iter().cloned()), expected);
        });
        check_reason_choice(reasons);
        cases += 1;
    });
    assert_eq!(cases, 1 + 9 + 81 + 729 + 6561);
}

#[test]
fn reason_choice_holds_on_random_inputs() {
    check!().with_type::<Vec<(u16, u8)>>().for_each(|raw| {
        let reasons: Vec<Reason> = raw
            .iter()
            .take(64)
            .map(|(precedence, code)| Reason {
                precedence: *precedence % 8,
                code: CODES[usize::from(*code) % CODES.len()],
            })
            .collect();
        check_reason_choice(&reasons);
    });
}

// ---------------------------------------------------------------------------
// Law codec-canonical: the strict decoder accepts a byte string only if it is
// the canonical encoding of the value it decodes to.
// ---------------------------------------------------------------------------

/// Returns whether the decoder accepted `bytes`.
fn check_canonical_decode(bytes: &[u8]) -> bool {
    let Ok(value) = decode_value(bytes, DecodeLimits::default()) else {
        return false;
    };
    // Admission validates canonical structure, such as key order, without the
    // encoder, so a value the encoder would copy verbatim is still checked.
    assert!(
        AdmittedValue::try_new(value.clone()).is_ok(),
        "the decoder returned a value outside canonical admission: {value:?}"
    );
    let canonical = value
        .canonical_bytes()
        .expect("a decoded value must be canonically encodable");
    assert_eq!(canonical, bytes, "the decoder accepted non-canonical bytes");
    assert_eq!(decode_value(&canonical, DecodeLimits::default()), Ok(value));
    true
}

/// Small values of every kind, nested once.
fn sample_values() -> Vec<Value> {
    let leaves = vec![
        Value::Unit,
        Value::Bool(false),
        Value::Bool(true),
        Value::U128(0),
        Value::U128(255),
        Value::U128(u128::MAX),
        Value::I128(-1),
        Value::I128(i128::MIN),
        Value::Bytes(Box::from([0_u8, 255].as_slice())),
        Value::Text(Box::from("zeno")),
        Value::Enum {
            type_id: 7,
            variant: 1,
        },
        Value::Sum {
            type_id: 7,
            variant: 0,
            payload: None,
        },
    ];
    let mut values = leaves.clone();
    for leaf in &leaves {
        values.push(Value::Tuple(Box::from([leaf.clone(), Value::Unit])));
        values.push(Value::Vector(Box::from([leaf.clone()])));
        values.push(Value::Record(Box::from([
            Field::new(1, leaf.clone()),
            Field::new(2, Value::Bool(true)),
        ])));
        values.push(Value::Sum {
            type_id: 9,
            variant: 2,
            payload: Some(Box::new(leaf.clone())),
        });
        if let Ok(entry) = MapEntry::try_new(leaf.clone(), Value::U128(1)) {
            values.push(Value::Map(Box::from([entry])));
        }
    }
    // Two-element collections, where one changed byte can break key or field
    // order.
    values.push(Value::Record(Box::from([
        Field::new(1, Value::U128(1)),
        Field::new(2, Value::U128(2)),
    ])));
    values.push(Value::Vector(Box::from([Value::U128(1), Value::U128(2)])));
    let mut entries: Vec<MapEntry> = [1_u128, 2]
        .into_iter()
        .map(|key| MapEntry::try_new(Value::U128(key), Value::Unit).expect("map entry"))
        .collect();
    entries.sort_by(|left, right| left.encoded_key().cmp(right.encoded_key()));
    values.push(Value::Map(entries.into_boxed_slice()));
    values
}

#[test]
fn codec_canonical_holds_on_every_small_input() {
    // Every byte string of length at most two, and every single-byte change,
    // one-byte truncation, and one-byte extension of each sample encoding.
    let mut cases = 0_u64;
    let mut accepted = 0_u64;
    let mut check = |bytes: &[u8]| {
        cases += 1;
        accepted += u64::from(check_canonical_decode(bytes));
    };
    check(&[]);
    for first in 0..=255_u8 {
        check(&[first]);
        for second in 0..=255_u8 {
            check(&[first, second]);
        }
    }
    let mut encodings = 0;
    for value in sample_values() {
        let Ok(encoding) = value.canonical_bytes() else {
            continue;
        };
        encodings += 1;
        assert_eq!(
            decode_value(&encoding, DecodeLimits::default()),
            Ok(value),
            "a canonical encoding must decode to its value"
        );
        check(&encoding[..encoding.len() - 1]);
        for position in 0..encoding.len() {
            let mut changed = encoding.clone();
            for byte in 0..=255_u8 {
                changed[position] = byte;
                check(&changed);
            }
        }
        for byte in 0..=255_u8 {
            let mut extended = encoding.clone();
            extended.push(byte);
            check(&extended);
        }
    }
    // The decoder must be exercised on acceptance as well as rejection.
    assert!(encodings >= 60, "only {encodings} sample values encoded");
    assert!(
        cases > 65_536 && accepted > 1_000,
        "{accepted} of {cases} accepted"
    );
}

#[test]
fn codec_canonical_holds_on_random_inputs() {
    check!().with_type::<Vec<u8>>().for_each(|bytes| {
        check_canonical_decode(bytes);
    });
}

#[test]
fn codec_canonical_holds_on_random_changes_to_valid_encodings() {
    let encodings: Vec<Vec<u8>> = sample_values()
        .iter()
        .filter_map(|value| value.canonical_bytes().ok())
        .collect();
    check!()
        .with_type::<(u16, u16, u8)>()
        .for_each(|(which, position, byte)| {
            let mut changed = encodings[usize::from(*which) % encodings.len()].clone();
            let index = usize::from(*position) % changed.len();
            changed[index] = *byte;
            check_canonical_decode(&changed);
        });
}

// ---------------------------------------------------------------------------
// Law patch-overlap: a patch is rejected for overlap exactly when one
// operation's path is a prefix of, or equal to, another's.
// ---------------------------------------------------------------------------

fn overlaps(paths: &[ValuePath]) -> bool {
    paths.iter().enumerate().any(|(i, left)| {
        paths
            .iter()
            .enumerate()
            .any(|(j, right)| i != j && left.is_prefix_of(right))
    })
}

fn check_patch_overlap(paths: &[ValuePath]) {
    let operations: Vec<PatchOp> = paths
        .iter()
        .map(|path| PatchOp::Delete {
            path: path.clone(),
            expected_old_hash: Hash32::ZERO,
        })
        .collect();
    match CanonicalPatch::try_new(1, Hash32::ZERO, operations) {
        Err(PatchError::OverlappingPaths) => {
            assert!(overlaps(paths), "rejected disjoint paths {paths:?}");
        }
        Err(other) => panic!("unexpected {other:?} for {paths:?}"),
        Ok(patch) => {
            assert!(!overlaps(paths), "accepted overlapping paths {paths:?}");
            let mut expected: Vec<Vec<u8>> = paths
                .iter()
                .map(|path| path.canonical_bytes().expect("path encoding"))
                .collect();
            expected.sort();
            let actual: Vec<Vec<u8>> = patch
                .operations()
                .iter()
                .map(|operation| operation.path().canonical_bytes().expect("path encoding"))
                .collect();
            assert_eq!(
                actual, expected,
                "operations must be in canonical path order"
            );
        }
    }
}

#[test]
fn patch_overlap_holds_on_every_small_input() {
    // Every path of length at most three over three segments, and every
    // sequence of up to three such paths.
    let segments = [
        PathSegment::Field(0),
        PathSegment::Field(1),
        PathSegment::TupleIndex(0),
    ];
    let mut paths = Vec::new();
    for_each_sequence(&segments, 3, &mut |segments| {
        paths.push(ValuePath::new(segments.to_vec()));
    });
    assert_eq!(paths.len(), 40);
    let mut cases = 0_u64;
    for_each_sequence(&paths, 3, &mut |selection| {
        check_patch_overlap(selection);
        cases += 1;
    });
    assert_eq!(cases, 1 + 40 + 1_600 + 64_000);
}

#[test]
fn patch_overlap_holds_on_random_inputs() {
    check!().with_type::<Vec<Vec<(u8, u8)>>>().for_each(|raw| {
        let paths: Vec<ValuePath> = raw
            .iter()
            .take(8)
            .map(|segments| {
                ValuePath::new(
                    segments
                        .iter()
                        .take(4)
                        .map(|(kind, index)| match kind % 5 {
                            0 => PathSegment::Field(u16::from(*index % 3)),
                            1 => PathSegment::TupleIndex(u32::from(*index % 3)),
                            2 => PathSegment::VectorIndex(u32::from(*index % 3)),
                            3 => PathSegment::SumPayload,
                            _ => PathSegment::MapKey(Box::from([*index % 3].as_slice())),
                        })
                        .collect(),
                )
            })
            .collect();
        check_patch_overlap(&paths);
    });
}

// ---------------------------------------------------------------------------
// Enumeration helpers.
// ---------------------------------------------------------------------------

/// Calls `visit` with every sequence over `alphabet` of length `0..=max_len`.
fn for_each_sequence<T: Clone>(alphabet: &[T], max_len: usize, visit: &mut dyn FnMut(&[T])) {
    fn extend<T: Clone>(
        alphabet: &[T],
        remaining: usize,
        prefix: &mut Vec<T>,
        visit: &mut dyn FnMut(&[T]),
    ) {
        visit(prefix);
        if remaining == 0 {
            return;
        }
        for item in alphabet {
            prefix.push(item.clone());
            extend(alphabet, remaining - 1, prefix, visit);
            prefix.pop();
        }
    }
    extend(alphabet, max_len, &mut Vec::new(), visit);
}

/// Calls `visit` with every ordering of `items` (Heap's algorithm).
fn for_each_permutation<T: Clone>(items: &[T], visit: &mut dyn FnMut(&[T])) {
    fn permute<T: Clone>(items: &mut [T], size: usize, visit: &mut dyn FnMut(&[T])) {
        if size <= 1 {
            visit(items);
            return;
        }
        for index in 0..size {
            permute(items, size - 1, visit);
            let swap = if size % 2 == 0 { index } else { 0 };
            items.swap(swap, size - 1);
        }
    }
    let mut working = items.to_vec();
    let size = working.len();
    permute(&mut working, size, visit);
}
