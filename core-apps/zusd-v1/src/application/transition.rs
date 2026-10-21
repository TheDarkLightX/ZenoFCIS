//! One conditional authenticated decision. All mutation is exclusive invocation
//! scratch, dropped on failure. No supplied economic plan or Usage is trusted.
use super::encoding::{self, W};
use super::receipts::Message;
use super::*;
use crate::{
    Accumulator256, Evaluation, Failure, Limits, Q, Work, add, common, composition, redemption,
    run, sub, wide::Wide,
};
use ledger::{Asset, Movement, Quantity};
const ACTIVE: usize = 0;
const DEFAULT: usize = 1;
const GAS: usize = 2;
const SP_Z: usize = 3;
const SP_A: usize = 4;
const ISSUANCE: usize = 5;
const STAKE_T: usize = 6;
const FEES: usize = 7;
const SURPLUS: usize = 8;
fn price_required(c: &Command) -> bool {
    matches!(
        c,
        Command::Open { .. }
            | Command::Close
            | Command::Liquidate(_)
            | Command::Redeem(_)
            | Command::Pool(stability::Command::Withdraw(1..))
    ) || matches!(
        c,
        Command::Adjust {
            collateral: vaults::Delta::Remove(_),
            ..
        } | Command::Adjust {
            debt: vaults::Delta::Add(_),
            ..
        }
    )
}
fn trigger_command(c: &Command) -> bool {
    matches!(
        c,
        Command::Pool(
            stability::Command::Deposit { .. }
                | stability::Command::Withdraw(_)
                | stability::Command::Claim
        ) | Command::MoveGain { .. }
            | Command::Liquidate(_)
    )
}
fn authority(p: &Policy, d: Domain) -> Result<Id, Failure> {
    let c = &p.config;
    Ok(match d {
        Domain::Actor => c.actor_registry,
        Domain::Clock => c.clock_backend,
        Domain::Oracle => c.oracle_verifier,
        Domain::Custody => c.custody_verifier,
        Domain::Allocation => c.allocation_verifier,
        Domain::Drip => c.drip_verifier,
        Domain::Funding => c.funding_verifier,
        Domain::ParameterPolicy => c.change.ok_or(Failure::UnboundProfile)?.authority,
        Domain::ExposureRelease => c.release_verifier,
    })
}
fn message(p: &Policy, pre: Id, d: Domain, body: &[u8]) -> Result<Message, Failure> {
    let authority = authority(p, d)?;
    if authority == [0; 32] {
        return Err(Failure::UnboundProfile);
    }
    let mut w = W::new(b"zusd.ReceiptVerifierV1/1\0");
    w.u8(d as u8);
    w.id(authority);
    w.id(p.config.chain);
    w.id(p.id);
    w.id(pre);
    w.blob(body);
    Ok(Message {
        domain: d,
        authority,
        bytes: w.0,
    })
}
fn kinds(r: &Request) -> Vec<Domain> {
    let mut ds = alloc::vec![Domain::Actor, Domain::Clock];
    if price_required(&r.command) || matches!(r.command, Command::ObserveOracle) {
        ds.push(Domain::Oracle)
    }
    ds.push(Domain::Custody);
    if price_required(&r.command) {
        ds.push(Domain::Allocation)
    }
    if r.drip.is_some() {
        ds.push(Domain::Drip)
    }
    if matches!(r.command, Command::Fund { .. }) {
        ds.push(Domain::Funding)
    }
    if matches!(r.command, Command::ChangeParameters { .. }) {
        ds.push(Domain::ParameterPolicy)
    }
    if matches!(r.command, Command::ReleaseExposure { .. }) {
        ds.push(Domain::ExposureRelease)
    }
    ds
}
fn bounded_request(p: &Policy, r: &Request) -> Result<(), Failure> {
    let b = p.config.bounds;
    if r.proofs.len() > 9
        || r.proofs
            .iter()
            .any(|v| v.bytes.len() > b.proof_bytes as usize)
        || r.drip
            .as_ref()
            .is_some_and(|d| d.next.len() > b.drip_bytes as usize)
        || r.oracle
            .as_ref()
            .is_some_and(|o| o.view.aggregate_bytes.len() as u64 > b.frame_bytes)
    {
        return Err(Failure::Capacity);
    }
    // Reject unbounded raw vectors before allocating their canonical copies.
    // The remaining fixed request overhead is <2 KiB. A valid sealed prestate
    // already fits frame_bytes; preparation reserves twelve complete frames.
    let ids = match &r.command {
        Command::Liquidate(liquidation::Command::Batch(ids)) => ids.len(),
        Command::ReleaseExposure { events } => events.len(),
        _ => 0,
    };
    let variable = (ids as u64)
        .checked_mul(32)
        .and_then(|n| {
            n.checked_add(
                r.oracle
                    .as_ref()
                    .map_or(0, |o| o.view.aggregate_bytes.len() as u64),
            )
        })
        .and_then(|n| n.checked_add(r.drip.as_ref().map_or(0, |d| d.next.len() as u64)))
        .ok_or(Failure::Capacity)?;
    let proof_bytes = r
        .proofs
        .iter()
        .try_fold(0u64, |n, p| n.checked_add(p.bytes.len() as u64))
        .ok_or(Failure::Capacity)?;
    if variable > b.frame_bytes || proof_bytes > b.frame_bytes {
        return Err(Failure::Capacity);
    }
    match &r.command {
        Command::Liquidate(liquidation::Command::Batch(ids)) if ids.len() > b.vaults as usize => {
            return Err(Failure::Capacity);
        }
        Command::ReleaseExposure { events } if events.len() > b.events as usize => {
            return Err(Failure::Capacity);
        }
        _ => (),
    };
    if r.drip.is_some() && !trigger_command(&r.command) {
        return Err(Failure::Canonical);
    }
    if r.oracle.is_some()
        != (price_required(&r.command) || matches!(r.command, Command::ObserveOracle))
        || r.allocation.is_some() != price_required(&r.command)
    {
        return Err(Failure::Canonical);
    }
    Ok(())
}
/// Preparation/serialization only: messages confer no decision, measured work
/// or publication authority, even when a signer later returns proofs for them.
pub fn receipt_messages(p: &Policy, s: &State, r: &Request) -> Result<Vec<Message>, Failure> {
    bounded_request(p, r)?;
    let body = encoding::payload(r);
    if body.len() as u64 > p.config.bounds.frame_bytes {
        return Err(Failure::Capacity);
    }
    kinds(r)
        .into_iter()
        .map(|d| message(p, s.root(), d, &body))
        .collect()
}
pub fn genesis_messages(p: &Policy, g: &Genesis) -> Result<Vec<Message>, Failure> {
    if g.wallets.balances.len() > p.config.bounds.balances as usize
        || g.wallets.balances.len() as u64 > p.config.bounds.frame_bytes / 49
        || g.proofs.len() > 4
        || g.proofs
            .iter()
            .any(|v| v.bytes.len() > p.config.bounds.proof_bytes as usize)
    {
        return Err(Failure::Capacity);
    }
    let body = encoding::genesis(g);
    if body.len() as u64 > p.config.bounds.frame_bytes {
        return Err(Failure::Capacity);
    }
    [
        Domain::Actor,
        Domain::Clock,
        Domain::Custody,
        Domain::Funding,
    ]
    .into_iter()
    .map(|d| message(p, [0; 32], d, &body))
    .collect()
}
fn verify(
    p: &Policy,
    messages: &[Message],
    proofs: &[Proof],
    v: &impl ReceiptVerifierV1,
    w: &mut Work,
) -> Result<(), Failure> {
    if messages.len() != proofs.len() {
        return Err(Failure::Canonical);
    }
    for (m, proof) in messages.iter().zip(proofs) {
        if m.domain != proof.domain || proof.bytes.len() > p.config.bounds.proof_bytes as usize {
            return Err(Failure::Canonical);
        }
        w.charge(
            p.config.bounds.verification_steps,
            m.bytes.len() as u64,
            (m.bytes.len() + proof.bytes.len()) as u64,
        )?;
        v.verify(m.domain, m.authority, &m.bytes, &proof.bytes)
            .map_err(|e| match e {
                VerificationFailure::Invalid => {
                    if m.domain == Domain::Actor {
                        Failure::Reject(1000)
                    } else {
                        Failure::Canonical
                    }
                }
                VerificationFailure::Unavailable | VerificationFailure::Unsupported => {
                    Failure::UnboundProfile
                }
            })?;
    }
    Ok(())
}
fn valid_clock(p: &Policy, c: Clock) -> Result<(), Failure> {
    if c.backend != p.config.clock_backend || c.finality == [0; 32] || c.sync == [0; 32] {
        return Err(Failure::Canonical);
    }
    Ok(())
}
fn wallet(s: &State, id: Id, a: Asset) -> u128 {
    s.ledger
        .balances
        .binary_search_by_key(&(id, a), |r| (r.account, r.quantity.asset()))
        .ok()
        .map(|i| s.ledger.balances[i].quantity.atoms())
        .unwrap_or(0)
}
fn totals(s: &ledger::Sheet, w: &mut Work) -> Result<(u128, u128), Failure> {
    let mut a = Wide::from(0);
    let mut t = Wide::from(0);
    for r in &s.balances {
        w.charge(1, 1, 0)?;
        match r.quantity {
            Quantity::Agrs(n) => a = a.add(Wide::from(n), w)?,
            Quantity::Zdex(n) => t = t.add(Wide::from(n), w)?,
            _ => (),
        }
    }
    Ok((a.narrow()?, t.narrow()?))
}
fn check_backing(sheet: &ledger::Sheet, b: Backing, w: &mut Work) -> Result<(), Failure> {
    let (a, t) = totals(sheet, w)?;
    if [b.custody, b.funding, b.supply_root].contains(&[0; 32])
        || a != b.agrs
        || t != b.zdex
        || b.zdex > b.zdex_supply
        || b.zdex_supply.checked_add(b.zdex_burned).is_none()
    {
        return Err(Failure::Conservation);
    }
    Ok(())
}
fn nonzero(id: Id) -> Result<(), Failure> {
    if id == [0; 32] {
        Err(Failure::Canonical)
    } else {
        Ok(())
    }
}
fn seen(s: &mut State, id: Id, p: &Policy, w: &mut Work) -> Result<(), Failure> {
    nonzero(id)?;
    w.charge(s.seen.len() as u64, s.seen.len() as u64, 32)?;
    match s.seen.binary_search(&id) {
        Ok(_) => Err(Failure::Reject(1001)),
        Err(i) => {
            if s.seen.len() >= p.config.bounds.seen as usize {
                return Err(Failure::Reject(1061));
            }
            s.seen.insert(i, id);
            Ok(())
        }
    }
}
fn shape(p: &Policy, s: &State) -> Result<(), Failure> {
    let b = p.config.bounds;
    if s.policy != p.id
        || s.nonces.len() > b.nonces as usize
        || s.seen.len() > b.seen as usize
        || s.events.len() > b.events as usize
        || s.params.is_empty()
        || s.params.len() > b.params as usize
        || s.drip_state.len() > b.drip_bytes as usize
        || s.nonces.windows(2).any(|a| a[0].0 >= a[1].0)
        || s.seen.windows(2).any(|a| a[0] >= a[1])
        || s.params.windows(2).any(|a| a[0].version >= a[1].version)
        || s.events.windows(2).any(|a| a[0].id >= a[1].id)
    {
        return Err(Failure::Canonical);
    }
    Ok(())
}
fn validate(p: &Policy, s: &State, w: &mut Work) -> Result<(), Failure> {
    let b = p.config.bounds;
    w.charge(b.frame_bytes, 1, b.frame_bytes)?;
    shape(p, s)?;
    if encoding::state(s).len() as u64 > b.frame_bytes {
        return Err(Failure::Capacity);
    }
    valid_clock(p, s.clock)?;
    liquidation::validate(&s.book, b.vaults, p.sp_bounds(), w)?;
    w.invoke(|l| stability::liabilities(&s.pool, p.sp_bounds(), l))?;
    w.invoke(|l| staking::fee_event(&s.staking, 0, 0, b.stakers, l))?;
    let _observed = w.invoke(|l| ledger::observe_sheet(&s.ledger, b.balances, l))?;
    if s.pool.pool != s.book.pool
        || s.ledger.default_debt != s.book.default_debt
        || s.ledger.active_debt.checked_add(s.book.default_debt) != Some(s.book.system.debt)
    {
        return Err(Failure::Conservation);
    }
    let mut raw = Wide::from(0);
    let mut surplus = Wide::from(0);
    for v in &s.book.vaults {
        w.charge(s.params.len() as u64, 1, 0)?;
        if v.id != vault_id(p, v.owner)
            || !s.params.iter().any(|q| q.version == v.parameter_version)
        {
            return Err(Failure::Domain);
        }
        if v.status == vaults::Status::Active {
            raw = raw.add(Wide::from(v.collateral), w)?;
        }
    }
    for (_, n) in &s.book.surplus {
        surplus = surplus.add(Wide::from(*n), w)?
    }
    let stock = s.pool.pool.stock;
    let unpaid = sub(stock.allocated, stock.paid, w)?;
    let issuance = add(add(stock.remaining, stock.missed, w)?, unpaid, w)?;
    let roles = [
        (ACTIVE, Asset::Agrs, raw.narrow()?),
        (DEFAULT, Asset::Agrs, s.book.default_collateral),
        (GAS, Asset::Zusd, s.book.gas_balance),
        (SP_Z, Asset::Zusd, s.pool.pool.total),
        (SP_A, Asset::Agrs, s.pool.pool.collateral),
        (ISSUANCE, Asset::Zdex, issuance),
        (STAKE_T, Asset::Zdex, s.staking.total),
        (FEES, Asset::Zusd, s.staking.custody_z),
        (FEES, Asset::Agrs, s.staking.custody_a),
        (SURPLUS, Asset::Agrs, surplus.narrow()?),
    ];
    for (i, a, n) in roles {
        if wallet(s, p.config.accounts[i], a) != n {
            return Err(Failure::Conservation);
        }
    }
    for row in &s.ledger.balances {
        if let Some(i) = p.config.accounts.iter().position(|id| *id == row.account) {
            let a = row.quantity.asset();
            if !roles.iter().any(|(j, b, _)| *j == i && *b == a) && row.quantity.atoms() != 0 {
                return Err(Failure::Conservation);
            }
        }
    }
    if s.nonces
        .iter()
        .any(|(id, _)| p.config.accounts.contains(id))
        || s.book
            .vaults
            .iter()
            .any(|v| p.config.accounts.contains(&v.owner))
    {
        return Err(Failure::Domain);
    }
    check_backing(&s.ledger, s.backing, w)?;
    Ok(())
}
pub fn initialize(
    p: &Policy,
    g: &Genesis,
    v: &impl ReceiptVerifierV1,
    limits: Limits,
) -> Evaluation<State> {
    run(limits, |w| {
        w.charge(p.config.bounds.frame_bytes, 1, p.config.bounds.frame_bytes)?;
        // Reserve all message copies and root/encoding scratch before preparation.
        let preparation = p
            .config
            .bounds
            .frame_bytes
            .checked_mul(6)
            .ok_or(Failure::Arithmetic)?;
        w.charge(preparation, 4, preparation)?;
        let messages = genesis_messages(p, g)?;
        verify(p, &messages, &g.proofs, v, w)?;
        valid_clock(p, g.clock)?;
        if p.config.accounts.contains(&g.funder)
            || g.wallets.supply != 0
            || g.wallets.active_debt != 0
            || g.wallets.default_debt != 0
            || g.wallets
                .balances
                .iter()
                .any(|r| p.config.accounts.contains(&r.account) && r.quantity.atoms() != 0)
        {
            return Err(Failure::Domain);
        }
        check_backing(&g.wallets, g.backing, w)?;
        let pool = stability::State::reference_genesis(stability::FUNDED_CAP)?;
        let plan = w.invoke(|l| {
            ledger::compose(
                &g.wallets,
                &[Movement {
                    from: Some(g.funder),
                    to: Some(p.config.accounts[ISSUANCE]),
                    quantity: Quantity::Zdex(stability::FUNDED_CAP),
                }],
                (0, 0),
                p.config.bounds.balances,
                l,
            )
        })?;
        let s = State {
            policy: p.id,
            version: 0,
            deployed: g.clock.epoch,
            clock: g.clock,
            backing: g.backing,
            oracle_root: [0; 32],
            oracle: None,
            allocation_root: [0; 32],
            book: liquidation::Book {
                vaults: Vec::new(),
                order: Vec::new(),
                priorities: Vec::new(),
                system: vaults::System {
                    collateral: 0,
                    debt: 0,
                    active: 0,
                    stakes: 0,
                    stakes_snapshot: 0,
                    collateral_snapshot: 0,
                    reward_c: Accumulator256::ZERO,
                    reward_d: Accumulator256::ZERO,
                },
                default_collateral: 0,
                default_debt: 0,
                error_c: Accumulator256::ZERO,
                error_d: Accumulator256::ZERO,
                gas_balance: 0,
                surplus: Vec::new(),
                pool: pool.clone(),
            },
            pool: stability::Book {
                pool,
                deposits: Vec::new(),
                front_ends: Vec::new(),
            },
            staking: staking::State {
                total: 0,
                f_z: Accumulator256::ZERO,
                f_a: Accumulator256::ZERO,
                custody_z: 0,
                custody_a: 0,
                zero_z: 0,
                zero_a: 0,
                positions: Vec::new(),
            },
            ledger: plan.post().clone(),
            fee: fees::State {
                base: 0,
                last_epoch: g.clock.epoch,
            },
            params: alloc::vec![vaults::Parameters {
                clock_profile: p.config.clock.map(|c| c.profile),
                drip_profile: p.config.drip,
                ..vaults::Parameters::v1()
            }],
            nonces: Vec::new(),
            seen: Vec::new(),
            events: Vec::new(),
            drip_state: Vec::new(),
            last_trigger: None,
        };
        validate(p, &s, w)?;
        Ok(s)
    })
}
fn prices(p: &Policy, s: &State) -> vaults::Prices {
    let Some(v) = &s.oracle else {
        return vaults::Prices {
            committed: 0,
            pending: None,
            status: vaults::PriceStatus::Unavailable,
            pending_blocked: false,
        };
    };
    let mut status = v.status;
    let mut blocked = v.pending_blocked;
    let o = p.config.oracle;
    let stale = |r: PriceRecord| {
        r.observed_epoch > s.clock.epoch
            || s.clock.epoch - r.observed_epoch > o.max_age
            || r.median_seconds > s.clock.watermark
            || s.clock.watermark - r.median_seconds > o.max_lag
    };
    let committed = v.committed.map_or(0, |r| {
        if status == vaults::PriceStatus::Usable {
            if stale(r) {
                status = vaults::PriceStatus::Stale
            } else if r.deviation > o.max_deviation {
                status = vaults::PriceStatus::Uncertain
            }
        }
        r.value_e8
    });
    if v.pending
        .is_some_and(|r| stale(r) || r.deviation > o.max_deviation)
    {
        blocked = true
    }
    if v.committed.is_none() {
        status = vaults::PriceStatus::Unavailable
    }
    if status == vaults::PriceStatus::Usable {
        let e = p.config.exposure;
        let bound = e
            .debt_ceiling
            .checked_add(e.liquidation_cap)
            .and_then(|n| n.checked_add(e.redemption_cap));
        if bound.is_none_or(|b| b > v.capacity) || s.book.system.debt > e.debt_ceiling {
            status = vaults::PriceStatus::OverBudget
        }
    }
    vaults::Prices {
        committed,
        pending: v.pending.map(|r| r.value_e8),
        status,
        pending_blocked: blocked,
    }
}
fn elapsed(p: &Policy, from: u64, to: u64) -> Result<u64, Failure> {
    let c = p.config.clock.ok_or(Failure::UnboundProfile)?;
    let delta = to.checked_sub(from).ok_or(Failure::Domain)?;
    let n = u128::from(delta) * u128::from(c.minutes);
    u64::try_from(n / u128::from(c.epochs)).map_err(|_| Failure::Arithmetic)
}
fn time(p: &Policy, s: &State) -> Result<fees::LogicalTime, Failure> {
    Ok(fees::LogicalTime {
        epoch: s.clock.epoch,
        elapsed_minutes: elapsed(p, s.fee.last_epoch, s.clock.epoch)?,
    })
}
fn leg(out: &mut Vec<Movement>, from: Option<Id>, to: Option<Id>, quantity: Quantity) {
    if quantity.atoms() != 0 {
        out.push(Movement { from, to, quantity })
    }
}
fn transfer(out: &mut Vec<Movement>, from: Id, to: Id, q: Quantity) {
    leg(out, Some(from), Some(to), q)
}
fn refresh(s: &mut State, w: &mut Work) -> Result<(), Failure> {
    w.charge(
        s.book.vaults.len() as u64,
        s.book.vaults.len() as u64,
        s.book.vaults.len() as u64 * 48,
    )?;
    let mut priorities = Vec::new();
    for v in &s.book.vaults {
        if v.status == vaults::Status::Active {
            priorities.push((v.id, liquidation::fresh(&s.book, v.id, w)?))
        }
    }
    s.book.priorities = priorities;
    Ok(())
}
fn owner(
    p: &Policy,
    s: &mut State,
    o: vaults::Outcome,
    upper: Option<Id>,
    lower: Option<Id>,
    legs: &mut Vec<Movement>,
    w: &mut Work,
) -> Result<(), Failure> {
    let a = p.config.accounts;
    let id = o.vault.id;
    match s.book.vaults.binary_search_by_key(&id, |v| v.id) {
        Ok(i) => s.book.vaults[i] = o.vault,
        Err(i) => {
            if s.book.vaults.len() >= p.config.bounds.vaults as usize {
                return Err(Failure::Reject(1061));
            }
            s.book.vaults.insert(i, o.vault)
        }
    }
    s.book.system = o.system;
    s.fee = o.fee;
    s.book.default_collateral = sub(s.book.default_collateral, o.pending_collateral, w)?;
    s.book.default_debt = sub(s.book.default_debt, o.pending_debt, w)?;
    s.book.gas_balance = sub(
        add(s.book.gas_balance, o.reserve_minted, w)?,
        o.reserve_burned,
        w,
    )?;
    transfer(
        legs,
        a[DEFAULT],
        a[ACTIVE],
        Quantity::Agrs(o.pending_collateral),
    );
    transfer(
        legs,
        o.vault.owner,
        a[ACTIVE],
        Quantity::Agrs(o.collateral_in),
    );
    transfer(
        legs,
        a[ACTIVE],
        o.vault.owner,
        Quantity::Agrs(o.collateral_out),
    );
    leg(legs, None, Some(o.vault.owner), Quantity::Zusd(o.minted));
    leg(legs, Some(o.vault.owner), None, Quantity::Zusd(o.burned));
    leg(legs, None, Some(a[FEES]), Quantity::Zusd(o.fee_paid));
    leg(legs, None, Some(a[GAS]), Quantity::Zusd(o.reserve_minted));
    leg(legs, Some(a[GAS]), None, Quantity::Zusd(o.reserve_burned));
    if o.fee_paid > 0 {
        s.staking =
            w.invoke(|l| staking::fee_event(&s.staking, o.fee_paid, 0, p.config.bounds.stakers, l))?
    }
    if o.vault.status == vaults::Status::Active {
        redemption::cut(&mut s.book, id, o.nicr, upper, lower, w)?
    } else {
        s.book.order.retain(|v| *v != id)
    }
    refresh(s, w)
}
fn drip(p: &Policy, s: &State, r: &Request) -> Result<Option<u128>, Failure> {
    if let Some(d) = &r.drip {
        p.config.drip.ok_or(Failure::UnboundProfile)?;
        if s.last_trigger == Some(s.clock.epoch) && d.amount != 0 {
            return Err(Failure::Canonical);
        }
        if d.amount > s.pool.pool.stock.remaining {
            return Err(Failure::Conservation);
        }
        Ok(Some(d.amount))
    } else {
        Ok(None)
    }
}
fn use_drip(
    p: &Policy,
    s: &mut State,
    r: &Request,
    used: bool,
    w: &mut Work,
) -> Result<(), Failure> {
    if used {
        let d = r.drip.as_ref().ok_or(Failure::UnboundProfile)?;
        seen(s, d.event, p, w)?;
        s.last_trigger = Some(s.clock.epoch);
        s.drip_state = d.next.clone()
    } else if r.drip.is_some() {
        return Err(Failure::Canonical);
    }
    Ok(())
}
/// Immutable inputs for effective fund.
struct FundInputs<'a> {
    policy: &'a Policy,
    request: &'a Request,
    event: Id,
    recipient: Id,
    quantity: Quantity,
}
fn effective_fund(
    input: FundInputs<'_>,
    s: &mut State,
    legs: &mut Vec<Movement>,
    w: &mut Work,
) -> Result<(), Failure> {
    let FundInputs {
        policy: p,
        request: r,
        event,
        recipient,
        quantity: q,
    } = input;
    if q.atoms() == 0 || q.asset() == Asset::Zusd || p.config.accounts.contains(&recipient) {
        return Err(Failure::Domain);
    }
    seen(s, event, p, w)?;
    let mut balances = s.ledger.balances.clone();
    let key = (recipient, q.asset());
    let found = balances.binary_search_by_key(&key, |r| (r.account, r.quantity.asset()));
    let old = found.ok().map_or(0, |i| balances[i].quantity.atoms());
    let n = add(old, q.atoms(), w)?;
    let quantity = match q {
        Quantity::Agrs(_) => Quantity::Agrs(n),
        Quantity::Zdex(_) => Quantity::Zdex(n),
        _ => return Err(Failure::Domain),
    };
    match found {
        Ok(i) => balances[i].quantity = quantity,
        Err(i) => {
            if balances.len() >= p.config.bounds.balances as usize {
                return Err(Failure::Reject(1061));
            }
            balances.insert(
                i,
                ledger::Balance {
                    account: recipient,
                    quantity,
                },
            )
        }
    }
    s.ledger.balances = balances;
    s.backing = r.backing;
    check_backing(&s.ledger, s.backing, w)?;
    leg(legs, None, Some(recipient), q);
    Ok(())
}
fn oracle_update(p: &Policy, s: &mut State, r: &Request, w: &mut Work) -> Result<(), Failure> {
    let o = r.oracle.as_ref().ok_or(Failure::UnboundProfile)?;
    if o.before != s.oracle_root || o.after == [0; 32] || o.view.aggregate_bytes.is_empty() {
        return Err(Failure::Canonical);
    }
    for v in [o.view.committed, o.view.pending].into_iter().flatten() {
        if v.value_e8 == 0
            || v.aggregate == [0; 32]
            || v.observed_epoch > s.clock.epoch
            || v.median_seconds > s.clock.watermark
            || v.committed_epoch > s.clock.epoch
        {
            return Err(Failure::Canonical);
        }
    }
    let old = s.oracle.as_ref();
    let rules = p.config.oracle;
    // A new pending report cannot clear a committed dispute; an admitted
    // pending restriction cannot disappear through a status-only update.
    if !matches!(
        o.action,
        OracleAction::CommitMatured | OracleAction::InitializeFirstMatured
    ) && old.is_some_and(|v| v.status == vaults::PriceStatus::Disputed)
        && o.view.status != vaults::PriceStatus::Disputed
    {
        return Err(Failure::Canonical);
    }
    if o.action == OracleAction::RefreshStatus
        && old.is_some_and(|v| v.pending_blocked)
        && !o.view.pending_blocked
    {
        return Err(Failure::Canonical);
    }
    if let Some(pending) = o.view.pending {
        if (pending.committed_epoch != 0
            || pending.published_epoch != s.clock.epoch
            || pending.earliest_commit_epoch
                != pending
                    .published_epoch
                    .checked_add(rules.confirmation.max(rules.dispute_open))
                    .ok_or(Failure::Arithmetic)?
            || pending.dispute_until
                != pending
                    .published_epoch
                    .checked_add(rules.dispute_open)
                    .ok_or(Failure::Arithmetic)?)
            && (o.action != OracleAction::RefreshStatus)
        {
            return Err(Failure::Canonical);
        }
        if old
            .and_then(|v| v.committed)
            .is_some_and(|r| pending.round <= r.round)
            || (o.action == OracleAction::ImportPending
                && old
                    .and_then(|v| v.pending)
                    .is_some_and(|r| pending.round <= r.round))
        {
            return Err(Failure::Canonical);
        }
    }
    if let Some(c) = o.view.committed
        && (c.earliest_commit_epoch > c.committed_epoch
            || c.committed_epoch > s.clock.epoch
            || c.earliest_commit_epoch
                < c.published_epoch
                    .checked_add(rules.confirmation.max(rules.dispute_open))
                    .ok_or(Failure::Arithmetic)?)
    {
        return Err(Failure::Canonical);
    }
    match o.action {
        OracleAction::Use => return Err(Failure::Canonical),
        OracleAction::ImportPending => {
            if o.view.pending.is_none() || old.and_then(|v| v.committed) != o.view.committed {
                return Err(Failure::Canonical);
            }
        }
        OracleAction::CommitMatured => {
            let pending = old.and_then(|v| v.pending).ok_or(Failure::Canonical)?;
            let committed = o.view.committed.ok_or(Failure::Canonical)?;
            if committed.round != pending.round
                || committed.published_epoch != pending.published_epoch
                || committed.earliest_commit_epoch != pending.earliest_commit_epoch
                || committed.dispute_until != pending.dispute_until
                || committed.committed_epoch != s.clock.epoch
                || committed.aggregate != pending.aggregate
                || committed.value_e8 != pending.value_e8
                || committed.observed_epoch != pending.observed_epoch
                || committed.median_seconds != pending.median_seconds
                || committed.deviation != pending.deviation
                || o.view.pending.is_some()
            {
                return Err(Failure::Canonical);
            }
        }
        OracleAction::InitializeFirstMatured => {
            if old.and_then(|v| v.committed).is_some()
                || o.view.committed.is_none()
                || o.view.pending.is_some()
            {
                return Err(Failure::Canonical);
            }
        }
        OracleAction::RefreshStatus => {
            if old.is_none()
                || old.and_then(|v| v.committed) != o.view.committed
                || old.and_then(|v| v.pending) != o.view.pending
            {
                return Err(Failure::Canonical);
            }
        }
    }
    if let (Some(a), Some(b)) = (old.and_then(|v| v.committed), o.view.committed)
        && (a.aggregate != b.aggregate
            && (b.round <= a.round
                || b.observed_epoch < a.observed_epoch
                || b.median_seconds < a.median_seconds
                || b.committed_epoch < a.committed_epoch))
    {
        return Err(Failure::Canonical);
    }
    seen(s, o.event.ok_or(Failure::Canonical)?, p, w)?;
    s.oracle_root = o.after;
    s.oracle = Some(o.view.clone());
    Ok(())
}
fn exposure(
    p: &Policy,
    s: &mut State,
    class: ExposureClass,
    amount: u128,
    event: Id,
    w: &mut Work,
) -> Result<(), Failure> {
    let cap = match class {
        ExposureClass::Liquidation => p.config.exposure.liquidation_cap,
        ExposureClass::Redemption => p.config.exposure.redemption_cap,
    };
    let mut n = Wide::from(amount);
    for e in &s.events {
        w.charge(1, 1, 0)?;
        if e.class == class {
            n = n.add(Wide::from(e.amount), w)?
        }
    }
    if n.cmp(Wide::from(cap)).is_gt() {
        return Err(Failure::Reject(1024));
    }
    if s.events.len() >= p.config.bounds.events as usize {
        return Err(Failure::Reject(1061));
    }
    let at = s
        .events
        .binary_search_by_key(&event, |v| v.id)
        .map_or_else(Ok, |_| Err(Failure::Reject(1001)))?;
    s.events.insert(
        at,
        ExposureEvent {
            id: event,
            epoch: s.clock.epoch,
            class,
            amount,
        },
    );
    Ok(())
}
fn usd_up(coll: u128, price: u128, w: &mut Work) -> Result<u128, Failure> {
    let n = Wide::from(coll).mul(price, w)?.mul(10_000_000_000, w)?;
    let (q, r) = n.quotient(Wide::from(Q), w)?;
    if r.narrow()? == 0 {
        Ok(q)
    } else {
        add(q, 1, w)
    }
}
fn apply(
    p: &Policy,
    pre: &State,
    r: &Request,
    s: &mut State,
    legs: &mut Vec<Movement>,
    trace: &mut Vec<u8>,
    w: &mut Work,
) -> Result<(), Failure> {
    let a = p.config.accounts;
    let bounds = p.sp_bounds();
    let prices = prices(p, s);
    let params = *s.params.last().ok_or(Failure::Domain)?;
    let actor = r.actor;
    let wc = wallet(pre, actor, Asset::Agrs);
    let wz = wallet(pre, actor, Asset::Zusd);
    let wt = wallet(pre, actor, Asset::Zdex);
    let index = pre
        .book
        .vaults
        .binary_search_by_key(&vault_id(p, actor), |v| v.id)
        .ok();
    match &r.command {
        Command::Open {
            collateral,
            principal,
            max_fee,
            upper,
            lower,
        } => {
            let t = if vaults::recovery(s.book.system, prices.committed, w)? {
                None
            } else {
                Some(time(p, s)?)
            };
            let out = w.invoke(|l| {
                vaults::open(
                    vaults::OpenInputs {
                        existing: index.map(|i| pre.book.vaults[i]),
                        id: vault_id(p, actor),
                        owner: actor,
                        collateral: *collateral,
                        principal: *principal,
                        balances: (wc, wz),
                        system: pre.book.system,
                        parameters: params,
                        fee: pre.fee,
                        prices,
                        max_fee: *max_fee,
                        time: t,
                    },
                    l,
                )
            })?;
            owner(p, s, out, *upper, *lower, legs, w)?
        }
        Command::Adjust {
            collateral,
            debt,
            max_fee,
            upper,
            lower,
        } => {
            let v = pre.book.vaults[*index.as_ref().ok_or(Failure::Reject(1011))?];
            let t = if matches!(debt, vaults::Delta::Add(_))
                && !vaults::recovery(s.book.system, prices.committed, w)?
            {
                Some(time(p, s)?)
            } else {
                None
            };
            let out = w.invoke(|l| {
                vaults::adjust(
                    vaults::AdjustInputs {
                        vault: v,
                        system: pre.book.system,
                        parameters: params,
                        fee: pre.fee,
                        prices,
                        collateral_delta: *collateral,
                        debt_delta: *debt,
                        wallet: (wc, wz),
                        max_fee: *max_fee,
                        time: t,
                    },
                    l,
                )
            })?;
            owner(p, s, out, *upper, *lower, legs, w)?
        }
        Command::Close => {
            let v = pre.book.vaults[index.ok_or(Failure::Reject(1011))?];
            let out = w.invoke(|l| vaults::close(v, pre.book.system, pre.fee, prices, wz, l))?;
            owner(p, s, out, None, None, legs, w)?
        }
        Command::ClaimSurplus => {
            let out = w.invoke(|l| {
                common::claim_surplus(&pre.book.surplus, actor, p.config.bounds.vaults, l)
            })?;
            s.book.surplus = out.remaining;
            transfer(legs, a[SURPLUS], actor, Quantity::Agrs(out.amount))
        }
        Command::Pool(c) => {
            if *c == stability::Command::MoveGainToVault {
                return Err(Failure::Domain);
            }
            let amount = drip(p, s, r)?;
            let out = w.invoke(|l| {
                stability::change(
                    stability::ChangeInputs {
                        pre: &pre.pool,
                        actor,
                        command: *c,
                        wallet_zusd: wz,
                        issuance: amount,
                        prices,
                        inventory: &pre.book.vaults,
                        system: pre.book.system,
                        bounds,
                    },
                    l,
                )
            })?;
            let used = !matches!(c, stability::Command::RegisterFrontEnd(_));
            use_drip(p, s, r, used, w)?;
            transfer(legs, actor, a[SP_Z], Quantity::Zusd(out.principal_in));
            transfer(legs, a[SP_Z], actor, Quantity::Zusd(out.principal_out));
            transfer(legs, a[SP_A], actor, Quantity::Agrs(out.gains.collateral));
            transfer(
                legs,
                a[ISSUANCE],
                actor,
                Quantity::Zdex(out.gains.depositor_zdex),
            );
            if let Some(f) = out.front_end {
                transfer(
                    legs,
                    a[ISSUANCE],
                    f,
                    Quantity::Zdex(out.gains.front_end_zdex),
                )
            }
            s.pool = out.book;
            s.book.pool = s.pool.pool.clone()
        }
        Command::MoveGain { upper, lower } => {
            let amount = drip(p, s, r)?;
            let out = w.invoke(|l| {
                composition::move_gain_to_vault(
                    composition::GainToVaultInputs {
                        pre: &pre.book,
                        pool: &pre.pool,
                        actor,
                        parameters: params,
                        fee: pre.fee,
                        prices,
                        issuance: amount,
                        upper: *upper,
                        lower: *lower,
                        capacity: p.config.bounds.vaults,
                        bounds,
                    },
                    l,
                )
            })?;
            use_drip(p, s, r, true, w)?;
            transfer(
                legs,
                a[SP_A],
                a[ACTIVE],
                Quantity::Agrs(out.settlement.collateral),
            );
            transfer(
                legs,
                a[DEFAULT],
                a[ACTIVE],
                Quantity::Agrs(out.owner.pending_collateral),
            );
            transfer(
                legs,
                a[ISSUANCE],
                actor,
                Quantity::Zdex(out.settlement.depositor_zdex),
            );
            if let Some(f) = out.front_end {
                transfer(
                    legs,
                    a[ISSUANCE],
                    f,
                    Quantity::Zdex(out.settlement.front_end_zdex),
                )
            }
            s.book = out.vaults;
            s.pool = out.stability
        }
        Command::Stake(c) => {
            let out = w.invoke(|l| {
                staking::change(&pre.staking, actor, *c, wt, p.config.bounds.stakers, l)
            })?;
            transfer(legs, actor, a[STAKE_T], Quantity::Zdex(out.stake_in));
            transfer(legs, a[STAKE_T], actor, Quantity::Zdex(out.stake_out));
            transfer(legs, a[FEES], actor, Quantity::Zusd(out.zusd_gain));
            transfer(legs, a[FEES], actor, Quantity::Agrs(out.agrs_gain));
            s.staking = out.state
        }
        Command::Liquidate(c) => {
            let amount = drip(p, s, r)?;
            let out = w.invoke(|l| {
                liquidation::liquidate(
                    &pre.book,
                    c,
                    prices,
                    amount,
                    p.config.bounds.vaults,
                    bounds,
                    l,
                )
            })?;
            let used = out.burned > 0 && pre.pool.pool.total > 0;
            use_drip(p, s, r, used, w)?;
            for (id, part) in out.selected.iter().zip(&out.partitions) {
                let v = pre.book.vaults[pre
                    .book
                    .vaults
                    .binary_search_by_key(id, |v| v.id)
                    .map_err(|_| Failure::Domain)?];
                let effective = crate::vaults::effective(v, pre.book.system, w)?;
                transfer(
                    legs,
                    a[DEFAULT],
                    a[ACTIVE],
                    Quantity::Agrs(sub(effective.collateral, v.collateral, w)?),
                );
                transfer(legs, a[ACTIVE], a[SP_A], Quantity::Agrs(part.sp_collateral));
                transfer(
                    legs,
                    a[ACTIVE],
                    a[DEFAULT],
                    Quantity::Agrs(part.redistributed_collateral),
                );
                transfer(
                    legs,
                    a[ACTIVE],
                    actor,
                    Quantity::Agrs(part.keeper_collateral),
                );
                transfer(legs, a[ACTIVE], a[SURPLUS], Quantity::Agrs(part.surplus));
                transfer(legs, a[GAS], actor, Quantity::Zusd(part.reserve));
            }
            leg(legs, Some(a[SP_Z]), None, Quantity::Zusd(out.burned));
            let mut coll = 0;
            let mut gas = 0;
            for q in &out.partitions {
                coll = add(coll, q.collateral, w)?;
                gas = add(gas, q.reserve, w)?
            }
            let charge = add(usd_up(coll, prices.committed, w)?, gas, w)?;
            exposure(
                p,
                s,
                ExposureClass::Liquidation,
                charge,
                encoding::hash(&encoding::payload(r)),
                w,
            )?;
            let mut t = W::new(b"liquidation/1\0");
            t.len(out.selected.len());
            for id in &out.selected {
                t.id(*id)
            }
            t.n(out.burned);
            t.n(out.redistributed_debt);
            t.n(out.redistributed_collateral);
            *trace = t.0;
            s.book = out.book;
            s.pool.pool = s.book.pool.clone()
        }
        Command::Redeem(c) => {
            let t = Some(time(p, s)?);
            let boot = Some(elapsed(p, s.deployed, s.clock.epoch)?);
            let holder = redemption::Holder {
                zusd: wz,
                agrs: wc,
                staking_agrs: pre.staking.custody_a,
                fee: pre.fee,
            };
            let out = w.invoke(|l| {
                redemption::redeem(
                    redemption::RedeemInputs {
                        pre: &pre.book,
                        holder,
                        command: *c,
                        prices,
                        time: t,
                        bootstrap_minutes: boot,
                        parameters: params,
                        capacity: p.config.bounds.vaults,
                        bounds,
                    },
                    l,
                )
            })?;
            for id in &out.trace.settled {
                let i = pre
                    .book
                    .vaults
                    .binary_search_by_key(id, |v| v.id)
                    .map_err(|_| Failure::Domain)?;
                let v = pre.book.vaults[i];
                let eff = vaults::effective(v, pre.book.system, w)?;
                transfer(
                    legs,
                    a[DEFAULT],
                    a[ACTIVE],
                    Quantity::Agrs(sub(eff.collateral, v.collateral, w)?),
                )
            }
            leg(legs, Some(actor), None, Quantity::Zusd(out.trace.executed));
            leg(
                legs,
                Some(a[GAS]),
                None,
                Quantity::Zusd(out.trace.reserve_burn),
            );
            transfer(
                legs,
                a[ACTIVE],
                actor,
                Quantity::Agrs(sub(out.trace.gross, out.trace.fee, w)?),
            );
            transfer(legs, a[ACTIVE], a[FEES], Quantity::Agrs(out.trace.fee));
            transfer(
                legs,
                a[ACTIVE],
                a[SURPLUS],
                Quantity::Agrs(out.trace.surplus),
            );
            s.staking = w.invoke(|l| {
                staking::fee_event(&pre.staking, 0, out.trace.fee, p.config.bounds.stakers, l)
            })?;
            s.fee = out.holder.fee;
            exposure(
                p,
                s,
                ExposureClass::Redemption,
                usd_up(
                    add(out.trace.gross, out.trace.surplus, w)?,
                    prices.committed,
                    w,
                )?,
                encoding::hash(&encoding::payload(r)),
                w,
            )?;
            let mut t = W::new(b"redemption/1\0");
            for ids in [&out.trace.visited, &out.trace.closed, &out.trace.settled] {
                t.len(ids.len());
                for id in ids {
                    t.id(*id)
                }
            }
            for n in [
                out.trace.executed,
                out.trace.unfilled,
                out.trace.gross,
                out.trace.reserve_burn,
                out.trace.surplus,
                out.trace.fee,
            ] {
                t.n(n)
            }
            t.u8(match out.trace.stop {
                redemption::Stop::Filled => 0,
                redemption::Stop::VisitCap => 1,
                redemption::Stop::CancelledPartial => 2,
                redemption::Stop::End => 3,
            });
            *trace = t.0;
            s.book = out.book
        }
        Command::ObserveOracle => oracle_update(p, s, r, w)?,
        Command::AdvanceClock => {}
        Command::Fund {
            event,
            recipient,
            quantity,
        } => effective_fund(
            FundInputs {
                policy: p,
                request: r,
                event: *event,
                recipient: *recipient,
                quantity: *quantity,
            },
            s,
            legs,
            w,
        )?,
        Command::ChangeParameters {
            policy,
            version,
            minimum_net,
            reserve,
        } => {
            let approved = p.config.change.ok_or(Failure::UnboundProfile)?;
            if *policy != approved.policy {
                return Err(Failure::UnboundProfile);
            }
            if *version != params.version.checked_add(1).ok_or(Failure::Arithmetic)?
                || *minimum_net == 0
                || *reserve == 0
                || minimum_net.checked_add(*reserve).is_none()
            {
                return Err(Failure::Domain);
            }
            for row in &pre.book.vaults {
                w.charge(1, 1, 0)?;
                if row.status == vaults::Status::Active
                    && sub(row.debt, row.reserve, w)? < *minimum_net
                {
                    return Err(Failure::UnboundProfile);
                }
            }
            if s.params.len() >= p.config.bounds.params as usize {
                return Err(Failure::Reject(1061));
            }
            s.params.push(vaults::Parameters {
                version: *version,
                minimum_net: *minimum_net,
                reserve: *reserve,
                ..params
            })
        }
        Command::ReleaseExposure { events } => {
            if events.is_empty() || events.windows(2).any(|v| v[0] >= v[1]) {
                return Err(Failure::Canonical);
            }
            for id in events {
                w.charge(s.events.len() as u64, s.events.len() as u64, 0)?;
                let i = s
                    .events
                    .binary_search_by_key(id, |e| e.id)
                    .map_err(|_| Failure::Canonical)?;
                if u128::from(s.events[i].epoch) + u128::from(p.config.exposure.window)
                    > u128::from(s.clock.epoch)
                {
                    return Err(Failure::Canonical);
                }
                s.events.remove(i);
            }
        }
    }
    Ok(())
}
pub fn decide(
    p: &Policy,
    pre: &State,
    r: &Request,
    v: &impl ReceiptVerifierV1,
    limits: Limits,
) -> Evaluation<Prepared> {
    run(limits, |w| {
        w.charge(32, 16, 0)?;
        bounded_request(p, r)?;
        validate(p, pre, w)?;
        let preparation = p
            .config
            .bounds
            .frame_bytes
            .checked_mul(12)
            .ok_or(Failure::Arithmetic)?;
        w.charge(preparation, 9, preparation)?;
        let messages = receipt_messages(p, pre, r)?;
        verify(p, &messages, &r.proofs, v, w)?;
        if p.config.accounts.contains(&r.actor) {
            return Err(Failure::Reject(1000));
        }
        let next_nonce =
            w.invoke(|l| common::identity(r.actor, None, r.nonce, pre.nonce(r.actor), l))?;
        valid_clock(p, r.clock)?;
        if r.clock.epoch < pre.clock.epoch || r.clock.watermark < pre.clock.watermark {
            return Err(Failure::Canonical);
        }
        if !matches!(r.command, Command::AdvanceClock) && r.clock != pre.clock {
            return Err(Failure::Canonical);
        }
        if !matches!(r.command, Command::Fund { .. }) && r.backing != pre.backing {
            return Err(Failure::Canonical);
        }
        if price_required(&r.command) {
            let o = r.oracle.as_ref().ok_or(Failure::UnboundProfile)?;
            if o.action != OracleAction::Use
                || o.event.is_some()
                || o.before != pre.oracle_root
                || o.after != pre.oracle_root
                || pre.oracle.as_ref() != Some(&o.view)
            {
                return Err(Failure::Canonical);
            }
            let a = r.allocation.ok_or(Failure::UnboundProfile)?;
            let e = p.config.exposure;
            let bound = add(
                add(e.debt_ceiling, e.liquidation_cap, w)?,
                e.redemption_cap,
                w,
            )?;
            if a.root == [0; 32]
                || a.root != o.view.capacity_root
                || a.capacity > o.view.capacity
                || a.capacity < bound
            {
                return Err(Failure::Reject(1024));
            }
        }
        w.charge(p.config.bounds.frame_bytes, 1, p.config.bounds.frame_bytes)?;
        let mut post = pre.clone();
        if matches!(r.command, Command::AdvanceClock) {
            post.clock = r.clock
        }
        if let Some(a) = r.allocation {
            post.allocation_root = a.root
        }
        let mut legs = Vec::new();
        let mut trace = Vec::new();
        apply(p, pre, r, &mut post, &mut legs, &mut trace, w)?;
        if !matches!(r.command, Command::Fund { .. }) {
            let active = sub(post.book.system.debt, post.book.default_debt, w)?;
            let plan = w.invoke(|l| {
                ledger::compose(
                    &pre.ledger,
                    &legs,
                    (active, post.book.default_debt),
                    p.config.bounds.balances,
                    l,
                )
            })?;
            post.ledger = plan.post().clone();
        }
        if post.book.system.debt > p.config.exposure.debt_ceiling
            && post.book.system.debt > pre.book.system.debt
        {
            return Err(Failure::Reject(1024));
        }
        match post.nonces.binary_search_by_key(&r.actor, |v| v.0) {
            Ok(i) => post.nonces[i].1 = next_nonce,
            Err(i) => {
                if post.nonces.len() >= p.config.bounds.nonces as usize {
                    return Err(Failure::Reject(1061));
                }
                post.nonces.insert(i, (r.actor, next_nonce));
            }
        }
        post.version = pre.version.checked_add(1).ok_or(Failure::Arithmetic)?;
        validate(p, &post, w)?;
        // One canonical bundle contains every output, replay update and economic leg.
        w.charge(p.config.bounds.frame_bytes, 1, p.config.bounds.frame_bytes)?;
        let bytes = encoding::bundle(pre.root(), r, &post, &legs, &trace, w.usage);
        if bytes.len() as u64 > p.config.bounds.frame_bytes {
            return Err(Failure::Capacity);
        }
        Ok(Prepared {
            pre: pre.root(),
            request: r.clone(),
            post,
            legs,
            trace,
            usage: w.usage,
            bytes,
        })
    })
}
