//! Differential controls for exact accounting, not timing/proof substitutes.
use super::*;
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
use crate::neural_loop::request::{Policy, Request};
use crate::neural_loop::session::{ResumeHeader, ResumeRefusal};

fn reference_accounting(ledger: &Ledger) -> Accounting {
    let mut totals = Accounting::default();
    let mut reserved = Vec::new();
    let mut settled = Vec::new();
    for entry in &ledger.entries {
        match &entry.kind {
            Kind::Opened { .. } => {}
            Kind::Reserved { attempt, stage } => match stage {
                Stage::Attempt => {
                    totals.attempts = totals.attempts.saturating_add(1);
                    if let Some(attempt) = attempt {
                        reserved.push(*attempt);
                    }
                }
                Stage::ModelCall {
                    input_tokens,
                    output_tokens,
                    money_micros,
                } => {
                    totals.model_calls = totals.model_calls.saturating_add(1);
                    totals.tokens_reserved = totals
                        .tokens_reserved
                        .saturating_add(u64::from(*input_tokens))
                        .saturating_add(u64::from(*output_tokens));
                    totals.money_reserved = totals.money_reserved.saturating_add(*money_micros);
                }
                Stage::Check { work } => {
                    totals.checks = totals.checks.saturating_add(1);
                    totals.work_reserved = totals.work_reserved.saturating_add(*work);
                }
                Stage::Preparation { work } => {
                    totals.work_reserved = totals.work_reserved.saturating_add(*work);
                }
                Stage::Replay { work } => {
                    totals.replays = totals.replays.saturating_add(1);
                    totals.work_reserved = totals.work_reserved.saturating_add(*work);
                }
            },
            Kind::Settled { attempt, .. } => {
                totals.settled = totals.settled.saturating_add(1);
                settled.push(*attempt);
            }
            Kind::Replaced {
                attempt,
                candidate_sha256,
                receipt_sha256,
                cost,
            } => {
                totals.replaced = Some((
                    *attempt,
                    candidate_sha256.clone(),
                    receipt_sha256.clone(),
                    *cost,
                ));
            }
            Kind::Closed { reason } => totals.closed = Some(reason.clone()),
        }
    }
    totals.unresolved = reserved
        .into_iter()
        .filter(|attempt| !settled.contains(attempt))
        .collect();
    totals
}

fn opened(request_id: &str) -> Ledger {
    let mut ledger = Ledger::new();
    ledger.append(Kind::Opened {
        request_id: request_id.into(),
        checker: CheckerIdentity::current(),
    });
    ledger
}
fn reserve(ledger: &mut Ledger, attempt: u8) {
    ledger.append(Kind::Reserved {
        attempt: Some(attempt),
        stage: Stage::Attempt,
    });
}
fn settle(ledger: &mut Ledger, attempt: u8) {
    ledger.append(Kind::Settled {
        attempt,
        outcome: "different".into(),
        candidate_sha256: None,
        cost: None,
    });
}

#[test]
fn all_u8_ids_preserve_duplicate_and_order_semantics() {
    let all: Vec<u8> = (0..=u8::MAX).collect();
    let mut reserved = all.clone();
    reserved.extend(all.iter().rev().copied());
    reserved.extend([0, 255, 7, 7, 255, 0]);
    for settled in [
        Vec::new(),
        vec![7, 7, 0, 255],
        all.clone(),
        all.iter().step_by(2).copied().collect(),
    ] {
        let expected: Vec<u8> = reserved
            .iter()
            .copied()
            .filter(|id| !settled.contains(id))
            .collect();
        assert_eq!(
            membership::unresolved_attempts(&reserved, &settled),
            expected
        );
        assert_eq!(
            membership::unresolved_attempts(&[], &settled),
            Vec::<u8>::new()
        );
    }
    assert_eq!(
        membership::unresolved_attempts(&[3, 1, 3, 2, 1], &[2, 2]),
        vec![3, 1, 3, 1]
    );
}

#[test]
fn full_accounting_matches_original_with_saturation_and_long_strings() {
    let mut ledger = opened(&"r".repeat(64));
    for attempt in 0..=u8::MAX {
        reserve(&mut ledger, attempt);
        reserve(&mut ledger, attempt);
        if attempt % 2 == 0 {
            settle(&mut ledger, attempt);
        }
        ledger.append(Kind::Reserved {
            attempt: Some(attempt),
            stage: Stage::Check { work: u64::MAX },
        });
        ledger.append(Kind::Reserved {
            attempt: None,
            stage: Stage::Preparation { work: 1 },
        });
        ledger.append(Kind::Reserved {
            attempt: None,
            stage: Stage::ModelCall {
                input_tokens: u32::MAX,
                output_tokens: u32::MAX,
                money_micros: u64::MAX,
            },
        });
    }
    for suffix in ["first", "last"] {
        ledger.append(Kind::Replaced {
            attempt: 7,
            candidate_sha256: format!("{}-{suffix}", "c".repeat(4096)),
            receipt_sha256: "d".repeat(8192),
            cost: Cost {
                nodes: 5,
                bytes: 200,
            },
        });
    }
    ledger.append(Kind::Closed {
        reason: "long\nreason".repeat(1024),
    });
    for _ in 0..256 {
        ledger.append(Kind::Reserved {
            attempt: None,
            stage: Stage::Replay { work: u64::MAX },
        });
    }
    let expected = reference_accounting(&ledger);
    assert_eq!(ledger.accounting(), expected);
    assert_eq!(expected.attempts, u8::MAX);
    assert_eq!(expected.checks, u8::MAX);
    assert_eq!(expected.replays, u8::MAX);
    assert_eq!(expected.work_reserved, u64::MAX);
    assert_eq!(expected.money_reserved, u64::MAX);
    let unresolved: Vec<u8> = (0..=u8::MAX)
        .filter(|id| id % 2 == 1)
        .flat_map(|id| [id, id])
        .collect();
    assert_eq!(expected.unresolved, unresolved);
    assert_eq!(
        Ledger::from_entries(ledger.entries().to_vec())
            .unwrap_or_else(|error| panic!("{error:?}"))
            .accounting(),
        expected
    );
}

