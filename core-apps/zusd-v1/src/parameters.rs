//! Select the actual Params row and refuse unsupported numeric configurations.
//! Profile identifiers remain uninterpreted; this does not configure a clock,
//! drip, oracle, exposure policy, change authority or migration.
use crate::{Evaluation, Failure, Id, Limits, Q, fees, run, stability, vaults, wire};
use zeno_fcis_value::{Value, ValueRef};
#[derive(Debug)]
pub struct Bound {
    record: Value,
    native: vaults::Parameters,
    profiles: [Option<Id>; 5],
}
impl Bound {
    pub fn record(&self) -> &Value {
        &self.record
    }
    pub fn native(&self) -> vaults::Parameters {
        self.native
    }
    /// clock, drip, oracle, exposure, change authority, respectively.
    pub fn profiles(&self) -> &[Option<Id>; 5] {
        &self.profiles
    }
}
fn field(v: &Value, id: u16) -> Result<&Value, Failure> {
    match v.view() {
        ValueRef::Record(fs) => fs
            .iter()
            .find(|f| f.id() == id)
            .map(|f| f.value())
            .ok_or(Failure::Domain),
        _ => Err(Failure::Domain),
    }
}
fn number(v: &Value) -> Result<u128, Failure> {
    match v.view() {
        ValueRef::U128(n) => Ok(n),
        _ => Err(Failure::Domain),
    }
}
pub fn bind(policy: &wire::Policy, state: &Value, limits: Limits) -> Evaluation<Bound> {
    run(limits, |w| {
        policy.validate(wire::STATE, state, w)?;
        policy.original_bounds(state, w)?;
        let version = number(field(field(state, 1)?, 14)?)?;
        let rows = match field(state, 15)?.view() {
            ValueRef::Map(es) => es,
            _ => return Err(Failure::Domain),
        };
        w.charge(rows.len() as u64, rows.len() as u64, 0)?;
        let record = rows
            .iter()
            .find(|e| e.key() == &Value::unsigned(version))
            .ok_or(Failure::Domain)?
            .value();
        let fixed = [
            500 * Q,
            50 * Q,
            vaults::MCR,
            vaults::CCR,
            200,
            fees::FLOOR,
            fees::FLOOR,
            Q / 20,
            2,
            fees::DECAY,
            525600000,
            20160,
            crate::SCALE,
            stability::FUNDED_CAP,
        ];
        for (i, expected) in fixed.into_iter().enumerate() {
            w.charge(1, 1, 0)?;
            if number(field(record, i as u16 + 1)?)? != expected {
                return Err(Failure::UnboundProfile);
            }
        }
        let mut profiles = [None; 5];
        for (i, p) in profiles.iter_mut().enumerate() {
            w.charge(1, 1, 0)?;
            *p = match field(record, 15 + i as u16)?.view() {
                ValueRef::Sum {
                    variant: 0,
                    payload: None,
                    ..
                } => None,
                ValueRef::Sum {
                    variant: 1,
                    payload: Some(v),
                    ..
                } => match v.view() {
                    ValueRef::Bytes(b) => Some(b.try_into().map_err(|_| Failure::Domain)?),
                    _ => return Err(Failure::Domain),
                },
                _ => return Err(Failure::Domain),
            };
        }
        if !matches!(field(record,20)?.view(),ValueRef::Bytes(b) if b.is_empty()) {
            return Err(Failure::UnboundProfile);
        }
        let n = wire::size(record, w, 0)?;
        w.charge(1, 0, n)?;
        Ok(Bound {
            record: record.clone(),
            native: vaults::Parameters {
                version: u64::try_from(version).map_err(|_| Failure::Domain)?,
                minimum_net: fixed[0],
                reserve: fixed[1],
                clock_profile: profiles[0],
                drip_profile: profiles[1],
            },
            profiles,
        })
    })
}
