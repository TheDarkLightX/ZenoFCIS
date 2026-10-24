//! Abstract integer arithmetic, independent of limb representation.
//! Rounding is a parameter; these statements select no financial policy.
use crate::{scalar, wide};
use vstd::arithmetic::{div_mod::*, mul::*};
use vstd::prelude::*;

verus! {

pub open spec fn signed_fit(n: int) -> bool { i128::MIN <= n <= i128::MAX }
pub open spec fn unsigned_fit(n: int) -> bool { 0 <= n <= u128::MAX }
pub open spec fn checked_signed(n: int) -> Option<i128> {
    if signed_fit(n) { Some(n as i128) } else { None }
}
pub open spec fn checked_unsigned(n: int) -> Option<u128> {
    if unsigned_fit(n) { Some(n as u128) } else { None }
}
/// Mathematical floor for either denominator sign (Verus / is Euclidean).
pub open spec fn floor(n: int, d: int) -> int {
    if d > 0 { n / d } else { (-n) / (-d) }
}
pub open spec fn ceiling(n: int, d: int) -> int { -floor(-n, d) }
pub open spec fn rounded(up: bool, n: int, d: int) -> int {
    if up { ceiling(n, d) } else { floor(n, d) }
}
pub open spec fn signed_division(up: bool, a: int, b: int) -> Option<i128> {
    if !signed_fit(a) || !signed_fit(b) || b == 0 { None }
    else { checked_signed(rounded(up, a, b)) }
}
pub open spec fn signed_fused(up: bool, a: int, b: int, d: int) -> Option<i128> {
    if !signed_fit(a) || !signed_fit(b) || !signed_fit(d) || a < 0 || b < 0 || d <= 0 { None }
    else { checked_signed(rounded(up, a * b, d)) }
}
pub open spec fn signed_full_fused(a: int, b: int, d: int) -> Option<i128> {
    if !signed_fit(a) || !signed_fit(b) || !signed_fit(d) || d <= 0 { None }
    else { checked_signed(floor(a * b, d)) }
}
pub open spec fn unsigned_fused(up: bool, a: int, b: int, d: int) -> Option<u128> {
    if !unsigned_fit(a) || !unsigned_fit(b) || !unsigned_fit(d) || d == 0 { None }
    else { checked_unsigned(rounded(up, a * b, d)) }
}

/// Characterize both directions by the same unique quotient and remainder.
pub proof fn rounding_interval(n: int, d: int)
    requires d > 0,
    ensures
        n == floor(n,d) * d + n % d, 0 <= n % d < d,
        floor(n,d) * d <= n < (floor(n,d) + 1) * d,
        (ceiling(n,d) - 1) * d < n <= ceiling(n,d) * d,
        floor(n,d) <= ceiling(n,d) <= floor(n,d) + 1,
        ceiling(n,d) == floor(n,d) + if n % d == 0 { 0int } else { 1int },
{
    lemma_fundamental_div_mod(n,d);
    lemma_mod_bound(n,d);
    let q = n/d;
    let r = n%d;
    assert(n == q*d+r) by(nonlinear_arith) requires n == d*q+r;
    let c = q + if r == 0 { 0int } else { 1int };
    let neg_r = if r == 0 { 0int } else { d-r };
    assert(-n == (-c)*d + neg_r) by(nonlinear_arith)
        requires n == q*d+r, c == q + if r==0 {0int} else {1int},
            neg_r == if r==0 {0int} else {d-r};
    lemma_fundamental_div_mod_converse_div(-n,d,-c,neg_r);
    assert(q*d <= n < (q+1)*d) by(nonlinear_arith)
        requires n==q*d+r, 0<=r<d;
    assert((c-1)*d < n <= c*d) by(nonlinear_arith)
        requires n==q*d+r, 0<=r<d, c==q+if r==0 {0int}else{1int};
}

pub(crate) proof fn unsigned_correspondence(a:u128,b:u128,d:u128)
    ensures wide::floor_value(a,b,d) == unsigned_fused(false,a as int,b as int,d as int),
        wide::ceiling_value(a,b,d) == unsigned_fused(true,a as int,b as int,d as int),
{
    if d>0 {
        rounding_interval(a as int*b as int,d as int);
        lemma_mul_nonnegative(a as int,b as int);
        lemma_div_pos_is_pos(a as int*b as int,d as int);
    }
}

pub(crate) proof fn signed_fused_correspondence(up:bool,a:i128,b:i128,d:i128)
    ensures scalar::mul_div_value(up,a,b,d) == signed_fused(up,a as int,b as int,d as int),
        scalar::signed_mul_div_floor_value(a,b,d) == signed_full_fused(a as int,b as int,d as int),
{
    if d>0 { rounding_interval(a as int*b as int,d as int); }
}

pub(crate) proof fn signed_division_correspondence(up:bool,a:i128,b:i128)
    ensures scalar::division_value(up,a,b) == signed_division(up,a as int,b as int),
{
    if b != 0 {
        let x = scalar::magnitude(a as int);
        let y = scalar::magnitude(b as int);
        rounding_interval(x,y);
        rounding_interval(-x,y);
        assert(floor(a as int,b as int) ==
            if (a<0)!=(b<0) { -ceiling(x,y) } else { floor(x,y) });
        assert(ceiling(a as int,b as int) ==
            if (a<0)!=(b<0) { -floor(x,y) } else { ceiling(x,y) });
    }
}

/// Rounding in both directions is monotone in the numerator.
pub proof fn numerator_monotone(n:int,m:int,d:int)
    requires d>0, n<=m,
    ensures floor(n,d)<=floor(m,d), ceiling(n,d)<=ceiling(m,d),
{
    lemma_div_is_ordered(n,m,d);
    lemma_div_is_ordered(-m,-n,d);
}

/// Each multiplicative argument is monotone when the other is nonnegative.
pub proof fn factors_monotone(a:int,b:int,c:int,d:int)
    requires d>0, a<=b, 0<=c,
    ensures floor(a*c,d)<=floor(b*c,d), ceiling(a*c,d)<=ceiling(b*c,d),
        floor(c*a,d)<=floor(c*b,d), ceiling(c*a,d)<=ceiling(c*b,d),
{
    assert(a*c<=b*c) by(nonlinear_arith) requires a<=b, 0<=c;
    numerator_monotone(a*c,b*c,d);
    assert(c*a==a*c && c*b==b*c) by(nonlinear_arith);
}

pub proof fn denominator_monotone(n:int,d:int,e:int)
    requires 0<d<=e,
    ensures
        n>=0 ==> floor(n,e)<=floor(n,d) && ceiling(n,e)<=ceiling(n,d),
        n<=0 ==> floor(n,d)<=floor(n,e) && ceiling(n,d)<=ceiling(n,e),
{
    rounding_interval(n,d); rounding_interval(n,e);
    // Quotient monotonicity is sign dependent; no trade rounding policy is assumed.
    if n>=0 {
        lemma_div_pos_is_pos(n,d); lemma_div_pos_is_pos(n,e);
        assert(floor(n,e)<=floor(n,d)) by(nonlinear_arith)
            requires floor(n,e)*e<=n, n<(floor(n,d)+1)*d,
                floor(n,e)>=0, 0<d<=e;
        assert(ceiling(n,e)<=ceiling(n,d)) by(nonlinear_arith)
            requires n<=ceiling(n,d)*d, (ceiling(n,e)-1)*e<n,
                ceiling(n,d)>=0, 0<d<=e;
    } else {
        assert(floor(n,d)<=floor(n,e)) by(nonlinear_arith)
            requires floor(n,d)*d<=n, n<(floor(n,e)+1)*e,
                floor(n,d)<0, floor(n,e)<0, 0<d<=e;
        assert(ceiling(n,d)<=ceiling(n,e)) by(nonlinear_arith)
            requires n<=ceiling(n,e)*e, (ceiling(n,d)-1)*d<n,
                ceiling(n,d)<=0, ceiling(n,e)<=0, 0<d<=e;
    }
}

/// Non-vacuity, rounding direction and exact refusal boundaries.
pub proof fn witnesses()
    ensures
        signed_division(false,-1,2)==Some(-1i128),
        signed_division(true,-1,2)==Some(0i128),
        signed_division(false,1,-2)==Some(-1i128),
        signed_division(true,1,2)==Some(1i128),
        signed_division(false,1,0)==None::<i128>,
        signed_division(false,i128::MIN as int,-1)==None::<i128>,
        checked_signed(i128::MAX as int+1)==None::<i128>,
        checked_unsigned(-1)==None::<u128>,
        signed_fused(false,-1,1,2)==None::<i128>,
        signed_fused(false,1,1,-1)==None::<i128>,
        signed_fused(false,5,1,2)==Some(2i128),
        signed_fused(true,5,1,2)==Some(3i128),
        signed_full_fused(-5,1,2)==Some(-3i128),
        unsigned_fused(false,u128::MAX as int,u128::MAX as int,u128::MAX as int)==Some(u128::MAX),
        unsigned_fused(false,u128::MAX as int-1,u128::MAX as int-1,u128::MAX as int-2)==Some(u128::MAX),
        unsigned_fused(true,u128::MAX as int-1,u128::MAX as int-1,u128::MAX as int-2)==None::<u128>,
{
    assert(signed_division(false,-1,2)==Some(-1i128)) by(compute);
    assert(signed_division(true,-1,2)==Some(0i128)) by(compute);
    assert(signed_division(false,1,-2)==Some(-1i128)) by(compute);
    assert(signed_division(true,1,2)==Some(1i128)) by(compute);
    assert(unsigned_fused(false,u128::MAX as int,u128::MAX as int,u128::MAX as int)==Some(u128::MAX)) by(compute);
    assert(unsigned_fused(false,u128::MAX as int-1,u128::MAX as int-1,u128::MAX as int-2)==Some(u128::MAX)) by(compute);
    assert(unsigned_fused(true,u128::MAX as int-1,u128::MAX as int-1,u128::MAX as int-2)==None::<u128>) by(compute);
}

pub open spec fn signed_products(a:int,b:int,c:int,d:int,e:int,f:int)->Option<i128>{
    if !signed_fit(a) || !signed_fit(b) || !signed_fit(c) || !signed_fit(d)
        || !signed_fit(e) || !signed_fit(f) || a<0 || b<0 || c<0 || d<0 || e<0 || f<0 {None}
    else {Some(if a*b*c>=d*e*f {1i128}else{0i128})}
}

pub proof fn fused_exactness(a:int,b:int,d:int)
    requires d>0,
    ensures a*b==floor(a*b,d)*d+(a*b)%d, 0<=(a*b)%d<d,
        floor(a*b,d)*d<=a*b<=ceiling(a*b,d)*d,
        0<=ceiling(a*b,d)-floor(a*b,d)<=1,
{ rounding_interval(a*b,d); }

pub proof fn premise_counterexamples()
    ensures
        floor(1,-1)>floor(2,-1),
        floor(2,1)>floor(1,1),
        floor(1*(-1),1)>floor(2*(-1),1),
        floor(4,2)>floor(4,4),
        floor(-4,1)<floor(-4,2),
        !(0<=0int<0int),
        !signed_fit(i128::MAX as int*2),
        !signed_fit((i128::MIN as int)/(-1int)),
        !unsigned_fit(u128::MAX as int*u128::MAX as int),
{
    assert(floor(1,-1)>floor(2,-1)) by(compute);
    assert(floor(4,2)>floor(4,4)) by(compute);
    assert(floor(-4,1)<floor(-4,2)) by(compute);
}

}
