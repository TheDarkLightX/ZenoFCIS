//! Versioned flat table schema and canonical reconstruction for native lifecycle.
//! IDs, units, option tags and C/I/B/F are part of the schema/profile identity.
use super::aggregation::Aggregate;
use super::bonds::{BondCommand, DisputePhase, FinalVerdict, ReporterPhase, Verdict};
use super::lifecycle::{
    FeedRegistration, LifecycleCommand, LifecycleContext, LifecycleProfile, LifecycleState,
    NativePriceCommand, NativeRewardCommand, RetainedKey,
};
use super::reports::{RawReport, ReportPayload};
use super::rewards::ClaimPhase;
use super::types::{BLS_POP_SCHEME, EscrowRecipient, Hash, Id, REASONS, Reason};
use super::wire;
use super::{Evaluation, Failure, Limits, measured, zero_limits};
use crate::finite::execution_v2::tables::{Descriptor, FieldType, Shape, Type};
use alloc::{vec, vec::Vec};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_value::{MapEntry, Value, ValueRef};

const HASH_OPTION: u32 = 90;
const U64_OPTION: u32 = 91; // optional epoch; retained numeric ID
const KEY_OPTION: u32 = 94;
const SECONDS_OPTION: u32 = 95;
const SIG_OPTION: u32 = 92;
const PRICE_OPTION: u32 = 93;
const VERDICT_OPTION: u32 = 96;
const SEQUENCE_OPTION: u32 = 97;
fn maybe_verdict(v: Option<&FinalVerdict>) -> Result<Value, Failure> {
    Ok(Value::sum(
        VERDICT_OPTION,
        u16::from(v.is_some()),
        v.map(verdict_value).transpose()?,
    ))
}
pub(super) fn maybe_u64(x: Option<u64>) -> Result<Value, Failure> {
    Ok(Value::sum(
        U64_OPTION,
        u16::from(x.is_some()),
        x.map(|n| Value::unsigned(u128::from(n))),
    ))
}
fn maybe_hash(x: Option<Hash>) -> Result<Value, Failure> {
    Ok(Value::sum(
        HASH_OPTION,
        u16::from(x.is_some()),
        x.map(|h| wire::blob(&h)).transpose()?,
    ))
}
pub(super) fn maybe_key_version(x: Option<u64>) -> Result<Value, Failure> {
    Ok(Value::sum(KEY_OPTION, u16::from(x.is_some()), x.map(n)))
}
fn maybe_seconds(x: Option<u64>) -> Result<Value, Failure> {
    Ok(Value::sum(SECONDS_OPTION, u16::from(x.is_some()), x.map(n)))
}
fn maybe_sig(x: Option<[u8; 96]>) -> Result<Value, Failure> {
    Ok(Value::sum(
        SIG_OPTION,
        u16::from(x.is_some()),
        x.map(|s| wire::blob(&s)).transpose()?,
    ))
}
fn n(x: u64) -> Value {
    Value::unsigned(u128::from(x))
}
fn number(x: u128) -> Value {
    Value::unsigned(x)
}
fn table(mut entries: Vec<(Value, Value)>) -> Result<Value, Failure> {
    let mut rows = Vec::new();
    for (key, value) in entries.drain(..) {
        rows.push(MapEntry::try_new(key, value).map_err(|_| Failure::Canonical)?);
    }
    rows.sort_by(|a, b| a.encoded_key().cmp(b.encoded_key()));
    Value::map_canonical(rows).map_err(|_| Failure::Canonical)
}
fn tuple(xs: Vec<Value>) -> Result<Value, Failure> {
    Value::tuple(xs).map_err(|_| Failure::Canonical)
}
fn phase(x: ReporterPhase) -> u64 {
    match x {
        ReporterPhase::Active => 0,
        ReporterPhase::Unbonding => 1,
        ReporterPhase::Retired => 2,
    }
}
fn dispute_phase(x: DisputePhase) -> u64 {
    match x {
        DisputePhase::Open => 0,
        DisputePhase::ResolvedUpheld => 1,
        DisputePhase::ResolvedRejected => 2,
    }
}
fn recipient(x: &EscrowRecipient) -> u64 {
    match x {
        EscrowRecipient::Challenger => 0,
        EscrowRecipient::ReporterOwner => 1,
        EscrowRecipient::Treasury => 2,
        EscrowRecipient::Burn => 3,
    }
}

