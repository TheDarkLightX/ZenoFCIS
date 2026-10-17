//! Arithmetic characterization only. These functions grant no decision or publication authority.
//! The proof and native characterization use the unchanged production wide primitives.
use crate::wide;
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::div_mod::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const AMOUNT_LIMIT: u128 = 3_000_000_000;
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const BPS: u128 = 10_000;

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn ceil(p: int, d: int) -> int {
    p / d + if p % d == 0 { 0int } else { 1int }
}
pub proof fn floor_interval(p: int, d: int)
    requires p >= 0, d > 0,
    ensures 0 <= p / d, (p / d) * d <= p < (p / d + 1) * d,
{
    lemma_fundamental_div_mod(p, d);
    lemma_mod_bound(p, d);
    lemma_div_is_ordered(0, p, d);
    assert((p/d)*d == d*(p/d)) by(nonlinear_arith);
    assert((p / d) * d <= p < (p / d + 1) * d) by(nonlinear_arith)
        requires p == (p / d) * d + p % d, 0 <= p % d < d;
}
pub proof fn ceil_interval(p: int, d: int)
    requires p >= 0, d > 0,
    ensures 0 <= ceil(p,d), 0 <= ceil(p,d)*d-p < d,
        ceil(p,d) > 0 ==> (ceil(p,d)-1)*d < p,
{
    floor_interval(p,d);
    lemma_fundamental_div_mod(p,d);
    lemma_mod_bound(p,d);
    assert((p/d)*d == d*(p/d)) by(nonlinear_arith);
    if p % d == 0 {
        assert(ceil(p,d)*d == p);
    } else {
        assert(0 < p % d < d);
        assert(0 <= ceil(p,d)*d-p < d) by(nonlinear_arith)
            requires p == (p/d)*d + p%d, 0 < p%d < d, ceil(p,d) == p/d+1;
    }
    assert(ceil(p,d) > 0 ==> (ceil(p,d)-1)*d < p) by(nonlinear_arith)
        requires 0 <= ceil(p,d)*d-p < d;
}
pub proof fn fee_bound(gross: u128, rate: u128)
    requires rate <= BPS,
    ensures 0 <= ceil((gross as int) * (rate as int), (BPS as int)) <= gross,
{
    let p = (gross as int) * (rate as int);
    assert(0 <= p <= (gross as int) * (BPS as int)) by(nonlinear_arith)
        requires p==(gross as int)*(rate as int),0<=gross,0<=rate<=BPS;
    ceil_interval(p,(BPS as int));
    if ceil(p,(BPS as int)) > gross {
        assert((ceil(p,(BPS as int))-1)*(BPS as int) >= (gross as int) * (BPS as int))
            by(nonlinear_arith) requires ceil(p,BPS as int)>gross,BPS>0;
        assert(false);
    }
}
/// Monotonic fee in either argument, and monotonic net in gross at a fixed rate.
pub proof fn fee_monotonic(g1: u128, g2: u128, r1: u128, r2: u128)
    requires g1 <= g2, r1 <= r2 <= BPS,
    ensures ceil((g1 as int)*(r1 as int),(BPS as int)) <= ceil((g2 as int)*(r2 as int),(BPS as int)),
        (g1 as int)-ceil((g1 as int)*(r1 as int),(BPS as int))
            <= (g2 as int)-ceil((g2 as int)*(r1 as int),(BPS as int)),
{
    let p1=(g1 as int)*(r1 as int);
    let p2=(g2 as int)*(r2 as int);
    let p3=(g2 as int)*(r1 as int);
    assert(0 <= p1 <= p2) by(nonlinear_arith)
        requires p1==(g1 as int)*(r1 as int),p2==(g2 as int)*(r2 as int),0<=g1<=g2,0<=r1<=r2;
    assert(p3-p1 <= ((g2 as int)-(g1 as int))*(BPS as int)) by(nonlinear_arith)
        requires p3==(g2 as int)*(r1 as int),p1==(g1 as int)*(r1 as int),0<=g1<=g2,0<=r1<=BPS;
    assert(p3>=0) by(nonlinear_arith) requires p3==(g2 as int)*(r1 as int),0<=g2,0<=r1;
    ceil_interval(p1,(BPS as int)); ceil_interval(p2,(BPS as int)); ceil_interval(p3,(BPS as int));
    if ceil(p1,(BPS as int))>ceil(p2,(BPS as int)) {
        assert((ceil(p1,(BPS as int))-1)*(BPS as int) >= ceil(p2,(BPS as int))*(BPS as int))
            by(nonlinear_arith) requires ceil(p1,BPS as int)>ceil(p2,BPS as int),BPS>0;
        assert(false);
    }
    if ceil(p3,(BPS as int))>ceil(p1,(BPS as int))+((g2 as int)-(g1 as int)) {
        assert((ceil(p3,(BPS as int))-1)*(BPS as int) >=
            ceil(p1,(BPS as int))*(BPS as int)+((g2 as int)-(g1 as int))*(BPS as int))
            by(nonlinear_arith) requires ceil(p3,BPS as int)>ceil(p1,BPS as int)+(g2 as int)-(g1 as int),BPS>0;
        assert(false);
    }
}
}

