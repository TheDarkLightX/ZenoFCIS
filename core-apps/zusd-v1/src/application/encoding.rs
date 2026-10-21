//! Canonical step4/1 BE transport. Fixed tags, mandatory fields, explicit vector
//! lengths; no Debug/serde/host representation enters a root or signed message.
use super::*;
use crate::Accumulator256;
#[cfg(any(test, feature = "std"))]
use crate::Failure;
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
pub(super) fn hash(b: &[u8]) -> Id {
    *RustCryptoSha256::hash(b).as_bytes()
}
// Source identity includes all native economic code and this complete adapter.
// Source text is an immutable build input, not a caller-selected evaluator.
pub(super) fn core() -> Id {
    let mut w = W::new(b"zusd.step4.core/1\0");
    for (path, source) in super::source_closure::FILES {
        w.blob(path.as_bytes());
        w.blob(source);
    }
    hash(&w.0)
}
pub(super) struct W(pub Vec<u8>);
impl W {
    pub fn new(label: &[u8]) -> Self {
        Self(label.to_vec())
    }
    pub fn u8(&mut self, x: u8) {
        self.0.push(x)
    }
    pub fn bool(&mut self, x: bool) {
        self.u8(u8::from(x))
    }
    pub fn u32(&mut self, x: u32) {
        self.0.extend_from_slice(&x.to_be_bytes())
    }
    pub fn u64(&mut self, x: u64) {
        self.0.extend_from_slice(&x.to_be_bytes())
    }
    pub fn n(&mut self, x: u128) {
        self.0.extend_from_slice(&x.to_be_bytes())
    }
    pub fn id(&mut self, x: Id) {
        self.0.extend_from_slice(&x)
    }
    pub fn len(&mut self, n: usize) {
        self.u64(n as u64)
    }
    pub fn blob(&mut self, b: &[u8]) {
        self.len(b.len());
        self.0.extend_from_slice(b)
    }
    pub fn opt<T>(&mut self, x: Option<T>, f: impl FnOnce(&mut Self, T)) {
        self.bool(x.is_some());
        if let Some(x) = x {
            f(self, x)
        }
    }
    fn acc(&mut self, x: Accumulator256) {
        let (h, l) = x.limbs();
        self.n(h);
        self.n(l)
    }
}
fn clock(w: &mut W, c: Clock) {
    w.id(c.backend);
    w.id(c.finality);
    w.id(c.sync);
    w.u64(c.epoch);
    w.u64(c.watermark)
}
fn backing(w: &mut W, c: Backing) {
    w.id(c.custody);
    w.id(c.funding);
    w.id(c.supply_root);
    w.n(c.agrs);
    w.n(c.zdex);
    w.n(c.zdex_supply);
    w.n(c.zdex_burned)
}
fn params(w: &mut W, p: vaults::Parameters) {
    w.u64(p.version);
    w.n(p.minimum_net);
    w.n(p.reserve);
    w.opt(p.clock_profile, W::id);
    w.opt(p.drip_profile, W::id)
}
fn delta(w: &mut W, d: vaults::Delta) {
    match d {
        vaults::Delta::None => w.u8(0),
        vaults::Delta::Add(x) => {
            w.u8(1);
            w.n(x)
        }
        vaults::Delta::Remove(x) => {
            w.u8(2);
            w.n(x)
        }
    }
}
fn quantity(w: &mut W, q: ledger::Quantity) {
    w.u8(q.asset() as u8);
    w.n(q.atoms())
}
fn record(w: &mut W, r: PriceRecord) {
    w.id(r.aggregate);
    w.u64(r.round);
    w.u64(r.published_epoch);
    w.u64(r.earliest_commit_epoch);
    w.u64(r.dispute_until);
    w.n(r.value_e8);
    w.u64(r.observed_epoch);
    w.u64(r.median_seconds);
    w.u64(r.committed_epoch);
    w.n(r.deviation)
}
fn view(w: &mut W, v: &PriceView) {
    w.opt(v.committed, record);
    w.opt(v.pending, record);
    w.bool(v.pending_blocked);
    w.u8(match v.status {
        vaults::PriceStatus::Usable => 0,
        vaults::PriceStatus::Unavailable => 1,
        vaults::PriceStatus::Stale => 2,
        vaults::PriceStatus::Disputed => 3,
        vaults::PriceStatus::Uncertain => 4,
        vaults::PriceStatus::OverBudget => 5,
    });
    w.n(v.capacity);
    w.id(v.capacity_root);
    w.blob(&v.aggregate_bytes)
}
fn oracle(w: &mut W, r: &OracleReceipt) {
    w.id(r.before);
    w.id(r.after);
    w.opt(r.event, W::id);
    w.u8(match r.action {
        OracleAction::Use => 0,
        OracleAction::ImportPending => 1,
        OracleAction::CommitMatured => 2,
        OracleAction::InitializeFirstMatured => 3,
        OracleAction::RefreshStatus => 4,
    });
    view(w, &r.view)
}
fn pool_command(w: &mut W, c: stability::Command) {
    match c {
        stability::Command::Deposit { amount, tag } => {
            w.u8(0);
            w.n(amount);
            w.opt(tag, W::id)
        }
        stability::Command::Withdraw(x) => {
            w.u8(1);
            w.n(x)
        }
        stability::Command::Claim => w.u8(2),
        stability::Command::MoveGainToVault => w.u8(3),
        stability::Command::RegisterFrontEnd(k) => {
            w.u8(4);
            w.n(k)
        }
    }
}
fn stake_command(w: &mut W, c: staking::Command) {
    match c {
        staking::Command::Stake(x) => {
            w.u8(0);
            w.n(x)
        }
        staking::Command::Unstake(x) => {
            w.u8(1);
            w.n(x)
        }
        staking::Command::Claim => w.u8(2),
    }
}
fn liquidation_command(w: &mut W, c: &liquidation::Command) {
    match c {
        liquidation::Command::Direct(id) => {
            w.u8(0);
            w.id(*id)
        }
        liquidation::Command::Lowest(n) => {
            w.u8(1);
            w.u32(*n)
        }
        liquidation::Command::Batch(ids) => {
            w.u8(2);
            w.len(ids.len());
            for id in ids {
                w.id(*id)
            }
        }
    }
}
fn redemption_command(w: &mut W, c: crate::redemption::Command) {
    w.n(c.maximum);
    w.u32(c.visits);
    w.opt(c.first, W::id);
    w.n(c.partial_nicr);
    w.opt(c.upper, W::id);
    w.opt(c.lower, W::id);
    w.n(c.max_fee);
    w.n(c.min_out)
}
fn command(w: &mut W, c: &Command) {
    match c {
        Command::Open {
            collateral,
            principal,
            max_fee,
            upper,
            lower,
        } => {
            w.u8(0);
            w.n(*collateral);
            w.n(*principal);
            w.n(*max_fee);
            w.opt(*upper, W::id);
            w.opt(*lower, W::id)
        }
        Command::Adjust {
            collateral,
            debt,
            max_fee,
            upper,
            lower,
        } => {
            w.u8(1);
            delta(w, *collateral);
            delta(w, *debt);
            w.n(*max_fee);
            w.opt(*upper, W::id);
            w.opt(*lower, W::id)
        }
        Command::Close => w.u8(2),
        Command::ClaimSurplus => w.u8(3),
        Command::Pool(c) => {
            w.u8(4);
            pool_command(w, *c)
        }
        Command::MoveGain { upper, lower } => {
            w.u8(5);
            w.opt(*upper, W::id);
            w.opt(*lower, W::id)
        }
        Command::Stake(c) => {
            w.u8(6);
            stake_command(w, *c)
        }
        Command::Liquidate(c) => {
            w.u8(7);
            liquidation_command(w, c)
        }
        Command::Redeem(c) => {
            w.u8(8);
            redemption_command(w, *c)
        }
        Command::ObserveOracle => w.u8(9),
        Command::AdvanceClock => w.u8(10),
        Command::Fund {
            event,
            recipient,
            quantity: q,
        } => {
            w.u8(11);
            w.id(*event);
            w.id(*recipient);
            quantity(w, *q)
        }
        Command::ChangeParameters {
            policy,
            version,
            minimum_net,
            reserve,
        } => {
            w.u8(12);
            w.id(*policy);
            w.u64(*version);
            w.n(*minimum_net);
            w.n(*reserve)
        }
        Command::ReleaseExposure { events } => {
            w.u8(13);
            w.len(events.len());
            for e in events {
                w.id(*e)
            }
        }
    }
}
pub(super) fn payload(r: &Request) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.request/1\0");
    w.id(r.actor);
    w.u64(r.nonce);
    command(&mut w, &r.command);
    clock(&mut w, r.clock);
    w.opt(r.oracle.as_ref(), oracle);
    backing(&mut w, r.backing);
    w.opt(r.allocation, |w, a| {
        w.id(a.root);
        w.n(a.capacity)
    });
    w.opt(r.drip.as_ref(), |w, d| {
        w.id(d.event);
        w.n(d.amount);
        w.blob(&d.next)
    });
    w.0
}
pub(super) fn request(r: &Request) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.signed/1\0");
    w.blob(&payload(r));
    w.len(r.proofs.len());
    for p in &r.proofs {
        w.u8(p.domain as u8);
        w.blob(&p.bytes)
    }
    w.0
}
fn sheet(w: &mut W, s: &ledger::Sheet) {
    w.len(s.balances.len());
    for b in &s.balances {
        w.id(b.account);
        quantity(w, b.quantity)
    }
    w.n(s.supply);
    w.n(s.active_debt);
    w.n(s.default_debt)
}
pub(super) fn genesis(g: &Genesis) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.genesis/1\0");
    sheet(&mut w, &g.wallets);
    w.id(g.funder);
    clock(&mut w, g.clock);
    backing(&mut w, g.backing);
    w.0
}
pub(super) fn config(c: &Config) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.policy/1\0");
    w.id(core());
    for id in [
        c.chain,
        c.actor_registry,
        c.clock_backend,
        c.oracle_verifier,
        c.custody_verifier,
        c.funding_verifier,
        c.allocation_verifier,
        c.release_verifier,
        c.drip_verifier,
    ] {
        w.id(id)
    }
    w.opt(c.clock, |w, c| {
        w.id(c.profile);
        w.u64(c.epochs);
        w.u64(c.minutes)
    });
    w.opt(c.drip, W::id);
    w.opt(c.change, |w, c| {
        w.id(c.policy);
        w.id(c.authority)
    });
    let o = c.oracle;
    for id in [o.feed, o.query, o.profile, o.scheme, o.evaluator] {
        w.id(id)
    }
    w.u64(o.max_age);
    w.u64(o.max_lag);
    w.n(o.max_deviation);
    w.u64(o.confirmation);
    w.u64(o.dispute_open);
    let e = c.exposure;
    w.u64(e.window);
    w.n(e.debt_ceiling);
    w.n(e.liquidation_cap);
    w.n(e.redemption_cap);
    let b = c.bounds;
    for n in [
        b.vaults,
        b.balances,
        b.deposits,
        b.front_ends,
        b.stakers,
        b.history,
        b.epochs,
        b.nonces,
        b.seen,
        b.events,
        b.params,
        b.proof_bytes,
        b.drip_bytes,
    ] {
        w.u32(n)
    }
    w.u64(b.frame_bytes);
    w.u64(b.history_bytes);
    w.u64(b.verification_steps);
    for id in c.accounts {
        w.id(id)
    }
    w.0
}
fn sp(w: &mut W, s: &stability::State) {
    w.n(s.total);
    w.n(s.p);
    w.u64(s.epoch);
    w.u64(s.scale);
    w.acc(s.error_s);
    w.acc(s.error_loss);
    w.acc(s.error_g);
    w.n(s.collateral);
    for n in [
        s.stock.remaining,
        s.stock.scheduled,
        s.stock.allocated,
        s.stock.missed,
        s.stock.paid,
    ] {
        w.n(n)
    }
    w.len(s.history.len());
    for c in &s.history {
        w.u64(c.epoch);
        w.u64(c.scale);
        w.acc(c.s);
        w.acc(c.g)
    }
    w.len(s.epochs.len());
    for e in &s.epochs {
        w.u64(e.epoch);
        w.u64(e.last_scale);
        w.bool(e.sealed)
    }
}
fn book(w: &mut W, b: &liquidation::Book) {
    w.len(b.vaults.len());
    for v in &b.vaults {
        w.u64(v.parameter_version);
        w.id(v.id);
        w.id(v.owner);
        w.u8(match v.status {
            vaults::Status::Active => 0,
            vaults::Status::ClosedOwner => 1,
            vaults::Status::ClosedLiquidation => 2,
            vaults::Status::ClosedRedemption => 3,
        });
        for n in [v.collateral, v.debt, v.reserve, v.stake] {
            w.n(n)
        }
        w.acc(v.snapshot_c);
        w.acc(v.snapshot_d)
    }
    w.len(b.order.len());
    for id in &b.order {
        w.id(*id)
    }
    w.len(b.priorities.len());
    for (id, n) in &b.priorities {
        w.id(*id);
        w.n(*n)
    }
    let s = b.system;
    w.n(s.collateral);
    w.n(s.debt);
    w.u32(s.active);
    w.n(s.stakes);
    w.n(s.stakes_snapshot);
    w.n(s.collateral_snapshot);
    w.acc(s.reward_c);
    w.acc(s.reward_d);
    w.n(b.default_collateral);
    w.n(b.default_debt);
    w.acc(b.error_c);
    w.acc(b.error_d);
    w.n(b.gas_balance);
    w.len(b.surplus.len());
    for (id, n) in &b.surplus {
        w.id(*id);
        w.n(*n)
    }
    sp(w, &b.pool)
}
pub(super) fn state(s: &State) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.state/1\0");
    w.id(s.policy);
    w.u64(s.version);
    w.u64(s.deployed);
    clock(&mut w, s.clock);
    backing(&mut w, s.backing);
    w.id(s.oracle_root);
    w.opt(s.oracle.as_ref(), view);
    w.id(s.allocation_root);
    book(&mut w, &s.book);
    sp(&mut w, &s.pool.pool);
    w.len(s.pool.deposits.len());
    for d in &s.pool.deposits {
        w.id(d.owner);
        w.n(d.initial);
        w.opt(d.tag, W::id);
        let a = d.snapshot;
        w.n(a.p);
        w.acc(a.s);
        w.acc(a.g);
        w.u64(a.epoch);
        w.u64(a.scale)
    }
    w.len(s.pool.front_ends.len());
    for f in &s.pool.front_ends {
        w.id(f.owner);
        w.n(f.kickback);
        w.n(f.initial);
        let a = f.snapshot;
        w.n(a.p);
        w.acc(a.g);
        w.u64(a.epoch);
        w.u64(a.scale)
    }
    let st = &s.staking;
    for n in [st.total, st.custody_z, st.custody_a, st.zero_z, st.zero_a] {
        w.n(n)
    }
    w.acc(st.f_z);
    w.acc(st.f_a);
    w.len(st.positions.len());
    for p in &st.positions {
        w.id(p.owner);
        w.n(p.stake);
        w.acc(p.snapshot_z);
        w.acc(p.snapshot_a)
    }
    sheet(&mut w, &s.ledger);
    w.n(s.fee.base);
    w.u64(s.fee.last_epoch);
    w.len(s.params.len());
    for p in &s.params {
        params(&mut w, *p)
    }
    w.len(s.nonces.len());
    for (id, n) in &s.nonces {
        w.id(*id);
        w.u64(*n)
    }
    w.len(s.seen.len());
    for id in &s.seen {
        w.id(*id)
    }
    w.len(s.events.len());
    for e in &s.events {
        w.id(e.id);
        w.u64(e.epoch);
        w.u8(match e.class {
            ExposureClass::Liquidation => 0,
            ExposureClass::Redemption => 1,
        });
        w.n(e.amount)
    }
    w.blob(&s.drip_state);
    w.opt(s.last_trigger, W::u64);
    w.0
}
pub(super) fn bundle(
    pre: Id,
    r: &Request,
    post: &State,
    legs: &[ledger::Movement],
    trace: &[u8],
    usage: crate::Usage,
) -> Vec<u8> {
    let mut w = W::new(b"zusd.step4.commit/1\0");
    w.id(pre);
    w.blob(&request(r));
    w.blob(&state(post));
    w.id(post.root());
    w.len(legs.len());
    for l in legs {
        w.opt(l.from, W::id);
        w.opt(l.to, W::id);
        quantity(&mut w, l.quantity)
    }
    w.blob(trace);
    w.u64(usage.steps());
    w.u64(usage.reads());
    w.u64(usage.bytes());
    let id = hash(&w.0);
    w.id(id);
    w.0
}
// Replay decodes ONLY requests, then re-executes the core. Persisted successor
// bytes never become an unchecked State constructor.
#[cfg(any(test, feature = "std"))]
pub(super) struct R<'a> {
    b: &'a [u8],
    at: usize,
    cap: usize,
}
#[cfg(any(test, feature = "std"))]
impl<'a> R<'a> {
    pub fn new(b: &'a [u8], label: &[u8], cap: usize) -> Result<Self, Failure> {
        if b.len() > cap || !b.starts_with(label) {
            return Err(Failure::Canonical);
        }
        Ok(Self {
            b,
            at: label.len(),
            cap,
        })
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        let end = self.at.checked_add(n).ok_or(Failure::Canonical)?;
        let out = self.b.get(self.at..end).ok_or(Failure::Canonical)?;
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, Failure> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, Failure> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Failure::Canonical),
        }
    }
    fn u32(&mut self) -> Result<u32, Failure> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| Failure::Canonical)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, Failure> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| Failure::Canonical)?,
        ))
    }
    fn n(&mut self) -> Result<u128, Failure> {
        Ok(u128::from_be_bytes(
            self.take(16)?.try_into().map_err(|_| Failure::Canonical)?,
        ))
    }
    pub fn id(&mut self) -> Result<Id, Failure> {
        self.take(32)?.try_into().map_err(|_| Failure::Canonical)
    }
    pub(super) fn count(&mut self) -> Result<usize, Failure> {
        let n = usize::try_from(self.u64()?).map_err(|_| Failure::Capacity)?;
        if n > self.cap || n > self.b.len().saturating_sub(self.at) {
            return Err(Failure::Capacity);
        }
        Ok(n)
    }
    pub fn blob(&mut self) -> Result<&'a [u8], Failure> {
        let n = self.count()?;
        self.take(n)
    }
    fn opt<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Failure>,
    ) -> Result<Option<T>, Failure> {
        if self.bool()? {
            Ok(Some(f(self)?))
        } else {
            Ok(None)
        }
    }
    pub fn end(&self) -> Result<(), Failure> {
        if self.at == self.b.len() {
            Ok(())
        } else {
            Err(Failure::Canonical)
        }
    }
    fn clock(&mut self) -> Result<Clock, Failure> {
        Ok(Clock {
            backend: self.id()?,
            finality: self.id()?,
            sync: self.id()?,
            epoch: self.u64()?,
            watermark: self.u64()?,
        })
    }
    fn backing(&mut self) -> Result<Backing, Failure> {
        Ok(Backing {
            custody: self.id()?,
            funding: self.id()?,
            supply_root: self.id()?,
            agrs: self.n()?,
            zdex: self.n()?,
            zdex_supply: self.n()?,
            zdex_burned: self.n()?,
        })
    }
    fn delta(&mut self) -> Result<vaults::Delta, Failure> {
        Ok(match self.u8()? {
            0 => vaults::Delta::None,
            1 => vaults::Delta::Add(self.n()?),
            2 => vaults::Delta::Remove(self.n()?),
            _ => return Err(Failure::Canonical),
        })
    }
    fn quantity(&mut self) -> Result<ledger::Quantity, Failure> {
        let tag = self.u8()?;
        let n = self.n()?;
        Ok(match tag {
            0 => ledger::Quantity::Zusd(n),
            1 => ledger::Quantity::Agrs(n),
            2 => ledger::Quantity::Zdex(n),
            _ => return Err(Failure::Canonical),
        })
    }
    fn record(&mut self) -> Result<PriceRecord, Failure> {
        Ok(PriceRecord {
            aggregate: self.id()?,
            round: self.u64()?,
            published_epoch: self.u64()?,
            earliest_commit_epoch: self.u64()?,
            dispute_until: self.u64()?,
            value_e8: self.n()?,
            observed_epoch: self.u64()?,
            median_seconds: self.u64()?,
            committed_epoch: self.u64()?,
            deviation: self.n()?,
        })
    }
    fn view(&mut self) -> Result<PriceView, Failure> {
        Ok(PriceView {
            committed: self.opt(Self::record)?,
            pending: self.opt(Self::record)?,
            pending_blocked: self.bool()?,
            status: match self.u8()? {
                0 => vaults::PriceStatus::Usable,
                1 => vaults::PriceStatus::Unavailable,
                2 => vaults::PriceStatus::Stale,
                3 => vaults::PriceStatus::Disputed,
                4 => vaults::PriceStatus::Uncertain,
                5 => vaults::PriceStatus::OverBudget,
                _ => return Err(Failure::Canonical),
            },
            capacity: self.n()?,
            capacity_root: self.id()?,
            aggregate_bytes: self.blob()?.to_vec(),
        })
    }
    fn oracle(&mut self) -> Result<OracleReceipt, Failure> {
        Ok(OracleReceipt {
            before: self.id()?,
            after: self.id()?,
            event: self.opt(Self::id)?,
            action: match self.u8()? {
                0 => OracleAction::Use,
                1 => OracleAction::ImportPending,
                2 => OracleAction::CommitMatured,
                3 => OracleAction::InitializeFirstMatured,
                4 => OracleAction::RefreshStatus,
                _ => return Err(Failure::Canonical),
            },
            view: self.view()?,
        })
    }
    fn pool(&mut self) -> Result<stability::Command, Failure> {
        Ok(match self.u8()? {
            0 => stability::Command::Deposit {
                amount: self.n()?,
                tag: self.opt(Self::id)?,
            },
            1 => stability::Command::Withdraw(self.n()?),
            2 => stability::Command::Claim,
            3 => stability::Command::MoveGainToVault,
            4 => stability::Command::RegisterFrontEnd(self.n()?),
            _ => return Err(Failure::Canonical),
        })
    }
    fn stake(&mut self) -> Result<staking::Command, Failure> {
        Ok(match self.u8()? {
            0 => staking::Command::Stake(self.n()?),
            1 => staking::Command::Unstake(self.n()?),
            2 => staking::Command::Claim,
            _ => return Err(Failure::Canonical),
        })
    }
    fn ids(&mut self) -> Result<Vec<Id>, Failure> {
        let n = self.count()?;
        if n > self.b.len().saturating_sub(self.at) / 32 {
            return Err(Failure::Canonical);
        }
        (0..n).map(|_| self.id()).collect()
    }
    fn liquidation(&mut self) -> Result<liquidation::Command, Failure> {
        Ok(match self.u8()? {
            0 => liquidation::Command::Direct(self.id()?),
            1 => liquidation::Command::Lowest(self.u32()?),
            2 => liquidation::Command::Batch(self.ids()?),
            _ => return Err(Failure::Canonical),
        })
    }
    fn redemption(&mut self) -> Result<crate::redemption::Command, Failure> {
        Ok(crate::redemption::Command {
            maximum: self.n()?,
            visits: self.u32()?,
            first: self.opt(Self::id)?,
            partial_nicr: self.n()?,
            upper: self.opt(Self::id)?,
            lower: self.opt(Self::id)?,
            max_fee: self.n()?,
            min_out: self.n()?,
        })
    }
    fn command(&mut self) -> Result<Command, Failure> {
        Ok(match self.u8()? {
            0 => Command::Open {
                collateral: self.n()?,
                principal: self.n()?,
                max_fee: self.n()?,
                upper: self.opt(Self::id)?,
                lower: self.opt(Self::id)?,
            },
            1 => Command::Adjust {
                collateral: self.delta()?,
                debt: self.delta()?,
                max_fee: self.n()?,
                upper: self.opt(Self::id)?,
                lower: self.opt(Self::id)?,
            },
            2 => Command::Close,
            3 => Command::ClaimSurplus,
            4 => Command::Pool(self.pool()?),
            5 => Command::MoveGain {
                upper: self.opt(Self::id)?,
                lower: self.opt(Self::id)?,
            },
            6 => Command::Stake(self.stake()?),
            7 => Command::Liquidate(self.liquidation()?),
            8 => Command::Redeem(self.redemption()?),
            9 => Command::ObserveOracle,
            10 => Command::AdvanceClock,
            11 => Command::Fund {
                event: self.id()?,
                recipient: self.id()?,
                quantity: self.quantity()?,
            },
            12 => Command::ChangeParameters {
                policy: self.id()?,
                version: self.u64()?,
                minimum_net: self.n()?,
                reserve: self.n()?,
            },
            13 => Command::ReleaseExposure {
                events: self.ids()?,
            },
            _ => return Err(Failure::Canonical),
        })
    }
}
#[cfg(any(test, feature = "std"))]
pub(super) fn decode_request(b: &[u8], cap: usize) -> Result<Request, Failure> {
    let mut r = R::new(b, b"zusd.step4.signed/1\0", cap)?;
    let data = r.blob()?;
    let mut d = R::new(data, b"zusd.step4.request/1\0", cap)?;
    let mut out = Request {
        actor: d.id()?,
        nonce: d.u64()?,
        command: d.command()?,
        clock: d.clock()?,
        oracle: d.opt(R::oracle)?,
        backing: d.backing()?,
        allocation: d.opt(|r| {
            Ok(Allocation {
                root: r.id()?,
                capacity: r.n()?,
            })
        })?,
        drip: d.opt(|r| {
            Ok(Drip {
                event: r.id()?,
                amount: r.n()?,
                next: r.blob()?.to_vec(),
            })
        })?,
        proofs: Vec::new(),
    };
    d.end()?;
    let n = r.count()?;
    if n > 9 {
        return Err(Failure::Canonical);
    }
    for _ in 0..n {
        let domain = match r.u8()? {
            0 => Domain::Actor,
            1 => Domain::Clock,
            2 => Domain::Oracle,
            3 => Domain::Custody,
            4 => Domain::Allocation,
            5 => Domain::Drip,
            6 => Domain::Funding,
            7 => Domain::ParameterPolicy,
            8 => Domain::ExposureRelease,
            _ => return Err(Failure::Canonical),
        };
        out.proofs.push(Proof {
            domain,
            bytes: r.blob()?.to_vec(),
        })
    }
    r.end()?;
    if request(&out) != b {
        return Err(Failure::Canonical);
    }
    Ok(out)
}
#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn request_transport_roundtrip_all_tags_rejects_prefixes_lengths_and_trailing_data() {
        let id = [1; 32];
        let commands = alloc::vec![
            Command::Open {
                collateral: 1,
                principal: 2,
                max_fee: 3,
                upper: Some(id),
                lower: None
            },
            Command::Adjust {
                collateral: vaults::Delta::Add(1),
                debt: vaults::Delta::Remove(2),
                max_fee: 3,
                upper: None,
                lower: Some(id)
            },
            Command::Close,
            Command::ClaimSurplus,
            Command::Pool(stability::Command::Deposit {
                amount: 1,
                tag: Some(id)
            }),
            Command::Pool(stability::Command::Withdraw(2)),
            Command::Pool(stability::Command::Claim),
            Command::Pool(stability::Command::RegisterFrontEnd(3)),
            Command::MoveGain {
                upper: None,
                lower: Some(id)
            },
            Command::Stake(staking::Command::Stake(1)),
            Command::Stake(staking::Command::Unstake(2)),
            Command::Stake(staking::Command::Claim),
            Command::Liquidate(liquidation::Command::Direct(id)),
            Command::Liquidate(liquidation::Command::Lowest(1)),
            Command::Liquidate(liquidation::Command::Batch(alloc::vec![id])),
            Command::Redeem(crate::redemption::Command {
                maximum: 1,
                visits: 2,
                first: Some(id),
                partial_nicr: 3,
                upper: Some(id),
                lower: None,
                max_fee: 4,
                min_out: 5
            }),
            Command::ObserveOracle,
            Command::AdvanceClock,
            Command::Fund {
                event: id,
                recipient: id,
                quantity: ledger::Quantity::Agrs(1)
            },
            Command::ChangeParameters {
                policy: id,
                version: 2,
                minimum_net: 3,
                reserve: 4
            },
            Command::ReleaseExposure {
                events: alloc::vec![id]
            }
        ];
        let mut prefixes = 0;
        for command in commands {
            let r = Request {
                actor: id,
                nonce: 3,
                command,
                clock: Clock {
                    backend: id,
                    finality: id,
                    sync: id,
                    epoch: 4,
                    watermark: 5,
                },
                oracle: None,
                backing: Backing {
                    custody: id,
                    funding: id,
                    supply_root: id,
                    agrs: 6,
                    zdex: 7,
                    zdex_supply: 8,
                    zdex_burned: 9,
                },
                allocation: None,
                drip: None,
                proofs: alloc::vec![Proof {
                    domain: Domain::Actor,
                    bytes: alloc::vec![1, 2, 3]
                }],
            };
            let bytes = request(&r);
            assert_eq!(decode_request(&bytes, 4096), Ok(r));
            for i in 0..bytes.len() {
                assert!(decode_request(&bytes[..i], 4096).is_err());
                prefixes += 1;
            }
            let mut extra = bytes.clone();
            extra.push(0);
            assert!(decode_request(&extra, 4096).is_err());
            let mut overflow = bytes;
            let n = b"zusd.step4.signed/1\0".len();
            overflow[n..n + 8].copy_from_slice(&u64::MAX.to_be_bytes());
            assert!(decode_request(&overflow, 4096).is_err());
        }
        assert!(prefixes > 5000);
    }
}
