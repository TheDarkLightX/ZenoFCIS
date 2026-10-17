//! A closed bounds analysis for the arithmetic graphs in graphs.rs.
//! All eager nodes are covered; business guards and resource sufficiency are separate.
use crate::spot::{AMOUNT_LIMIT, BPS, Range};
#[cfg(verus_keep_ghost)]
use crate::spot::{ceil, floor_interval};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub amount: Range,
    pub rate: Range,
    pub reserve: Range,
    pub supply: Range,
    pub fee: Range,
    pub net: Range,
    pub denominator: Range,
    pub swap_output: Range,
    pub remaining_reserve: Range,
    pub burn_output: Range,
    pub mint_side: Range,
    pub minted: Range,
}

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn admitted(amount:u128,reserve:u128,supply:u128)->bool {
    amount<=AMOUNT_LIMIT && 0<reserve<=AMOUNT_LIMIT && 0<supply<=AMOUNT_LIMIT
}
pub open spec fn result_bounds(amount:u128,reserve:u128,supply:u128,b:Bounds)->bool {
    b.amount==Range{lower:0,upper:amount} && b.rate==Range{lower:0,upper:BPS}
    && b.reserve==Range{lower:1,upper:reserve} && b.supply==Range{lower:1,upper:supply}
    && b.fee==b.amount && b.net==b.amount
    && b.denominator.lower==1 && (b.denominator.upper as int)==(reserve as int)+(amount as int)
    && b.swap_output.lower==0 && (b.swap_output.upper as int)==(reserve as int)-1
    && b.remaining_reserve==Range{lower:1,upper:reserve}
    && b.burn_output.lower==0 && (b.burn_output.upper as int)==(amount as int)*(reserve as int)
    && b.mint_side.lower==0 && (b.mint_side.upper as int)==(amount as int)*(supply as int)
    && b.minted==b.mint_side
}
}

/// The burn bound also covers eagerly evaluated raw lp > supply rows.
/// LP <= supply is needed separately for payout <= reserve.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(b)=>admitted(amount,reserve,supply)&&result_bounds(amount,reserve,supply,b),
        None=>!admitted(amount,reserve,supply),
    },
))]
pub fn analyse(amount: u128, reserve: u128, supply: u128) -> Option<Bounds> {
    if amount > AMOUNT_LIMIT
        || reserve == 0
        || reserve > AMOUNT_LIMIT
        || supply == 0
        || supply > AMOUNT_LIMIT
    {
        return None;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert((amount as int)*(reserve as int) <= (AMOUNT_LIMIT as int)*(AMOUNT_LIMIT as int))
            by(nonlinear_arith) requires 0<=amount<=AMOUNT_LIMIT,0<=reserve<=AMOUNT_LIMIT;
        assert((amount as int)*(supply as int) <= (AMOUNT_LIMIT as int)*(AMOUNT_LIMIT as int))
            by(nonlinear_arith) requires 0<=amount<=AMOUNT_LIMIT,0<=supply<=AMOUNT_LIMIT;
    }
    let a = Range {
        lower: 0,
        upper: amount,
    };
    let mint_side = Range {
        lower: 0,
        upper: amount * supply,
    };
    Some(Bounds {
        amount: a,
        rate: Range {
            lower: 0,
            upper: BPS,
        },
        reserve: Range {
            lower: 1,
            upper: reserve,
        },
        supply: Range {
            lower: 1,
            upper: supply,
        },
        fee: a,
        net: a,
        denominator: Range {
            lower: 1,
            upper: reserve + amount,
        },
        swap_output: Range {
            lower: 0,
            upper: reserve - 1,
        },
        remaining_reserve: Range {
            lower: 1,
            upper: reserve,
        },
        burn_output: Range {
            lower: 0,
            upper: amount * reserve,
        },
        mint_side,
        minted: mint_side,
    })
}