pub(super) fn profile_value(p: &LifecycleProfile) -> Result<Value, Failure> {
    let c = p
        .parameters
        .clock
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    let d = p
        .parameters
        .disputes
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    let e = p
        .parameters
        .economy
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    wire::record(vec![
        wire::blob(BLS_POP_SCHEME.as_bytes())?,
        wire::blob(p.zdex_asset.bytes())?,
        wire::record(vec![
            number(p.reward_split.reporter),
            number(p.reward_split.treasury),
            number(p.reward_split.burn),
            number(p.reward_split.denominator),
            wire::blob(p.reward_split.treasury_account.bytes())?,
        ])?,
        wire::record(vec![
            wire::blob(&c.backend)?,
            n(c.heartbeat),
            n(c.confirmation),
            n(c.dispute_open),
            n(c.inclusion),
            n(c.max_age),
            n(c.max_epoch_seconds),
            n(c.max_time_lag),
        ])?,
        wire::record(vec![
            wire::blob(&d.adjudicator)?,
            number(d.minimum_bond.0),
            number(d.challenge_bond.0),
            number(d.slash.0),
            n(d.exit_delay),
            n(d.finality_delay),
            n(d.appeal_delay),
            Value::boolean(d.false_price_slashable),
            wire::blob(d.treasury.bytes())?,
            n(recipient(&d.slash_recipient)),
            n(recipient(&d.upheld_bond_recipient)),
            n(recipient(&d.rejected_bond_recipient)),
        ])?,
        wire::record(vec![
            wire::blob(e.reward_unit.asset.bytes())?,
            n(u64::from(e.reward_unit.scale)),
            number(e.reward_per_report),
            number(e.alpha_numerator),
            number(e.alpha_denominator),
            wire::blob(&e.stress_source)?,
            number(e.stress_haircut_numerator),
            number(e.stress_haircut_denominator),
            number(e.debt_ceiling.0),
            number(e.liquidation_cap.0),
            number(e.redemption_cap.0),
            number(e.other_cap.0),
            n(e.window),
        ])?,
        n(u64::from(p.bounds.max_entries)),
        n(p.bounds.max_item_bytes),
        n(p.bounds.max_snapshot_bytes),
        n(p.frame_bytes),
        wire::blob(
            zeno_fcis_crypto::RustCryptoSha256::hash(
                &schema(p)?.schema_bytes().map_err(|_| Failure::Canonical)?,
            )
            .as_bytes(),
        )?,
        wire::blob(
            zeno_fcis_crypto::RustCryptoSha256::hash(include_bytes!(
                "../../../../../../verification/oracle_crypto_profile.json"
            ))
            .as_bytes(),
        )?,
        wire::blob(&super::super::authority::EVALUATOR)?,
    ])
}

pub(super) fn feed_value(f: &FeedRegistration, include_id: bool) -> Result<Value, Failure> {
    if f.sources.windows(2).any(|w| w[0].id >= w[1].id) {
        return Err(Failure::Canonical);
    }
    let mut rows = vec![
        wire::blob(if include_id { &f.rules.feed } else { &[0; 32] })?,
        n(f.version),
        wire::blob(f.base.bytes())?,
        wire::blob(f.quote.bytes())?,
        wire::blob(&f.rules.profile)?,
        wire::blob(&f.rules.query)?,
        number(f.rules.max_deviation_bps),
        wire::integers(
            &f.rules
                .diversity
                .minimum
                .iter()
                .chain(f.rules.diversity.max_same_class.iter())
                .map(|v| u128::from(*v))
                .collect::<Vec<_>>(),
        )?,
        n(f.first_open),
        n(f.cutoff_delta),
        wire::record(vec![
            wire::blob(&f.rules.clock.backend)?,
            n(f.rules.clock.heartbeat),
            n(f.rules.clock.confirmation),
            n(f.rules.clock.dispute_open),
            n(f.rules.clock.inclusion),
            n(f.rules.clock.max_age),
            n(f.rules.clock.max_epoch_seconds),
            n(f.rules.clock.max_time_lag),
        ])?,
        wire::integers(&[
            u128::from(f.rules.bounds.max_entries),
            u128::from(f.rules.bounds.max_item_bytes),
            u128::from(f.rules.bounds.max_snapshot_bytes),
            u128::from(f.rules.frame_bytes),
        ])?,
    ];
    rows.push(table(
        f.sources
            .iter()
            .map(|s| Ok((wire::blob(s.id.bytes())?, wire::source(s)?)))
            .collect::<Result<_, Failure>>()?,
    )?);
    rows.push(table(
        f.assignments
            .iter()
            .map(|(a, b)| Ok((wire::blob(a.bytes())?, wire::blob(b.bytes())?)))
            .collect::<Result<_, Failure>>()?,
    )?);
    wire::record(rows)
}
fn feed_row(f: &FeedRegistration) -> Result<Value, Failure> {
    wire::record(vec![
        n(f.version),
        wire::blob(f.base.bytes())?,
        wire::blob(f.quote.bytes())?,
        wire::blob(&f.rules.profile)?,
        wire::blob(&f.rules.query)?,
        number(f.rules.max_deviation_bps),
        wire::integers(
            &f.rules
                .diversity
                .minimum
                .iter()
                .chain(f.rules.diversity.max_same_class.iter())
                .map(|v| u128::from(*v))
                .collect::<Vec<_>>(),
        )?,
        n(f.first_open),
        n(f.cutoff_delta),
    ])
}
fn key_value(k: &RetainedKey) -> Result<Value, Failure> {
    wire::record(vec![
        wire::blob(BLS_POP_SCHEME.as_bytes())?,
        wire::blob(k.key.owner.bytes())?,
        wire::blob(&k.key.public_key)?,
        wire::blob(&k.key.possession)?,
        n(k.activated),
        maybe_u64(k.retired)?,
        n(k.activation_sequence),
        Value::sum(
            SEQUENCE_OPTION,
            u16::from(k.retirement_sequence.is_some()),
            k.retirement_sequence.map(n),
        ),
        wire::blob(&k.registration_root)?,
        n(k.registration_nonce),
        maybe_key_version(k.old_version)?,
        wire::blob(&k.authorization)?,
        maybe_sig(k.old_authorization)?,
    ])
}
fn payload_value(p: &ReportPayload) -> Result<Value, Failure> {
    wire::record(vec![
        wire::blob(p.chain.bytes())?,
        wire::blob(p.reporter.bytes())?,
        n(p.key_version),
        wire::blob(&p.feed)?,
        wire::blob(&p.profile)?,
        wire::blob(&p.query)?,
        wire::blob(p.source.bytes())?,
        n(p.round),
        number(p.value.raw()),
        n(p.observed_epoch),
        n(p.observation_seconds),
        n(p.sequence),
        maybe_hash(p.previous)?,
        wire::blob(BLS_POP_SCHEME.as_bytes())?,
    ])
}
fn raw_report_value(r: &RawReport) -> Result<Value, Failure> {
    wire::record(vec![
        payload_value(&r.payload)?,
        wire::blob(&r.payload_hash)?,
        wire::blob(&r.id)?,
        wire::blob(&r.signature)?,
    ])
}

