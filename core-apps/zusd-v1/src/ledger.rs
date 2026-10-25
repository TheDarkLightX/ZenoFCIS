//! Canonical quantity observations and exact per-asset candidate conservation.
//! These checks do not authenticate a mint, funding event, caller or publication.
use crate::{Evaluation, Failure, Id, Limits, Work, run, wide::Wide};
use alloc::{vec, vec::Vec};
use zeno_fcis_value::{MapEntry, Value};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Asset {
    Zusd,
    Agrs,
    Zdex,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Quantity {
    Zusd(u128),
    Agrs(u128),
    Zdex(u128),
}
impl Quantity {
    pub fn asset(self) -> Asset {
        match self {
            Self::Zusd(_) => Asset::Zusd,
            Self::Agrs(_) => Asset::Agrs,
            Self::Zdex(_) => Asset::Zdex,
        }
    }
    pub fn atoms(self) -> u128 {
        match self {
            Self::Zusd(x) | Self::Agrs(x) | Self::Zdex(x) => x,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Balance {
    pub account: Id,
    pub quantity: Quantity,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sheet {
    pub balances: Vec<Balance>,
    pub supply: u128,
    pub active_debt: u128,
    pub default_debt: u128,
}
#[derive(Debug)]
#[must_use]
pub struct Observed {
    value: Value,
    totals: [Wide; 3],
}
impl Observed {
    pub fn value(&self) -> &Value {
        &self.value
    }
}
pub(crate) fn observe(sheet: &Sheet, capacity: u32, w: &mut Work) -> Result<Observed, Failure> {
    w.charge(1, 0, 0)?;
    if sheet.balances.len() > capacity as usize {
        return Err(Failure::Capacity);
    }
    // Fixed-width encoded operands, scratch reservation and the complete scan pay before work.
    let n = u64::try_from(sheet.balances.len()).map_err(|_| Failure::Capacity)?;
    w.charge(n, n, n.checked_mul(128).ok_or(Failure::Arithmetic)?)?;
    let mut previous = None;
    let mut entries = Vec::with_capacity(sheet.balances.len());
    let mut totals = [Wide::from(0); 3];
    for row in &sheet.balances {
        let key = (row.account, row.quantity.asset());
        if previous.is_some_and(|old| old >= key) {
            return Err(Failure::Canonical);
        }
        previous = Some(key);
        let asset = row.quantity.asset();
        totals[asset as usize] = totals[asset as usize].add(Wide::from(row.quantity.atoms()), w)?;
        let account = Value::bytes(row.account.to_vec()).map_err(|_| Failure::Canonical)?;
        let key = Value::tuple(vec![account, Value::unsigned(asset as u128)])
            .map_err(|_| Failure::Canonical)?;
        entries.push(
            MapEntry::try_new(key, Value::unsigned(row.quantity.atoms()))
                .map_err(|_| Failure::Canonical)?,
        );
    }
    // C<=u32::MAX makes every nonnegative sum <2^160. No persisted U256 value.
    // Preserve the prior six-limb debt-add charge and its check order.
    w.charge(6, 0, 0)?;
    if totals[0] != Wide::from(sheet.supply)
        || !crate::accounting::debt_matches(sheet.supply, sheet.active_debt, sheet.default_debt)
    {
        return Err(Failure::Conservation);
    }
    let value = Value::map_canonical(entries).map_err(|_| Failure::Canonical)?;
    Ok(Observed { value, totals })
}
pub fn observe_sheet(sheet: &Sheet, capacity: u32, limits: Limits) -> Evaluation<Observed> {
    run(limits, |w| observe(sheet, capacity, w))
}
/// Candidate-only closed composition: AGRS/zDEX conserve; zUSD changes only with debt.
/// Account-level economic outcomes remain obligations of the invoking relation.
pub fn check_transition(
    pre: &Sheet,
    post: &Sheet,
    capacity: u32,
    limits: Limits,
) -> Evaluation<Value> {
    run(limits, |w| {
        let a = observe(pre, capacity, w)?;
        let b = observe(post, capacity, w)?;
        if a.totals[1..] != b.totals[1..] {
            return Err(Failure::Conservation);
        }
        Ok(b.value)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Movement {
    pub from: Option<Id>,
    pub to: Option<Id>,
    pub quantity: Quantity,
}
#[derive(Debug)]
#[must_use]
pub struct Plan {
    post: Sheet,
    movements: Vec<Movement>,
    canonical: Value,
}
impl Plan {
    pub fn post(&self) -> &Sheet {
        &self.post
    }
    pub fn movements(&self) -> &[Movement] {
        &self.movements
    }
    pub fn canonical(&self) -> &Value {
        &self.canonical
    }
}
fn change(
    sheet: &mut Sheet,
    id: Id,
    q: Quantity,
    credit: bool,
    capacity: u32,
    w: &mut Work,
) -> Result<(), Failure> {
    w.charge(sheet.balances.len() as u64, sheet.balances.len() as u64, 0)?;
    let key = (id, q.asset());
    let found = sheet
        .balances
        .binary_search_by_key(&key, |v| (v.account, v.quantity.asset()));
    let old = found
        .ok()
        .map(|i| sheet.balances[i].quantity.atoms())
        .unwrap_or(0);
    if !credit && old < q.atoms() {
        return Err(Failure::Reject(1033));
    }
    w.charge(1, 0, 0)?;
    let value =
        crate::accounting::balance_change(old, q.atoms(), credit).ok_or(Failure::Arithmetic)?;
    let quantity = match q {
        Quantity::Zusd(_) => Quantity::Zusd(value),
        Quantity::Agrs(_) => Quantity::Agrs(value),
        Quantity::Zdex(_) => Quantity::Zdex(value),
    };
    match found {
        Ok(i) => sheet.balances[i].quantity = quantity,
        Err(i) if value > 0 => {
            if sheet.balances.len() >= capacity as usize {
                return Err(Failure::Reject(1061));
            }
            sheet.balances.insert(
                i,
                Balance {
                    account: id,
                    quantity,
                },
            );
        }
        Err(_) => (),
    }
    Ok(())
}
/// Pure closed ledger composition. Movements are domain inputs, never mint
/// credentials. Step 4 must bind the invoking monetary command and authentic state.
pub fn compose(
    pre: &Sheet,
    movements: &[Movement],
    post_debt: (u128, u128),
    capacity: u32,
    limits: Limits,
) -> Evaluation<Plan> {
    run(limits, |w| {
        let _pre_observation = observe(pre, capacity, w)?;
        if movements.len() > capacity as usize {
            return Err(Failure::Capacity);
        }
        w.charge(
            1,
            0,
            (pre.balances.len() as u64 * 128) + (movements.len() as u64 * 96),
        )?;
        let mut post = pre.clone();
        for leg in movements {
            w.charge(1, 1, 96)?;
            if leg.from.is_none() && leg.to.is_none() {
                return Err(Failure::Domain);
            }
            if !crate::accounting::movement_allowed(
                leg.from.is_some(),
                leg.to.is_some(),
                leg.quantity.asset() == Asset::Zusd,
            ) {
                return Err(Failure::Conservation);
            }
            if let Some(id) = leg.from {
                change(&mut post, id, leg.quantity, false, capacity, w)?;
            } else {
                post.supply = crate::add(post.supply, leg.quantity.atoms(), w)?;
            }
            if let Some(id) = leg.to {
                change(&mut post, id, leg.quantity, true, capacity, w)?;
            } else {
                post.supply = crate::sub(post.supply, leg.quantity.atoms(), w)?;
            }
        }
        post.active_debt = post_debt.0;
        post.default_debt = post_debt.1;
        let checked = observe(&post, capacity, w)?;
        Ok(Plan {
            post,
            movements: movements.to_vec(),
            canonical: checked.value,
        })
    })
}