/// Total full-u128 fee split; only a rate above 10000 refuses.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some((fee,net)) => rate <= BPS && fee <= gross
            && (fee as int)+(net as int) == (gross as int)
            && (fee as int) == ceil((gross as int)*(rate as int),(BPS as int))
            && 0 <= (fee as int)*(BPS as int)-(gross as int)*(rate as int) < BPS,
        None => rate > BPS,
    },
))]
pub fn fee_split(gross: u128, rate: u128) -> Option<(u128, u128)> {
    if rate > BPS {
        return None;
    }
    #[cfg(verus_keep_ghost)]
    proof! { fee_bound(gross,rate); ceil_interval((gross as int)*(rate as int),(BPS as int)); }
    let fee = wide::mul_div_ceil(gross, rate, BPS)?;
    let net = gross.checked_sub(fee)?;
    Some((fee, net))
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SwapQuote {
    pub fee: u128,
    pub net: u128,
    pub output: u128,
    pub input_reserve: u128,
    pub output_reserve: u128,
}

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn swap_domain(gross:u128,rate:u128,rin:u128,rout:u128)->bool {
    gross <= AMOUNT_LIMIT && rate <= BPS && 0 < rin <= AMOUNT_LIMIT && 0 < rout <= AMOUNT_LIMIT
}
pub proof fn output_bound(rin:u128,rout:u128,net:u128)
    requires 0 < rin <= AMOUNT_LIMIT, 0 < rout <= AMOUNT_LIMIT, net <= AMOUNT_LIMIT,
    ensures 0 <= ((rout as int)*(net as int))/((rin as int)+(net as int)) < rout,
        ((rin as int)+(net as int))*((rout as int)-((rout as int)*(net as int))/((rin as int)+(net as int)))
            >= (rin as int)*(rout as int),
{
    let d=(rin as int)+(net as int);
    let p=(rout as int)*(net as int);
    assert(p>=0) by(nonlinear_arith) requires p==(rout as int)*(net as int),0<=rout,0<=net;
    floor_interval(p,d);
    let out=p/d;
    assert(p < (rout as int)*d) by(nonlinear_arith) requires p==(rout as int)*(net as int),d==(rin as int)+(net as int),rin>0,rout>0;
    if out>=rout {
        assert(out*d >= (rout as int)*d) by(nonlinear_arith) requires out>=rout,d>0;
        assert(false);
    }
    assert(d*((rout as int)-out) >= (rin as int)*(rout as int)) by(nonlinear_arith)
        requires out*d <= (rout as int)*(net as int), d==(rin as int)+(net as int);
}
}

/// The reserve update is defined on the complete stated domain, including zero net/output.
/// Business acceptance (zero amount, dust, pause, slippage, reserve cap) is a separate check.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(q) => swap_domain(gross,rate,rin,rout) && q.fee <= gross
            && (q.fee as int)+(q.net as int) == (gross as int)
            && (q.fee as int) == ceil((gross as int)*(rate as int),(BPS as int))
            && (q.output as int) == (rout as int)*(q.net as int)/((rin as int)+(q.net as int))
            && q.output < rout && (q.input_reserve as int) == (rin as int)+(q.net as int)
            && (q.output_reserve as int) == (rout as int)-(q.output as int)
            && q.input_reserve <= 2*AMOUNT_LIMIT && q.output_reserve <= AMOUNT_LIMIT
            && (q.input_reserve as int)*(q.output_reserve as int) >= (rin as int)*(rout as int),
        None => !swap_domain(gross,rate,rin,rout),
    },
))]
pub fn exact_in(gross: u128, rate: u128, rin: u128, rout: u128) -> Option<SwapQuote> {
    if gross > AMOUNT_LIMIT
        || rate > BPS
        || rin == 0
        || rin > AMOUNT_LIMIT
        || rout == 0
        || rout > AMOUNT_LIMIT
    {
        return None;
    }
    let (fee, net) = fee_split(gross, rate)?;
    let input_reserve = rin.checked_add(net)?;
    #[cfg(verus_keep_ghost)]
    proof! { output_bound(rin,rout,net); }
    let output = wide::mul_div_floor(rout, net, input_reserve)?;
    let output_reserve = rout.checked_sub(output)?;
    Some(SwapQuote {
        fee,
        net,
        output,
        input_reserve,
        output_reserve,
    })
}