#[cfg(verus_keep_ghost)]
verus! {
/// Source theorem for every arithmetic node of the closed swap graph, including
/// correlated fee subtraction and reserve subtraction that plain intervals miss.
pub(crate) proof fn swap_nodes_defined(a:u128,r:u128,s:u128,gross:u128,rate:u128,rin:u128,rout:u128,b:Bounds)
    requires admitted(a,r,s),result_bounds(a,r,s,b),
        gross<=a,rate<=BPS,0<rin<=r,0<rout<=r,
    ensures crate::wide::ceiling_value(gross,rate,BPS).is_some(),
        0<=ceil((gross as int)*(rate as int),(BPS as int))<=a,
        0<=(gross as int)-ceil((gross as int)*(rate as int),(BPS as int))<=a,
        1<=(rin as int)+(gross as int)-ceil((gross as int)*(rate as int),(BPS as int))<=b.denominator.upper,
        crate::wide::floor_value(rout,
            ((gross as int)-ceil((gross as int)*(rate as int),(BPS as int))) as u128,
            ((rin as int)+(gross as int)-ceil((gross as int)*(rate as int),(BPS as int))) as u128).is_some(),
        0 <= ((rout as int)*((gross as int)-ceil((gross as int)*(rate as int),(BPS as int))))
            /((rin as int)+(gross as int)-ceil((gross as int)*(rate as int),(BPS as int))) <= b.swap_output.upper,
        1 <= (rout as int)-((rout as int)*((gross as int)-ceil((gross as int)*(rate as int),(BPS as int))))
            /((rin as int)+(gross as int)-ceil((gross as int)*(rate as int),(BPS as int))) <= b.remaining_reserve.upper,
{
    crate::spot::fee_bound(gross,rate);
    let net=((gross as int)-ceil((gross as int)*(rate as int),(BPS as int))) as u128;
    crate::spot::output_bound(rin,rout,net);
}
/// Both independent LP mint arms are defined before min selection. This also
/// proves raw LP burn definedness without borrowing a later business guard.
pub(crate) proof fn liquidity_nodes_defined(a:u128,r:u128,s:u128,x:u128,y:u128,r0:u128,r1:u128,supply:u128,b:Bounds)
    requires admitted(a,r,s),result_bounds(a,r,s,b),x<=a,y<=a,0<r0<=r,0<r1<=r,0<supply<=s,
    ensures crate::wide::floor_value(x,r0,supply).is_some(),
        0 <= (x as int)*(r0 as int)/(supply as int) <= b.burn_output.upper,
        crate::wide::floor_value(x,supply,r0).is_some(),
        crate::wide::floor_value(y,supply,r1).is_some(),
        0 <= (x as int)*(supply as int)/(r0 as int) <= b.mint_side.upper,
        0 <= (y as int)*(supply as int)/(r1 as int) <= b.mint_side.upper,
        0 <= (if (x as int)*(supply as int)/(r0 as int) < (y as int)*(supply as int)/(r1 as int)
            {(x as int)*(supply as int)/(r0 as int)} else {(y as int)*(supply as int)/(r1 as int)}) <= b.minted.upper,
{
    let p=(x as int)*(r0 as int);
    let p0=(x as int)*(supply as int);
    let p1=(y as int)*(supply as int);
    assert(0<=p<=(a as int)*(r as int)) by(nonlinear_arith) requires p==(x as int)*(r0 as int),0<=x<=a,0<=r0<=r;
    assert(0<=p0<=(a as int)*(s as int)) by(nonlinear_arith) requires p0==(x as int)*(supply as int),0<=x<=a,0<=supply<=s;
    assert(0<=p1<=(a as int)*(s as int)) by(nonlinear_arith) requires p1==(y as int)*(supply as int),0<=y<=a,0<=supply<=s;
    floor_interval(p,(supply as int));floor_interval(p0,(r0 as int));floor_interval(p1,(r1 as int));
    assert(p/(supply as int)<=p) by(nonlinear_arith)
        requires supply>0,0<=p,(p/(supply as int))*(supply as int)<=p;
    assert(p0/(r0 as int)<=p0) by(nonlinear_arith)
        requires r0>0,0<=p0,(p0/(r0 as int))*(r0 as int)<=p0;
    assert(p1/(r1 as int)<=p1) by(nonlinear_arith)
        requires r1>0,0<=p1,(p1/(r1 as int))*(r1 as int)<=p1;
}
}