#[test]
fn long_hash_consistent_adversarial_history_remains_accepted_and_exact() {
    let mut ledger = opened(&"r".repeat(64));
    // Every old membership lookup scans the full unrelated settlement list.
    for _ in 0..3000 {
        reserve(&mut ledger, 1);
    }
    for _ in 0..3000 {
        settle(&mut ledger, 2);
    }
    let bytes: Vec<u8> = ledger.entries().iter().flat_map(Entry::line).collect();
    assert!(bytes.len() > 1024 * 1024);
    assert!(bytes.len() as u64 <= crate::neural_loop::acquisition::LEDGER_BYTES + 1);
    let parsed = Ledger::parse_lines(&bytes).unwrap_or_else(|error| panic!("{error:?}"));
    let admitted = Ledger::from_entries(parsed.clone()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(admitted.head(), ledger.head());
    assert_eq!(admitted.accounting(), reference_accounting(&admitted));
    assert_eq!(admitted.accounting().unresolved, vec![1; 3000]);
    let mut tampered = parsed;
    tampered[1].kind = Kind::Reserved {
        attempt: Some(2),
        stage: Stage::Attempt,
    };
    assert!(matches!(
        Ledger::from_entries(tampered),
        Err(LedgerFault::Digest { line: 1 })
    ));
    // A later settlement removes every duplicate, preserving global membership semantics.
    settle(&mut ledger, 1);
    assert_eq!(ledger.accounting(), reference_accounting(&ledger));
    assert!(ledger.accounting().unresolved.is_empty());
}

#[test]
fn admitted_header_reuses_exact_totals_and_keeps_binding_refusal_order() {
    let original =
        include_bytes!("../../tests/fixtures/i128-programs/boolean-kernel-original.zcve");
    let request = Request::admit(
        original,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    let mut ledger = opened(request.id());
    for id in [7, 1, 7, 255, 1, 0] {
        reserve(&mut ledger, id);
    }
    settle(&mut ledger, 1);
    let bytes: Vec<u8> = ledger.entries().iter().flat_map(Entry::line).collect();
    let header = ResumeHeader::admit(
        request.canonical_bytes(),
        original,
        &bytes,
        Some(ledger.head()),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(header.accounting(), &reference_accounting(&ledger));
    assert_eq!(header.accounting().unresolved, vec![7, 7, 255, 0]);
    assert_eq!(header.ledger().head(), ledger.head());
    let wrong = opened("wrong-request");
    let wrong_bytes: Vec<u8> = wrong.entries().iter().flat_map(Entry::line).collect();
    assert!(matches!(
        ResumeHeader::admit(
            request.canonical_bytes(),
            original,
            &wrong_bytes,
            Some(wrong.head())
        ),
        Err(ResumeRefusal::RequestMismatch)
    ));
    assert!(matches!(
        ResumeHeader::admit(b"bad request", original, b"bad ledger", None),
        Err(ResumeRefusal::Request(_))
    ));
}

#[test]
fn owned_accounting_pair_preserves_raw_latest_values_and_consumption_boundary() {
    let mut ledger = Ledger::new();
    // Deliberately raw history: accounting has no admitted-history premise.
    ledger.append(Kind::Closed {
        reason: "old\n🦀".repeat(512),
    });
    ledger.append(Kind::Reserved {
        attempt: Some(7),
        stage: Stage::Attempt,
    });
    ledger.append(Kind::Reserved {
        attempt: Some(7),
        stage: Stage::Attempt,
    });
    ledger.append(Kind::Reserved {
        attempt: None,
        stage: Stage::Attempt,
    });
    for (candidate, receipt) in [("old🦀", "oldé"), ("", "last\n🦀")] {
        ledger.append(Kind::Replaced {
            attempt: 255,
            candidate_sha256: candidate.into(),
            receipt_sha256: receipt.into(),
            cost: Cost {
                nodes: u64::MAX,
                bytes: 0,
            },
        });
    }
    ledger.append(Kind::Closed {
        reason: String::new(),
    });
    let before_head = ledger.head();
    let before_lines: Vec<Vec<u8>> = ledger.entries().iter().map(Entry::line).collect();
    let expected = reference_accounting(&ledger);
    let owned = account_ledger(ledger);
    assert_eq!(owned.totals(), &expected);
    assert_eq!(owned.ledger().head(), before_head);
    assert_eq!(
        owned
            .ledger()
            .entries()
            .iter()
            .map(Entry::line)
            .collect::<Vec<_>>(),
        before_lines
    );
    assert_eq!(owned.totals().unresolved, vec![7, 7]);
    assert_eq!(owned.totals().closed, Some(String::new()));
    assert_eq!(
        owned
            .totals()
            .replaced
            .as_ref()
            .unwrap_or_else(|| panic!("required test value"))
            .1,
        ""
    );
    assert_eq!(
        owned
            .totals()
            .replaced
            .as_ref()
            .unwrap_or_else(|| panic!("required test value"))
            .2,
        "last\n🦀"
    );
    let (mut ledger, prior) = owned.into_parts();
    assert_eq!(prior, ledger.accounting());
    ledger.append(Kind::Reserved {
        attempt: None,
        stage: Stage::Replay { work: 23 },
    });
    let later = ledger.accounting();
    assert_eq!(prior.work_reserved, 0);
    assert_eq!(later.work_reserved, 23);
    assert_eq!(prior.attempts, later.attempts);
    assert_eq!(prior.replaced, later.replaced);
    assert_eq!(prior.closed, later.closed);
}

#[test]
fn owned_accounting_pair_empty_has_exact_zero_totals() {
    let owned = account_ledger(Ledger::new());
    assert_eq!(owned.totals(), &Accounting::default());
    assert!(owned.ledger().entries().is_empty());
    let (ledger, totals) = owned.into_parts();
    assert_eq!(totals, reference_accounting(&ledger));
}

#[test]
fn explicit_default_and_clone_traits_preserve_all_owned_values() {
    let zero = Accounting::default();
    assert_eq!(zero.clone(), zero);
    assert_eq!(
        (
            zero.attempts,
            zero.checks,
            zero.replays,
            zero.model_calls,
            zero.settled
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(
        (
            zero.tokens_reserved,
            zero.money_reserved,
            zero.work_reserved
        ),
        (0, 0, 0)
    );
    assert!(zero.unresolved.is_empty());
    assert!(zero.replaced.is_none() && zero.closed.is_none());
    let checker = CheckerIdentity {
        semantics: "\n🦀".repeat(100),
        evaluator_identity: String::new(),
    };
    assert_eq!(checker.clone(), checker);
    let mut ledger = Ledger::new();
    ledger.append(Kind::Opened {
        request_id: "é".into(),
        checker,
    });
    for stage in [
        Stage::Attempt,
        Stage::Preparation { work: u64::MAX },
        Stage::ModelCall {
            input_tokens: u32::MAX,
            output_tokens: 7,
            money_micros: u64::MAX,
        },
        Stage::Check { work: 3 },
        Stage::Replay { work: 5 },
    ] {
        assert_eq!(stage.clone(), stage);
        ledger.append(Kind::Reserved {
            attempt: Some(255),
            stage,
        });
    }
    for candidate in [None, Some("owned🦀".into())] {
        ledger.append(Kind::Settled {
            attempt: 255,
            outcome: "resulté".into(),
            candidate_sha256: candidate,
            cost: Some(Cost { nodes: 3, bytes: 7 }),
        });
    }
    ledger.append(Kind::Replaced {
        attempt: 255,
        candidate_sha256: "candidate🦀".repeat(100),
        receipt_sha256: String::new(),
        cost: Cost {
            nodes: u64::MAX,
            bytes: 7,
        },
    });
    for attempt in [7, 1, 7] {
        ledger.append(Kind::Reserved {
            attempt: Some(attempt),
            stage: Stage::Attempt,
        });
    }
    ledger.append(Kind::Closed {
        reason: "closedé".into(),
    });
    for entry in ledger.entries() {
        assert_eq!(entry.kind.clone(), entry.kind);
        assert_eq!(entry.clone(), *entry);
        assert_eq!(entry.clone().line(), entry.line());
    }
    let cloned = ledger.clone();
    assert_eq!(cloned, ledger);
    assert_eq!(cloned.head(), ledger.head());
    let head = ledger.head();
    assert_eq!(head.clone(), head);
    let totals = ledger.accounting();
    assert_eq!(totals.unresolved, vec![7, 1, 7]);
    assert_eq!(totals.clone().unresolved, vec![7, 1, 7]);
    assert_eq!(totals.clone(), totals);
    assert_eq!(totals.clone().json(), totals.json());
    for fault in [
        LedgerFault::Unreadable { line: usize::MAX },
        LedgerFault::Sequence { line: 0 },
        LedgerFault::Link { line: 1 },
        LedgerFault::Digest { line: 2 },
        LedgerFault::Opened { line: 3 },
        LedgerFault::AfterClose { line: 4 },
    ] {
        assert_eq!(fault.clone(), fault);
        assert_eq!(fault.clone().json(), fault.json());
    }
}
