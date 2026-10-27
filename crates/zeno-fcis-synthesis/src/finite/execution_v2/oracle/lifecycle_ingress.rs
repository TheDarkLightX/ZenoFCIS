//! Closed command-byte ingress. Schemas are compiled library constants bound by
//! the native profile; applications cannot supply descriptors or evaluators.
use super::aggregation::FeedRules;
use super::bonds::{BondCommand, FinalVerdict, Verdict};
use super::lifecycle::{
    FeedRegistration, LifecycleCommand, LifecycleContext, LifecyclePlan, LifecycleState,
    NativePriceCommand, NativeRewardCommand, StressObservation,
};
use super::lifecycle_schema as schema;
use super::prices::ClockView;
use super::reports::{KeyRegistration, RawReport, ReportPayload};
use super::types::{
    BLS_POP_SCHEME, ClockParameters, Diversity, Hash, Id, PriceE8, Reason, Source, StressE8,
    ZdexE18,
};
use super::verification::BlsVerifier;
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured};
use alloc::{collections::BTreeMap, vec::Vec};
use zeno_fcis_value::{Value, ValueRef};

/// Encode the exact closed native command envelope.
pub fn lifecycle_command_bytes(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
) -> Result<Vec<u8>, Failure> {
    wire::bytes(&schema::command_value(s, actor, ctx, c)?)
}
/// Decode canonical command bytes and evaluate the same checked native relation.
pub fn lifecycle_relation_bytes(
    verifier: &impl BlsVerifier,
    s: &LifecycleState,
    bytes: &[u8],
    limits: Limits,
) -> Evaluation<Result<LifecyclePlan, Reason>> {
    measured(limits, |m| {
        let (actor, ctx, c) = decode(s, bytes, m)?;
        let result = super::lifecycle::run(verifier, s, &actor, ctx, &c, m)?;
        super::lifecycle_laws::check(
            verifier,
            s,
            &actor,
            ctx,
            &c,
            result.as_ref().map(|p| p.state()).map_err(|r| *r),
            m,
        )?;
        Ok(result)
    })
}
fn value(s: &LifecycleState, root: u32, bytes: &[u8], m: &mut Meter) -> Result<Value, Failure> {
    super::super::tables::admit_native_value(&schema::schema(&s.profile)?, root, bytes, m).map_err(
        |e| match e {
            super::super::tables::Failure::Budget(e) => Failure::Budget(e),
            _ => Failure::Canonical,
        },
    )
}
fn fields(v: &Value) -> Result<Vec<&Value>, Failure> {
    let ValueRef::Record(fs) = v.view() else {
        return Err(Failure::Canonical);
    };
    Ok(fs.iter().map(|f| f.value()).collect())
}
fn bytes(v: &Value) -> Result<&[u8], Failure> {
    if let ValueRef::Bytes(b) = v.view() {
        Ok(b)
    } else {
        Err(Failure::Canonical)
    }
}
fn number(v: &Value) -> Result<u128, Failure> {
    if let ValueRef::U128(n) = v.view() {
        Ok(n)
    } else {
        Err(Failure::Canonical)
    }
}
fn word(v: &Value) -> Result<u64, Failure> {
    u64::try_from(number(v)?).map_err(|_| Failure::Canonical)
}
fn hash(v: &Value) -> Result<Hash, Failure> {
    bytes(v)?.try_into().map_err(|_| Failure::Canonical)
}
fn id(v: &Value) -> Result<Id, Failure> {
    Id::new(bytes(v)?)
}
fn signature(v: &Value) -> Result<[u8; 96], Failure> {
    bytes(v)?.try_into().map_err(|_| Failure::Canonical)
}
fn tuple(v: &Value) -> Result<&[Value], Failure> {
    if let ValueRef::Tuple(xs) = v.view() {
        Ok(xs)
    } else {
        Err(Failure::Canonical)
    }
}
fn option(v: &Value, expected: u32) -> Result<Option<&Value>, Failure> {
    if let ValueRef::Sum {
        type_id,
        variant,
        payload,
    } = v.view()
        && type_id == expected
        && ((variant == 0 && payload.is_none()) || (variant == 1 && payload.is_some()))
    {
        return Ok(payload);
    }
    Err(Failure::Canonical)
}
fn boolean(v: &Value) -> Result<bool, Failure> {
    if let ValueRef::Bool(b) = v.view() {
        Ok(b)
    } else {
        Err(Failure::Canonical)
    }
}
fn scheme(v: &Value) -> Result<(), Failure> {
    if bytes(v)? == BLS_POP_SCHEME.as_bytes() {
        Ok(())
    } else {
        Err(Failure::UnsupportedScheme)
    }
}
type DecodedKey = (KeyRegistration, [u8; 96], Option<[u8; 96]>);
fn key(v: &Value) -> Result<DecodedKey, Failure> {
    let f = fields(v)?;
    scheme(f[f.len() - 1])?;
    Ok((
        KeyRegistration {
            reporter: id(f[0])?,
            owner: id(f[1])?,
            version: word(f[2])?,
            public_key: bytes(f[3])?.try_into().map_err(|_| Failure::Canonical)?,
            possession: signature(f[4])?,
        },
        signature(f[5])?,
        if f.len() == 8 {
            Some(signature(f[6])?)
        } else {
            None
        },
    ))
}
fn verdict(v: &Value) -> Result<FinalVerdict, Failure> {
    let f = fields(v)?;
    Ok(FinalVerdict {
        adjudicator: hash(f[0])?,
        dispute: hash(f[1])?,
        violation: hash(f[2])?,
        upheld: boolean(f[3])?,
        final_epoch: word(f[4])?,
        finality_root: hash(f[5])?,
    })
}
fn raw_report(v: &Value) -> Result<RawReport, Failure> {
    let f = fields(v)?;
    let p = fields(f[0])?;
    scheme(p[13])?;
    Ok(RawReport {
        payload: ReportPayload {
            chain: id(p[0])?,
            reporter: id(p[1])?,
            key_version: word(p[2])?,
            feed: hash(p[3])?,
            profile: hash(p[4])?,
            query: hash(p[5])?,
            source: id(p[6])?,
            round: word(p[7])?,
            value: PriceE8::new(number(p[8])?)?,
            observed_epoch: word(p[9])?,
            observation_seconds: word(p[10])?,
            sequence: word(p[11])?,
            previous: option(p[12], 90)?.map(hash).transpose()?,
        },
        payload_hash: hash(f[1])?,
        id: hash(f[2])?,
        signature: signature(f[3])?,
    })
}
fn feed(v: &Value) -> Result<FeedRegistration, Failure> {
    let f = fields(v)?;
    let d = tuple(f[7])?;
    let clock = fields(f[10])?;
    let b = tuple(f[11])?;
    let mut sources = Vec::new();
    let ValueRef::Map(rows) = f[12].view() else {
        return Err(Failure::Canonical);
    };
    for row in rows {
        let s = fields(row.value())?;
        let source = Source {
            id: id(s[0])?,
            operator: id(s[1])?,
            venue: id(s[2])?,
            family: id(s[3])?,
            transport: id(s[4])?,
            jurisdiction: id(s[5])?,
        };
        if id(row.key())? != source.id {
            return Err(Failure::Canonical);
        }
        sources.push(source);
    }
    let ValueRef::Map(rows) = f[13].view() else {
        return Err(Failure::Canonical);
    };
    let assignments = rows
        .iter()
        .map(|row| Ok((id(row.key())?, id(row.value())?)))
        .collect::<Result<BTreeMap<_, _>, Failure>>()?;
    let mut minimum = [0u8; 5];
    let mut max_same_class = [0u8; 5];
    for i in 0..5 {
        minimum[i] = u8::try_from(number(&d[i])?).map_err(|_| Failure::Canonical)?;
        max_same_class[i] = u8::try_from(number(&d[i + 5])?).map_err(|_| Failure::Canonical)?;
    }
    Ok(FeedRegistration {
        version: word(f[1])?,
        base: id(f[2])?,
        quote: id(f[3])?,
        rules: FeedRules {
            feed: hash(f[0])?,
            profile: hash(f[4])?,
            query: hash(f[5])?,
            max_deviation_bps: number(f[6])?,
            diversity: Diversity {
                minimum,
                max_same_class,
            },
            clock: ClockParameters {
                backend: hash(clock[0])?,
                heartbeat: word(clock[1])?,
                confirmation: word(clock[2])?,
                dispute_open: word(clock[3])?,
                inclusion: word(clock[4])?,
                max_age: word(clock[5])?,
                max_epoch_seconds: word(clock[6])?,
                max_time_lag: word(clock[7])?,
            },
            bounds: super::CollectionLimits {
                max_entries: u32::try_from(word(&b[0])?).map_err(|_| Failure::Canonical)?,
                max_item_bytes: word(&b[1])?,
                max_snapshot_bytes: word(&b[2])?,
            },
            frame_bytes: word(&b[3])?,
        },
        sources,
        assignments,
        first_open: word(f[8])?,
        cutoff_delta: word(f[9])?,
    })
}
fn bond(s: &LifecycleState, v: &Value, m: &mut Meter) -> Result<LifecycleCommand, Failure> {
    let f = fields(v)?;
    let reporter = id(f[0])?;
    let body = fields(f[1])?;
    let kind = word(body[0])?;
    let payload = bytes(body[1])?;
    let root = match kind {
        1 | 2 => 10,
        3 => 1,
        4 => 6,
        5 => 712,
        6 => 713,
        7 => 714,
        _ => return Err(Failure::Canonical),
    };
    let p = value(s, root, payload, m)?;
    let command = match kind {
        1 => BondCommand::Deposit(ZdexE18(number(&p)?)),
        2 => BondCommand::Withdraw(ZdexE18(number(&p)?)),
        3 => BondCommand::Unregister,
        4 => BondCommand::Clear {
            liability: hash(&p)?,
        },
        5 => {
            let p = fields(&p)?;
            BondCommand::OpenDispute {
                id: hash(p[0])?,
                liability: hash(p[1])?,
            }
        }
        6 => {
            let p = fields(&p)?;
            BondCommand::Slash {
                dispute: hash(p[0])?,
                verdict: verdict(p[1])?,
            }
        }
        7 => {
            let p = fields(&p)?;
            BondCommand::Resolve {
                dispute: hash(p[0])?,
                verdict: option(p[1], 96)?
                    .map(verdict)
                    .transpose()?
                    .map_or(Verdict::Inconclusive, Verdict::Final),
            }
        }
        _ => return Err(Failure::Canonical),
    };
    Ok(LifecycleCommand::Bond { reporter, command })
}
fn decode(
    s: &LifecycleState,
    input: &[u8],
    m: &mut Meter,
) -> Result<(Id, LifecycleContext, LifecycleCommand), Failure> {
    let envelope = value(s, 701, input, m)?;
    let e = fields(&envelope)?;
    if bytes(e[8])? != b"zeno.oracle.native_command/1"
        || id(e[9])? != s.chain
        || hash(e[10])? != s.profile_id
    {
        return Err(Failure::InvalidProfile);
    }
    let actor = id(e[0])?;
    let ctx = LifecycleContext {
        pre_root: hash(e[1])?,
        nonce: word(e[2])?,
        clock: ClockView {
            epoch: word(e[3])?,
            backend: hash(e[4])?,
            finality_root: hash(e[5])?,
        },
    };
    let kind = word(e[6])?;
    let root = match kind {
        1 => 706,
        2 => 707,
        3 => 708,
        4 | 6 => 709,
        5 => 111,
        7 => 710,
        8 => 715,
        9 => 716,
        10 => 717,
        _ => return Err(Failure::Canonical),
    };
    let payload = value(s, root, bytes(e[7])?, m)?;
    let f = fields(&payload)?;
    let c = match kind {
        10 => {
            let k = word(f[1])?;
            let root = match k {
                1 => 25,
                2 => 718,
                3 | 4 => 6,
                _ => return Err(Failure::Canonical),
            };
            let p = value(s, root, bytes(f[2])?, m)?;
            let command = match k {
                1 => NativeRewardCommand::Fund(number(&p)?),
                2 => {
                    let p = fields(&p)?;
                    NativeRewardCommand::Accrue {
                        aggregate: hash(p[0])?,
                        report: hash(p[1])?,
                    }
                }
                3 => NativeRewardCommand::Claim(hash(&p)?),
                4 => NativeRewardCommand::Clawback(hash(&p)?),
                _ => return Err(Failure::Canonical),
            };
            LifecycleCommand::Reward {
                feed: hash(f[0])?,
                command,
            }
        }
        1 => LifecycleCommand::RegisterFeed(feed(&payload)?),
        2 => {
            let (key, authorization, _) = key(&payload)?;
            LifecycleCommand::RegisterKey { key, authorization }
        }
        3 => {
            let (key, authorization, old) = key(&payload)?;
            LifecycleCommand::RotateKey {
                key,
                authorization,
                old_authorization: old.ok_or(Failure::Canonical)?,
            }
        }
        4 => LifecycleCommand::OpenRound {
            feed: hash(f[0])?,
            round: word(f[1])?,
        },
        5 => LifecycleCommand::SubmitReport(raw_report(&payload)?),
        6 => LifecycleCommand::BuildAggregate {
            feed: hash(f[0])?,
            round: word(f[1])?,
        },
        7 => bond(s, &payload, m)?,
        8 => LifecycleCommand::UpdateStress {
            feed: hash(f[0])?,
            observation: StressObservation {
                source: hash(f[1])?,
                valuation_root: hash(f[2])?,
                value: option(f[3], 93)?
                    .map(|v| number(v).map(StressE8))
                    .transpose()?,
                observed_epoch: word(f[4])?,
            },
        },
        9 => {
            let k = word(f[1])?;
            let root = match k {
                1 => 6,
                2 | 4 => 1,
                3 => 18,
                _ => return Err(Failure::Canonical),
            };
            let v = value(s, root, bytes(f[2])?, m)?;
            LifecycleCommand::Price {
                feed: hash(f[0])?,
                command: match k {
                    1 => NativePriceCommand::Stage {
                        aggregate: hash(&v)?,
                    },
                    2 => NativePriceCommand::Commit,
                    3 => NativePriceCommand::SyncTime { round: word(&v)? },
                    4 => NativePriceCommand::AdvanceEpoch,
                    _ => return Err(Failure::Canonical),
                },
            }
        }
        _ => return Err(Failure::Canonical),
    };
    if lifecycle_command_bytes(s, &actor, ctx, &c)? != input {
        return Err(Failure::Canonical);
    }
    Ok((actor, ctx, c))
}