#[cfg(verus_keep_ghost)]
verus! {
pub proof fn burn_bound(lp:u128,reserve:u128,supply:u128)
    requires 0 < supply, lp <= supply,
    ensures 0 <= (lp as int)*(reserve as int)/(supply as int) <= reserve,
        ((lp as int)*(reserve as int)/(supply as int))*(supply as int) <= (lp as int)*(reserve as int)
            < ((lp as int)*(reserve as int)/(supply as int)+1)*(supply as int),
{
    let p=(lp as int)*(reserve as int);
    assert(0 <= p <= (supply as int)*(reserve as int)) by(nonlinear_arith) requires p==(lp as int)*(reserve as int),0<=lp<=supply,0<=reserve;
    floor_interval(p,(supply as int));
    if p/(supply as int) > reserve {
        assert((p/(supply as int))*(supply as int) > (reserve as int)*(supply as int)) by(nonlinear_arith) requires p/(supply as int)>reserve,supply>0;
        assert((reserve as int)*(supply as int)==(supply as int)*(reserve as int)) by(nonlinear_arith);
        assert(false);
    }
}
}

/// Floor redemption at one price snapshot, including full-width fitting products.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(out) => supply>0 && lp<=supply && out<=reserve
            && (out as int) == (lp as int)*(reserve as int)/(supply as int)
            && (out as int)*(supply as int) <= (lp as int)*(reserve as int)
                < ((out as int)+1)*(supply as int),
        None => supply==0 || lp>supply,
    },
))]
pub fn lp_burn(lp: u128, reserve: u128, supply: u128) -> Option<u128> {
    if supply == 0 || lp > supply {
        return None;
    }
    #[cfg(verus_keep_ghost)]
    proof! { burn_bound(lp,reserve,supply); }
    wide::mul_div_floor(lp, reserve, supply)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == if a<b {a} else {b},))]
pub fn minimum(a: u128, b: u128) -> u128 {
    if a < b { a } else { b }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == if a<b {b} else {a},))]
pub fn maximum(a: u128, b: u128) -> u128 {
    if a < b { b } else { a }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result<=a,
    (result as int) == (if a>b {(a as int)} else {(b as int)})-(b as int),))]
pub fn saturated_subtraction(a: u128, b: u128) -> u128 {
    maximum(a, b) - b
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    pub lower: u128,
    pub upper: u128,
}

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn valid(r:Range)->bool {r.lower<=r.upper}
pub open spec fn contains(r:Range,x:u128)->bool {r.lower<=x<=r.upper}
pub open spec fn min(a:u128,b:u128)->u128 {if a<b {a}else{b}}
pub open spec fn max(a:u128,b:u128)->u128 {if a<b {b}else{a}}
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires valid(a),valid(b),
    ensures valid(result),result.lower==min(a.lower,b.lower),result.upper==min(a.upper,b.upper),
))]
pub fn min_bounds(a: Range, b: Range) -> Range {
    Range {
        lower: minimum(a.lower, b.lower),
        upper: minimum(a.upper, b.upper),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires valid(a),valid(b),
    ensures valid(result),result.lower==max(a.lower,b.lower),result.upper==max(a.upper,b.upper),
))]
pub fn max_bounds(a: Range, b: Range) -> Range {
    Range {
        lower: maximum(a.lower, b.lower),
        upper: maximum(a.upper, b.upper),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires valid(a),valid(b),
    ensures valid(result),result.lower==min(a.lower,b.lower),result.upper==max(a.upper,b.upper),
))]
pub fn select_bounds(a: Range, b: Range) -> Range {
    Range {
        lower: minimum(a.lower, b.lower),
        upper: maximum(a.upper, b.upper),
    }
}

/// Operand identities, not labels, determine a recognized Lt/Select minimum.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (left==less_left && right==less_right && yes==left && no==right),
))]
pub fn is_minimum(
    left: u16,
    right: u16,
    less_left: u16,
    less_right: u16,
    yes: u16,
    no: u16,
) -> bool {
    left == less_left && right == less_right && yes == left && no == right
}

#[cfg(verus_keep_ghost)]
verus! {
pub proof fn selection_bounds_sound(a:Range,b:Range,x:u128,y:u128,c:bool)
    requires valid(a),valid(b),contains(a,x),contains(b,y),
    ensures min(a.lower,b.lower)<=min(x,y)<=min(a.upper,b.upper),
        max(a.lower,b.lower)<=max(x,y)<=max(a.upper,b.upper),
        min(a.lower,b.lower)<=(if c{x}else{y})<=max(a.upper,b.upper),
        0 <= max(x,y) as int-(y as int) <= x,
{}
}