/// Field IDs 5..19 below are stable named flat tables. Every join key resolves
/// in this same root. State/header/profile fields occupy IDs 1..4.
pub(super) fn state_value(s: &LifecycleState) -> Result<Value, Failure> {
    let mut fs = vec![
        wire::blob(b"zeno.oracle.native_state/1")?,
        wire::blob(s.chain.bytes())?,
        wire::blob(&wire::bytes(&profile_value(&s.profile)?)?)?,
        n(s.epoch),
    ];
    fs.push(table(
        s.nonces
            .iter()
            .map(|(k, v)| Ok((wire::blob(k.bytes())?, n(*v))))
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.wallets
            .iter()
            .map(|(k, v)| Ok((wire::blob(k.bytes())?, number(v.0))))
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.feeds
            .iter()
            .map(|(k, v)| Ok((wire::blob(k)?, feed_row(v)?)))
            .collect::<Result<_, Failure>>()?,
    )?);
    let mut sources = Vec::new();
    let mut assignments = Vec::new();
    for (feed, f) in &s.feeds {
        for source in &f.sources {
            sources.push((
                tuple(vec![wire::blob(feed)?, wire::blob(source.id.bytes())?])?,
                wire::source(source)?,
            ));
        }
        for (reporter, source) in &f.assignments {
            assignments.push((
                tuple(vec![wire::blob(feed)?, wire::blob(reporter.bytes())?])?,
                wire::blob(source.bytes())?,
            ));
        }
    }
    fs.push(table(sources)?);
    fs.push(table(assignments)?);
    fs.push(table(
        s.keys
            .iter()
            .map(|((r, v), k)| Ok((tuple(vec![wire::blob(r.bytes())?, n(*v)])?, key_value(k)?)))
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.reporters
            .iter()
            .map(|(id, r)| {
                Ok((
                    wire::blob(id.bytes())?,
                    wire::record(vec![
                        wire::blob(r.owner.bytes())?,
                        n(r.active_version),
                        n(r.registered),
                        n(r.chain.next_sequence),
                        maybe_hash(r.chain.previous)?,
                        maybe_seconds(r.chain.last_seconds)?,
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.rounds
            .iter()
            .map(|((f, r), v)| {
                Ok((
                    tuple(vec![wire::blob(f)?, n(*r)])?,
                    wire::record(vec![
                        n(v.opening),
                        n(v.cutoff),
                        maybe_hash(v.aggregate)?,
                        maybe_hash(v.cutoff_admissions)?,
                        maybe_u64(v.built_epoch)?,
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    let mut eligible = Vec::new();
    for ((feed, round), r) in &s.rounds {
        for id in &r.eligible {
            eligible.push((
                tuple(vec![wire::blob(feed)?, n(*round), wire::blob(id.bytes())?])?,
                Value::unit(),
            ));
        }
    }
    fs.push(table(eligible)?);
    fs.push(table(
        s.reports
            .iter()
            .map(|(id, r)| {
                Ok((
                    wire::blob(id)?,
                    wire::record(vec![
                        raw_report_value(&r.report)?,
                        n(r.submitted),
                        wire::blob(&r.admission)?,
                        wire::blob(&r.liability)?,
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.aggregates
            .iter()
            .map(|(id, a)| Ok((wire::blob(id)?, a.value()?)))
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.bonds
            .iter()
            .map(|(id, b)| {
                Ok((
                    wire::blob(id.bytes())?,
                    wire::record(vec![
                        wire::blob(b.owner.bytes())?,
                        n(phase(b.phase)),
                        number(b.held.0),
                        number(b.burned.0),
                        maybe_u64(b.exit_epoch)?,
                        n(b.epoch),
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    let mut liabilities = Vec::new();
    let mut disputes = Vec::new();
    for (reporter, b) in &s.bonds {
        for l in &b.liabilities {
            liabilities.push((
                tuple(vec![wire::blob(reporter.bytes())?, wire::blob(&l.id)?])?,
                wire::record(vec![
                    wire::blob(&l.report)?,
                    wire::blob(&l.feed)?,
                    number(l.locked.0),
                    maybe_u64(l.publication)?,
                    n(l.exposure_until),
                    n(l.clear_after),
                    Value::boolean(l.cleared),
                ])?,
            ));
        }
        for d in &b.disputes {
            disputes.push((
                wire::blob(&d.id)?,
                wire::record(vec![
                    wire::blob(reporter.bytes())?,
                    wire::blob(&d.liability)?,
                    wire::blob(d.challenger.bytes())?,
                    number(d.escrow.0),
                    n(d.opened),
                    n(dispute_phase(d.phase)),
                    number(d.slash.0),
                    maybe_hash(d.violation)?,
                    maybe_verdict(d.slash_verdict.as_ref())?,
                    maybe_verdict(d.resolution_verdict.as_ref())?,
                ])?,
            ));
        }
    }
    fs.push(table(liabilities)?);
    fs.push(table(disputes)?);
    fs.push(table(
        s.stress
            .iter()
            .map(|(feed, o)| {
                Ok((
                    wire::blob(feed)?,
                    wire::record(vec![
                        wire::blob(&o.source)?,
                        wire::blob(&o.valuation_root)?,
                        Value::sum(
                            PRICE_OPTION,
                            u16::from(o.value.is_some()),
                            o.value.map(|v| number(v.0)),
                        ),
                        n(o.observed_epoch),
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.prices
            .iter()
            .map(|(feed, p)| {
                Ok((
                    wire::blob(feed)?,
                    wire::record(vec![
                        maybe_hash(p.committed.as_ref().map(Aggregate::id))?,
                        maybe_hash(p.pending.as_ref().map(|q| q.aggregate.id()))?,
                        maybe_u64(p.pending.as_ref().map(|q| q.published))?,
                        maybe_u64(p.pending.as_ref().map(|q| q.dispute_until))?,
                        maybe_u64(p.pending.as_ref().map(|q| q.earliest))?,
                        n(p.watermark),
                        n(p.epoch),
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    let mut history = Vec::new();
    for (feed, p) in &s.prices {
        for a in &p.history {
            history.push((
                tuple(vec![wire::blob(feed)?, n(a.round())])?,
                wire::blob(&a.id())?,
            ));
        }
    }
    fs.push(table(history)?);
    fs.push(table(
        s.time_quorums
            .iter()
            .map(|(feed, q)| {
                Ok((
                    wire::blob(feed)?,
                    wire::record(vec![
                        n(q.round),
                        tuple(
                            q.reports
                                .iter()
                                .map(|id| wire::blob(id))
                                .collect::<Result<_, Failure>>()?,
                        )?,
                        n(q.oldest_epoch),
                        n(q.seconds),
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.reward_wallets
            .iter()
            .map(|(id, n)| Ok((wire::blob(id.bytes())?, number(*n))))
            .collect::<Result<_, Failure>>()?,
    )?);
    fs.push(table(
        s.rewards
            .iter()
            .map(|(feed, r)| {
                Ok((
                    wire::blob(feed)?,
                    wire::record(vec![
                        wire::blob(r.unit.asset.bytes())?,
                        n(u64::from(r.unit.scale)),
                        number(r.funded),
                        number(r.remaining),
                        number(r.paid),
                        number(r.treasury_paid),
                        number(r.burned),
                        number(r.residual),
                        n(r.epoch),
                    ])?,
                ))
            })
            .collect::<Result<_, Failure>>()?,
    )?);
    let mut claims = Vec::new();
    for (feed, r) in &s.rewards {
        for c in &r.claims {
            claims.push((
                tuple(vec![wire::blob(feed)?, wire::blob(&c.id)?])?,
                wire::record(vec![
                    wire::blob(&c.aggregate)?,
                    wire::blob(&c.report)?,
                    wire::blob(&c.liability)?,
                    wire::blob(c.beneficiary.bytes())?,
                    number(c.amount),
                    n(match c.phase {
                        ClaimPhase::Held => 0,
                        ClaimPhase::Paid => 1,
                        ClaimPhase::ClawedBack => 2,
                    }),
                ])?,
            ));
        }
    }
    fs.push(table(claims)?);
    wire::record(fs)
}

fn ty(id: u32, name: &str, shape: Shape) -> Type {
    Type {
        id,
        name: name.as_bytes().to_vec(),
        shape,
    }
}
fn record_type(id: u32, name: &str, fields: &[u32]) -> Type {
    ty(
        id,
        name,
        Shape::Record(
            fields
                .iter()
                .enumerate()
                .map(|(i, t)| FieldType {
                    id: (i + 1) as u16,
                    name: alloc::format!("field{}", i + 1).into_bytes(),
                    type_id: *t,
                })
                .collect(),
        ),
    )
}
pub(super) fn schema(p: &LifecycleProfile) -> Result<Descriptor, Failure> {
    let bytes = |min, max| Shape::Bytes { min, max };
    let unsigned = |min, max| Shape::U128 { min, max };
    let mut types = vec![
        ty(1, "Unit", Shape::Unit),
        ty(2, "Bool", Shape::Bool),
        ty(25, "RewardAmountBoundByProfile", unsigned(0, u128::MAX)),
        ty(26, "RewardScale", unsigned(0, 38)),
        ty(3, "U128", unsigned(0, u128::MAX)),
        ty(4, "U64", unsigned(0, u128::from(u64::MAX))),
        ty(5, "OpaqueId", bytes(1, 64)),
        ty(6, "Hash32", bytes(32, 32)),
        ty(7, "G1PublicKey", bytes(48, 48)),
        ty(8, "G2Signature", bytes(96, 96)),
        ty(9, "ProfileBytes", bytes(1, 8192)),
        ty(10, "ZdexE18", unsigned(0, u128::MAX)),
        ty(11, "PriceUsdPerAgrsE8", unsigned(1, u128::MAX)),
        ty(12, "StressUsdPerZdexE8", unsigned(0, u128::MAX)),
        ty(13, "Epoch", unsigned(0, u128::from(u64::MAX))),
        ty(14, "ObservationSeconds", unsigned(0, u128::from(u64::MAX))),
        ty(15, "Nonce", unsigned(0, u128::from(u64::MAX))),
        ty(16, "KeyVersion", unsigned(0, u128::from(u64::MAX))),
        ty(17, "ReportSequence", unsigned(0, u128::from(u64::MAX))),
        ty(18, "Round", unsigned(0, u128::from(u64::MAX))),
        ty(19, "Bps", unsigned(0, u128::MAX)),
        ty(20, "Phase", unsigned(0, 2)),
        ty(21, "Concentration", unsigned(1, 3)),
        ty(HASH_OPTION, "OptionalHash", Shape::Maybe(6)),
        ty(U64_OPTION, "OptionalEpoch", Shape::Maybe(13)),
        ty(KEY_OPTION, "OptionalKeyVersion", Shape::Maybe(16)),
        ty(
            SECONDS_OPTION,
            "OptionalObservationSeconds",
            Shape::Maybe(14),
        ),
        ty(SIG_OPTION, "OptionalSignature", Shape::Maybe(8)),
        ty(PRICE_OPTION, "OptionalStress", Shape::Maybe(12)),
        ty(VERDICT_OPTION, "OptionalVerdict", Shape::Maybe(116)),
        ty(SEQUENCE_OPTION, "OptionalReportSequence", Shape::Maybe(17)),
    ];
    types.push(record_type(
        110,
        "ReportPayload",
        &[5, 5, 16, 6, 6, 6, 5, 18, 11, 13, 14, 17, HASH_OPTION, 5],
    ));
    types.push(record_type(111, "RawReport", &[110, 6, 6, 8]));
    types.push(ty(112, "ReportTriple", Shape::Tuple(vec![6, 6, 6])));
    types.push(ty(113, "IdentityTriple", Shape::Tuple(vec![5, 5, 5])));
    types.push(ty(
        114,
        "AggregateNumbers",
        Shape::Tuple(vec![11, 3, 19, 13, 14]),
    ));
    types.push(ty(115, "DiversityBounds", Shape::Tuple(vec![21; 10])));
    types.push(record_type(116, "FinalVerdict", &[6, 6, 6, 2, 13, 6]));
    let specs: Vec<(&str, Vec<u32>, Vec<u32>)> = vec![
        ("Nonces", vec![5], vec![15]),
        ("WalletZdex", vec![5], vec![10]),
        ("Feeds", vec![6], vec![4, 5, 5, 6, 6, 19, 115, 13, 4]),
        ("Sources", vec![6, 5], vec![5, 5, 5, 5, 5, 5]),
        ("Assignments", vec![6, 5], vec![5]),
        (
            "Keys",
            vec![5, 16],
            vec![
                5,
                5,
                7,
                8,
                13,
                U64_OPTION,
                17,
                SEQUENCE_OPTION,
                6,
                15,
                KEY_OPTION,
                8,
                SIG_OPTION,
            ],
        ),
        (
            "Reporters",
            vec![5],
            vec![5, 16, 13, 17, HASH_OPTION, SECONDS_OPTION],
        ),
        (
            "Rounds",
            vec![6, 18],
            vec![13, 13, HASH_OPTION, HASH_OPTION, U64_OPTION],
        ),
        ("Eligible", vec![6, 18, 5], vec![1]),
        ("Reports", vec![6], vec![111, 13, 6, 6]),
        (
            "Aggregates",
            vec![6],
            vec![6, 6, 6, 18, 112, 112, 113, 113, 114],
        ),
        ("Bonds", vec![5], vec![5, 20, 10, 10, U64_OPTION, 13]),
        (
            "Liabilities",
            vec![5, 6],
            vec![6, 6, 10, U64_OPTION, 13, 13, 2],
        ),
        (
            "Disputes",
            vec![6],
            vec![
                5,
                6,
                5,
                10,
                13,
                20,
                10,
                HASH_OPTION,
                VERDICT_OPTION,
                VERDICT_OPTION,
            ],
        ),
        ("Stress", vec![6], vec![6, 6, PRICE_OPTION, 13]),
        (
            "Prices",
            vec![6],
            vec![
                HASH_OPTION,
                HASH_OPTION,
                U64_OPTION,
                U64_OPTION,
                U64_OPTION,
                14,
                13,
            ],
        ),
        ("PriceHistory", vec![6, 18], vec![6]),
        ("TimeQuorums", vec![6], vec![18, 112, 13, 14]),
        ("RewardWallets", vec![5], vec![25]),
        (
            "RewardBudgets",
            vec![6],
            vec![5, 26, 25, 25, 25, 25, 25, 25, 13],
        ),
        ("RewardClaims", vec![6, 6], vec![6, 6, 6, 5, 25, 20]),
    ];
    let mut root_fields = vec![5, 5, 9, 13];
    for (i, (name, keys, values)) in specs.into_iter().enumerate() {
        let i = u32::try_from(i).map_err(|_| Failure::Arithmetic)?;
        let key = if keys.len() == 1 {
            keys[0]
        } else {
            types.push(ty(
                500 + i,
                &alloc::format!("{name}Key"),
                Shape::Tuple(keys),
            ));
            500 + i
        };
        let value = if values.len() == 1
            && [
                "Nonces",
                "WalletZdex",
                "Assignments",
                "Eligible",
                "PriceHistory",
                "RewardWallets",
            ]
            .contains(&name)
        {
            values[0]
        } else {
            types.push(record_type(300 + i, &alloc::format!("{name}Row"), &values));
            300 + i
        };
        types.push(ty(
            400 + i,
            name,
            Shape::Table {
                key,
                value,
                set: name == "Eligible",
                limits: p.bounds,
            },
        ));
        root_fields.push(400 + i);
    }
    types.push(record_type(600, "OracleLifecycleStateV1", &root_fields));
    types.push(record_type(601, "Empty", &[]));
    let blob_max = u32::try_from(p.frame_bytes).map_err(|_| Failure::InvalidProfile)?;
    types.push(ty(
        22,
        "CommandPayloadBytes",
        Shape::Bytes {
            min: 1,
            max: blob_max,
        },
    ));
    types.push(ty(23, "EpochDelta", unsigned(0, u128::from(u64::MAX))));
    types.push(ty(24, "SecondsDelta", unsigned(0, u128::from(u64::MAX))));
    types.push(record_type(
        701,
        "CommandEnvelope",
        &[5, 6, 15, 13, 6, 6, 4, 22, 5, 5, 6],
    ));
    types.push(record_type(
        702,
        "ClockRules",
        &[6, 23, 23, 23, 23, 23, 24, 24],
    ));
    types.push(ty(
        703,
        "CollectionAndFrameBounds",
        Shape::Tuple(vec![4, 4, 4, 4]),
    ));
    types.push(ty(
        704,
        "FeedSourceMap",
        Shape::Table {
            key: 5,
            value: 117,
            set: false,
            limits: p.bounds,
        },
    ));
    types.push(record_type(117, "SourceRow", &[5; 6]));
    types.push(ty(
        705,
        "FeedAssignmentMap",
        Shape::Table {
            key: 5,
            value: 5,
            set: false,
            limits: p.bounds,
        },
    ));
    types.push(record_type(
        706,
        "RegisterFeedCommand",
        &[6, 4, 5, 5, 6, 6, 19, 115, 13, 23, 702, 703, 704, 705],
    ));
    types.push(record_type(
        707,
        "RegisterKeyCommand",
        &[5, 5, 16, 7, 8, 8, 5],
    ));
    types.push(record_type(
        708,
        "RotateKeyCommand",
        &[5, 5, 16, 7, 8, 8, 8, 5],
    ));
    types.push(record_type(709, "RoundCommand", &[6, 18]));
    types.push(record_type(710, "BondCommand", &[5, 711]));
    types.push(record_type(711, "BondCommandBody", &[4, 22]));
    types.push(record_type(712, "OpenDisputeCommand", &[6, 6]));
    types.push(record_type(713, "FinalDisputeCommand", &[6, 116]));
    types.push(record_type(
        714,
        "ResolveDisputeCommand",
        &[6, VERDICT_OPTION],
    ));
    types.push(record_type(
        715,
        "StressCommand",
        &[6, 6, 6, PRICE_OPTION, 13],
    ));
    types.push(record_type(716, "PriceCommand", &[6, 4, 22]));
    types.push(record_type(717, "RewardCommand", &[6, 4, 22]));
    types.push(record_type(718, "AccrueRewardCommand", &[6, 6]));
    types.sort_by_key(|t| t.id);
    Ok(Descriptor {
        order: None,
        profile: b"OracleNativeFlatTablesV1".to_vec(),
        types,
        roots: [600, 601, 601],
        frame_bytes: [p.frame_bytes; 3],
        genesis: wire::record(Vec::new())?,
        reasons: REASONS.iter().map(|r| *r as u32).collect(),
        cases: Vec::new(),
        laws: Vec::new(),
        limits: zero_limits(),
    })
}

pub(super) fn fit(s: &LifecycleState) -> Result<Result<(), Reason>, Failure> {
    let v = state_value(s)?;
    let ValueRef::Record(fields) = v.view() else {
        return Err(Failure::Canonical);
    };
    // Growth refusal order is ItemTooLarge, Full, SnapshotTooLarge. No row or
    // already-incurred liability is dropped to make a new command fit.
    for f in &fields[4..] {
        if let ValueRef::Map(entries) = f.value().view()
            && entries.iter().any(|e| {
                e.key()
                    .encoded_length()
                    .ok()
                    .and_then(|n| {
                        e.value()
                            .encoded_length()
                            .ok()
                            .and_then(|v| n.checked_add(v))
                            .and_then(|v| v.checked_add(16))
                    })
                    .is_none_or(|n| n as u64 > s.profile.bounds.max_item_bytes)
            })
        {
            return Ok(Err(Reason::ItemTooLarge));
        }
    }
    for f in &fields[4..] {
        if let ValueRef::Map(entries) = f.value().view()
            && entries.len() > s.profile.bounds.max_entries as usize
        {
            return Ok(Err(Reason::Full));
        }
    }
    for f in &fields[4..] {
        if let ValueRef::Map(entries) = f.value().view() {
            let size = entries.iter().try_fold(29u64, |total, e| {
                let key = u64::try_from(e.encoded_key().len()).map_err(|_| Failure::Arithmetic)?;
                let value =
                    u64::try_from(e.value().encoded_length().map_err(|_| Failure::Canonical)?)
                        .map_err(|_| Failure::Arithmetic)?;
                total
                    .checked_add(16)
                    .and_then(|n| n.checked_add(key))
                    .and_then(|n| n.checked_add(value))
                    .ok_or(Failure::Arithmetic)
            })?;
            if size > s.profile.bounds.max_snapshot_bytes {
                return Ok(Err(Reason::SnapshotTooLarge));
            }
        }
    }
    if (v.encoded_length().map_err(|_| Failure::Canonical)? as u64)
        .checked_add(48)
        .ok_or(Failure::Arithmetic)?
        > s.profile.frame_bytes
    {
        return Err(Failure::Canonical);
    }
    Ok(Ok(()))
}

/// Return the canonical closed native oracle schema.
pub fn lifecycle_schema_bytes(state: &LifecycleState) -> Result<Vec<u8>, Failure> {
    schema(&state.profile)?
        .schema_bytes()
        .map_err(|_| Failure::Canonical)
}
/// Check that a snapshot is the exact canonical encoding of this state.
pub fn check_lifecycle_snapshot_bytes(
    state: &LifecycleState,
    bytes: &[u8],
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| {
        let d = schema(&state.profile)?;
        let value =
            super::super::tables::admit_native_snapshot(&d, bytes, m).map_err(|e| match e {
                super::super::tables::Failure::Budget(e) => Failure::Budget(e),
                _ => Failure::Canonical,
            })?;
        // Reconstruct the complete canonical Value under the closed schema,
        // then bind it to the retained native state. This does not import a new
        // retained state or qualify historical replay/publication (phase 6).
        if value != state_value(state)? {
            return Err(Failure::InvalidState);
        };
        Ok(())
    })
}

pub(super) fn command_value(
    state: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
) -> Result<Value, Failure> {
    let (kind, payload) = match c {
        LifecycleCommand::Reward { feed, command } => {
            let (k, p) = match command {
                NativeRewardCommand::Fund(n) => (1, number(*n)),
                NativeRewardCommand::Accrue { aggregate, report } => (
                    2,
                    wire::record(vec![wire::blob(aggregate)?, wire::blob(report)?])?,
                ),
                NativeRewardCommand::Claim(id) => (3, wire::blob(id)?),
                NativeRewardCommand::Clawback(id) => (4, wire::blob(id)?),
            };
            (
                10,
                wire::record(vec![
                    wire::blob(feed)?,
                    n(k),
                    wire::blob(&wire::bytes(&p)?)?,
                ])?,
            )
        }
        LifecycleCommand::Price { feed, command } => {
            let (kind, data) = match command {
                NativePriceCommand::Stage { aggregate } => (1, wire::blob(aggregate)?),
                NativePriceCommand::Commit => (2, Value::unit()),
                NativePriceCommand::SyncTime { round } => (3, n(*round)),
                NativePriceCommand::AdvanceEpoch => (4, Value::unit()),
            };
            (
                9,
                wire::record(vec![
                    wire::blob(feed)?,
                    n(kind),
                    wire::blob(&wire::bytes(&data)?)?,
                ])?,
            )
        }
        LifecycleCommand::RegisterFeed(f) => (1, feed_value(f, true)?),
        LifecycleCommand::RegisterKey { key, authorization } => (
            2,
            wire::record(vec![
                wire::blob(key.reporter.bytes())?,
                wire::blob(key.owner.bytes())?,
                n(key.version),
                wire::blob(&key.public_key)?,
                wire::blob(&key.possession)?,
                wire::blob(authorization)?,
                wire::blob(BLS_POP_SCHEME.as_bytes())?,
            ])?,
        ),
        LifecycleCommand::RotateKey {
            key,
            authorization,
            old_authorization,
        } => (
            3,
            wire::record(vec![
                wire::blob(key.reporter.bytes())?,
                wire::blob(key.owner.bytes())?,
                n(key.version),
                wire::blob(&key.public_key)?,
                wire::blob(&key.possession)?,
                wire::blob(authorization)?,
                wire::blob(old_authorization)?,
                wire::blob(BLS_POP_SCHEME.as_bytes())?,
            ])?,
        ),
        LifecycleCommand::OpenRound { feed, round } => {
            (4, wire::record(vec![wire::blob(feed)?, n(*round)])?)
        }
        LifecycleCommand::SubmitReport(r) => (5, raw_report_value(r)?),
        LifecycleCommand::BuildAggregate { feed, round } => {
            (6, wire::record(vec![wire::blob(feed)?, n(*round)])?)
        }
        LifecycleCommand::Bond { reporter, command } => (
            7,
            wire::record(vec![
                wire::blob(reporter.bytes())?,
                bond_command_value(command)?,
            ])?,
        ),
        LifecycleCommand::UpdateStress {
            feed,
            observation: o,
        } => (
            8,
            wire::record(vec![
                wire::blob(feed)?,
                wire::blob(&o.source)?,
                wire::blob(&o.valuation_root)?,
                Value::sum(
                    PRICE_OPTION,
                    u16::from(o.value.is_some()),
                    o.value.map(|v| number(v.0)),
                ),
                n(o.observed_epoch),
            ])?,
        ),
    };
    wire::record(vec![
        wire::blob(actor.bytes())?,
        wire::blob(&ctx.pre_root)?,
        n(ctx.nonce),
        n(ctx.clock.epoch),
        wire::blob(&ctx.clock.backend)?,
        wire::blob(&ctx.clock.finality_root)?,
        n(kind),
        wire::blob(&wire::bytes(&payload)?)?,
        wire::blob(b"zeno.oracle.native_command/1")?,
        wire::blob(state.chain.bytes())?,
        wire::blob(&state.profile_id)?,
    ])
}
fn verdict_value(v: &FinalVerdict) -> Result<Value, Failure> {
    wire::record(vec![
        wire::blob(&v.adjudicator)?,
        wire::blob(&v.dispute)?,
        wire::blob(&v.violation)?,
        Value::boolean(v.upheld),
        n(v.final_epoch),
        wire::blob(&v.finality_root)?,
    ])
}
fn bond_command_value(c: &BondCommand) -> Result<Value, Failure> {
    let (kind, p) = match c {
        BondCommand::Deposit(x) => (1, number(x.0)),
        BondCommand::Withdraw(x) => (2, number(x.0)),
        BondCommand::Unregister => (3, Value::unit()),
        BondCommand::Clear { liability } => (4, wire::blob(liability)?),
        BondCommand::OpenDispute { id, liability } => (
            5,
            wire::record(vec![wire::blob(id)?, wire::blob(liability)?])?,
        ),
        BondCommand::Slash { dispute, verdict } => (
            6,
            wire::record(vec![wire::blob(dispute)?, verdict_value(verdict)?])?,
        ),
        BondCommand::Resolve { dispute, verdict } => (
            7,
            wire::record(vec![
                wire::blob(dispute)?,
                match verdict {
                    Verdict::Inconclusive => maybe_verdict(None)?,
                    Verdict::Final(v) => maybe_verdict(Some(v))?,
                },
            ])?,
        ),
        BondCommand::Reserve { .. } | BondCommand::RecordPublication { .. } => {
            return Err(Failure::UnboundParameter);
        }
    };
    wire::record(vec![n(kind), wire::blob(&wire::bytes(&p)?)?])
}
