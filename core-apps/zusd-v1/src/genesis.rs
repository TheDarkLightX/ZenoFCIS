//! Conditional mathematical genesis. Existing balance facts are explicit inputs;
//! no asset is created and no funding/authentication/publication receipt is issued.
use crate::{Evaluation, Failure, Id, Limits, Q, Work, ledger, parameters, run, stability, wire};
use alloc::{vec, vec::Vec};
use zeno_fcis_value::{Field, MapEntry, Value, ValueRef};
/// Roles: Active, Default, SP, Gas, Surplus, Staking, Issuance, Rail.
#[derive(Clone, Copy, Debug)]
pub struct FundingAssumption {
    pub pool_accounts: [Id; 8],
    pub amount: u128,
}
pub struct Input<'a> {
    pub balances: &'a ledger::Sheet,
    /// zUSD/AGRS/zDEX identifiers, in nominal asset order.
    pub assets: [Id; 3],
    /// Header roots 2..10, carried as explicit uninterpreted reference facts.
    pub roots: [Id; 9],
    pub deployment_epoch: u64,
    pub last_trigger_epoch: u64,
    pub parameter_version: u64,
    pub parameters: &'a Value,
    pub funding: Option<FundingAssumption>,
}
#[derive(Debug)]
pub struct Reference {
    state: Value,
}
impl Reference {
    pub fn state(&self) -> &Value {
        &self.state
    }
}
fn record(fields: Vec<(u16, Value)>) -> Result<Value, Failure> {
    Value::record_canonical(fields.into_iter().map(|(i, v)| Field::new(i, v)).collect())
        .map_err(|_| Failure::Canonical)
}
fn map(entries: Vec<(Value, Value)>, w: &mut Work) -> Result<Value, Failure> {
    w.charge(
        entries.len() as u64,
        entries.len() as u64,
        entries.len() as u64 * 256,
    )?;
    let mut result = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        result.push(MapEntry::try_new(key, value).map_err(|_| Failure::Canonical)?);
    }
    // Bounded insertion sort pays every encoded-key comparison before work.
    for i in 1..result.len() {
        let mut j = i;
        while j > 0 {
            w.charge(1, 1, 128)?;
            if result[j - 1].encoded_key() <= result[j].encoded_key() {
                break;
            }
            result.swap(j - 1, j);
            j -= 1;
        }
    }
    Value::map_canonical(result).map_err(|_| Failure::Canonical)
}
fn bytes(id: Id) -> Result<Value, Failure> {
    Value::bytes(id.to_vec()).map_err(|_| Failure::Canonical)
}
pub fn reference(policy: &wire::Policy, input: Input<'_>, limits: Limits) -> Evaluation<Reference> {
    run(limits, |w| {
        let parameter_bytes = wire::size(input.parameters, w, 0)?;
        w.charge(
            1,
            0,
            parameter_bytes.checked_mul(4).ok_or(Failure::Arithmetic)?,
        )?;
        let funded = input.funding.ok_or(Failure::UnboundProfile)?;
        if funded.amount != stability::FUNDED_CAP {
            return Err(Failure::UnboundProfile);
        }
        for i in 0..8 {
            for j in i + 1..8 {
                w.charge(1, 1, 0)?;
                if funded.pool_accounts[i] == funded.pool_accounts[j] {
                    return Err(Failure::Domain);
                }
            }
        }
        if input.assets[0] == input.assets[1]
            || input.assets[0] == input.assets[2]
            || input.assets[1] == input.assets[2]
        {
            return Err(Failure::Domain);
        }
        let _checked_balances = w.invoke(|limits| {
            ledger::observe_sheet(
                input.balances,
                u32::try_from(input.balances.balances.len()).unwrap_or(u32::MAX),
                limits,
            )
        })?;
        if input.balances.supply != 0
            || input.balances.active_debt != 0
            || input.balances.default_debt != 0
        {
            return Err(Failure::Conservation);
        }
        let mut allocated = 0;
        for row in &input.balances.balances {
            w.charge(8, 8, 0)?;
            if let Some(role) = funded.pool_accounts.iter().position(|a| *a == row.account) {
                if role == 6 && row.quantity.asset() == ledger::Asset::Zdex {
                    allocated = row.quantity.atoms();
                } else if row.quantity.atoms() != 0 {
                    return Err(Failure::Conservation);
                }
            }
        }
        if allocated != funded.amount {
            return Err(Failure::Conservation);
        }
        let quantity_type = policy
            .schema()
            .types()
            .iter()
            .find(|t| t.name().as_str() == "Quantity_Z_A_T_")
            .ok_or(Failure::Domain)?
            .id()
            .get();
        w.charge(
            input.balances.balances.len() as u64,
            0,
            input.balances.balances.len() as u64 * 256,
        )?;
        let mut balances = Vec::new();
        for row in &input.balances.balances {
            let asset = row.quantity.asset() as usize;
            let key = Value::tuple(vec![bytes(row.account)?, bytes(input.assets[asset])?])
                .map_err(|_| Failure::Canonical)?;
            balances.push((
                key,
                Value::sum(
                    quantity_type,
                    asset as u16,
                    Some(Value::unsigned(row.quantity.atoms())),
                ),
            ));
        }
        let mut header = Vec::new();
        for id in 1u16..=42 {
            w.charge(1, 1, 64)?;
            let value = match id {
                1 => bytes(*policy.identity().as_bytes())?,
                2..=10 => bytes(input.roots[id as usize - 2])?,
                11 | 12 => Value::unsigned(input.deployment_epoch as u128),
                14 => Value::unsigned(input.parameter_version as u128),
                27 => Value::unsigned(Q),
                36 => Value::unsigned(funded.amount),
                41 => Value::unsigned(input.last_trigger_epoch as u128),
                42 => Value::bytes(Vec::new()).map_err(|_| Failure::Canonical)?,
                _ => Value::unsigned(0),
            };
            header.push((id, value));
        }
        let header = record(header)?;
        let history = map(
            vec![(
                Value::tuple(vec![Value::unsigned(0), Value::unsigned(0)])
                    .map_err(|_| Failure::Canonical)?,
                record(vec![(1, Value::unsigned(0)), (2, Value::unsigned(0))])?,
            )],
            w,
        )?;
        let epochs = map(
            vec![(
                Value::unsigned(0),
                record(vec![(1, Value::unsigned(0)), (2, Value::boolean(false))])?,
            )],
            w,
        )?;
        let mut state = Vec::new();
        for id in 1u16..=15 {
            let value = match id {
                1 => header.clone(),
                4 => Value::vector(Vec::new()).map_err(|_| Failure::Canonical)?,
                5 => map(balances.clone(), w)?,
                8 => history.clone(),
                9 => epochs.clone(),
                15 => map(
                    vec![(
                        Value::unsigned(input.parameter_version as u128),
                        input.parameters.clone(),
                    )],
                    w,
                )?,
                _ => Value::map_canonical(Vec::new()).map_err(|_| Failure::Canonical)?,
            };
            state.push((id, value));
        }
        let state = record(state)?;
        // Complete schema, collection bytes and all selected numeric Params are
        // checked before returning even this non-authoritative reference object.
        w.invoke(|limits| parameters::bind(policy, &state, limits))?;
        let framed = w.invoke(|limits| policy.frame(wire::STATE, &state, limits))?;
        let admitted = w.invoke(|limits| policy.admit(wire::STATE, &framed, limits))?;
        if !matches!(admitted.view(), ValueRef::Record(_)) {
            return Err(Failure::Domain);
        }
        Ok(Reference { state: admitted })
    })
}
