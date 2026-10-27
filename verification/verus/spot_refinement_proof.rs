//! The Spot Step contracts on the compiled program's accepted decisions.
//!
//! For every input the evaluator specification accepts and the decision
//! table answers with an accepting branch (codes 63..=69), the assigned
//! successor (1) equals the Spot core specification's successor exactly,
//! including every rounding direction, and (2) meets the integer Step
//! contract that NativeBridge.lean states and the Lean round-trip theorems
//! take as premises, for every coalition that contains the acting account.
use crate::evaluation::spec::execution;
use crate::evaluation::{Domain, Failure, Op};
use crate::spot_native_program::*;
use crate::spot_refinement_base::*;
use vstd::arithmetic::div_mod::*;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn den(v: int) -> int { if 1 < v { v } else { 1 } }

pub(crate) open spec fn limb() -> int { 4294967296 }

/// The u128 a key's four 32-bit limbs hold, least significant first.
pub(crate) open spec fn value(st: Seq<int>, key: int) -> int {
    st[8 + 4 * key] + st[9 + 4 * key] * 4294967296
        + st[10 + 4 * key] * 18446744073709551616
        + st[11 + 4 * key] * 79228162514264337593543950336
}

pub(crate) open spec fn limbs_in_range(st: Seq<int>, key: int) -> bool {
    0 <= st[8 + 4 * key] < limb() && 0 <= st[9 + 4 * key] < limb()
        && 0 <= st[10 + 4 * key] < limb() && 0 <= st[11 + 4 * key] < limb()
}

pub(crate) open spec fn pre(input: Seq<i128>) -> Seq<int> {
    Seq::new(40nat, |j: int| input[j] as int)
}

// ---------------------------------------------------------------------------
// Arithmetic of the opaque operators.

pub(crate) proof fn floor_facts(a: int, b: int, d: int)
    requires a >= 0, b >= 0, d > 0,
    ensures mdf(a, b, d) >= 0, mdf(a, b, d) * d <= a * b, a * b < (mdf(a, b, d) + 1) * d,
{
    reveal(mdf);
    let p = a * b;
    assert(p >= 0) by(nonlinear_arith) requires p == a * b, a >= 0, b >= 0;
    lemma_fundamental_div_mod(p, d);
    lemma_mod_bound(p, d);
    lemma_div_pos_is_pos(p, d);
    assert(mdf(a, b, d) * d <= p < (mdf(a, b, d) + 1) * d) by(nonlinear_arith)
        requires p == d * (p / d) + p % d, 0 <= p % d < d, mdf(a, b, d) == p / d;
}

pub(crate) proof fn ceil_facts(a: int, b: int, d: int)
    requires a >= 0, b >= 0, d > 0,
    ensures mdc(a, b, d) >= 0, a * b <= mdc(a, b, d) * d, mdc(a, b, d) * d < a * b + d,
{
    reveal(mdc);
    let p = a * b;
    assert(p >= 0) by(nonlinear_arith) requires p == a * b, a >= 0, b >= 0;
    lemma_fundamental_div_mod(p, d);
    lemma_mod_bound(p, d);
    lemma_div_pos_is_pos(p, d);
    let q = p / d;
    let r = p % d;
    if r == 0 {
        assert(mdc(a, b, d) == q);
        assert(p <= q * d < p + d) by(nonlinear_arith) requires p == d * q + r, r == 0, d > 0;
    } else {
        assert(mdc(a, b, d) == q + 1);
        assert(p <= (q + 1) * d < p + d) by(nonlinear_arith) requires p == d * q + r, 0 < r < d;
    }
}

/// The fee split: 0 <= P <= F <= A for a fee rate and share at most one.
pub(crate) proof fn fee_split(gross: int)
    requires gross >= 0,
    ensures 0 <= mdf(mdc(gross, 30, 10000), 2000, 10000) <= mdc(gross, 30, 10000) <= gross,
{
    ceil_facts(gross, 30, 10000);
    let f = mdc(gross, 30, 10000);
    assert(f <= gross) by(nonlinear_arith) requires f * 10000 < gross * 30 + 10000, f >= 0, gross >= 0;
    floor_facts(f, 2000, 10000);
    let p = mdf(f, 2000, 10000);
    assert(p <= f) by(nonlinear_arith) requires p * 10000 <= f * 2000, p >= 0, f >= 0;
}

/// The pool's product of reserves does not fall: (x+n)(y-q) >= xy whenever
/// q(x+n) <= yn, and the reserves move no less favourably than that.
pub(crate) proof fn swap_product(x: int, y: int, n: int, q: int, paid: int, gain: int)
    requires x > 0, y > 0, n > 0, q >= 0, q * (x + n) <= y * n, 0 < paid <= q, gain >= n,
    ensures (x + gain) * (y - paid) >= x * y, x + gain > 0, y - paid > 0,
{
    assert(q < y) by(nonlinear_arith) requires q * (x + n) <= y * n, x > 0, y > 0, n > 0, q >= 0;
    assert((x + n) * (y - q) >= x * y) by(nonlinear_arith)
        requires q * (x + n) <= y * n;
    assert((x + gain) * (y - paid) >= (x + n) * (y - q)) by(nonlinear_arith)
        requires gain >= n, paid <= q, q < y, x > 0, n > 0;
}

/// Floor pro-rata: mdf(u, s, r) * r <= u * s.
pub(crate) proof fn pro_rata(u: int, s: int, r: int)
    requires u >= 0, s >= 0, r > 0,
    ensures mdf(u, s, r) >= 0, mdf(u, s, r) * r <= u * s,
{
    floor_facts(u, s, r);
}

pub(crate) proof fn products(a: int, b: int, c: int, d: int)
    ensures pge(a, b, 1, c, d, 1) == (a * b >= c * d),
{
    reveal(pge);
    assert(a * b * 1 == a * b) by(nonlinear_arith);
    assert(c * d * 1 == c * d) by(nonlinear_arith);
}

/// One key's four-limb update: the limbs are the base-2^32 digits of
/// pre - debit + credit, which therefore fits and is exact.
pub(crate) proof fn limbs(b0: int, b1: int, b2: int, b3: int, column: int,
    n0: int, n1: int, n2: int, n3: int)
    requires
        0 <= b0 < limb(), 0 <= b1 < limb(), 0 <= b2 < limb(), 0 <= b3 < limb(),
        n0 == column - (column / 4294967296) * 4294967296,
        n1 == (b1 + column / 4294967296) - ((b1 + column / 4294967296) / 4294967296) * 4294967296,
        n2 == (b2 + (b1 + column / 4294967296) / 4294967296)
            - ((b2 + (b1 + column / 4294967296) / 4294967296) / 4294967296) * 4294967296,
        n3 == (b3 + (b2 + (b1 + column / 4294967296) / 4294967296) / 4294967296)
            - ((b3 + (b2 + (b1 + column / 4294967296) / 4294967296) / 4294967296) / 4294967296) * 4294967296,
        (b3 + (b2 + (b1 + column / 4294967296) / 4294967296) / 4294967296) / 4294967296 == 0,
    ensures
        n0 + n1 * 4294967296 + n2 * 18446744073709551616 + n3 * 79228162514264337593543950336
            == column - b0 + (b0 + b1 * 4294967296 + b2 * 18446744073709551616 + b3 * 79228162514264337593543950336),
        0 <= n0 < limb(), 0 <= n1 < limb(), 0 <= n2 < limb(), 0 <= n3 < limb(),
{
    let c0 = column / 4294967296;
    let c1 = (b1 + c0) / 4294967296;
    let c2 = (b2 + c1) / 4294967296;
    lemma_fundamental_div_mod(column, 4294967296);
    lemma_mod_bound(column, 4294967296);
    lemma_fundamental_div_mod(b1 + c0, 4294967296);
    lemma_mod_bound(b1 + c0, 4294967296);
    lemma_fundamental_div_mod(b2 + c1, 4294967296);
    lemma_mod_bound(b2 + c1, 4294967296);
    lemma_fundamental_div_mod(b3 + c2, 4294967296);
    lemma_mod_bound(b3 + c2, 4294967296);
}


// ---------------------------------------------------------------------------
// The Spot core specification (SPOT_CORE_SPEC.md), written over the input.

pub(crate) open spec fn v(input: Seq<i128>, i: int) -> int { input[i] as int }
pub(crate) open spec fn action(input: Seq<i128>) -> int { v(input, 40) }
pub(crate) open spec fn direction(input: Seq<i128>) -> int { v(input, 41) }
pub(crate) open spec fn amount(input: Seq<i128>) -> int { v(input, 42) }
pub(crate) open spec fn amount1(input: Seq<i128>) -> int { v(input, 43) }
pub(crate) open spec fn actor(input: Seq<i128>) -> int { v(input, 47) }
pub(crate) open spec fn recipient(input: Seq<i128>) -> int { v(input, 48) }
pub(crate) open spec fn is_swap(input: Seq<i128>) -> bool { action(input) == 1 || action(input) == 2 }

pub(crate) open spec fn x_in(input: Seq<i128>) -> int { if direction(input) == 0 { v(input, 0) } else { v(input, 1) } }
pub(crate) open spec fn y_out(input: Seq<i128>) -> int { if direction(input) == 0 { v(input, 1) } else { v(input, 0) } }
pub(crate) open spec fn fee(gross: int) -> int { mdc(gross, 30, 10000) }
pub(crate) open spec fn protocol(gross: int) -> int { mdf(fee(gross), 2000, 10000) }
pub(crate) open spec fn net(gross: int) -> int { gross - fee(gross) }
/// Exact-out: t = ceil(xw/(y-w)), A = ceil(t*B/(B-f)).
pub(crate) open spec fn gross(input: Seq<i128>) -> int {
    if action(input) == 2 {
        mdc(mdc(x_in(input), amount(input), den(y_out(input) - amount(input))), 10000, 9970)
    } else { amount(input) }
}
pub(crate) open spec fn quote(input: Seq<i128>) -> int {
    mdf(y_out(input), net(gross(input)), den(x_in(input) + net(gross(input))))
}
pub(crate) open spec fn paid(input: Seq<i128>) -> int {
    if action(input) == 2 { amount(input) } else { quote(input) }
}

pub(crate) open spec fn ratio(input: Seq<i128>) -> bool {
    amount(input) * v(input, 1) <= amount1(input) * v(input, 0)
}
pub(crate) open spec fn used(input: Seq<i128>, i: int) -> int {
    if ratio(input) {
        if i == 0 { amount(input) } else { mdf(amount(input), v(input, 1), den(v(input, 0))) }
    } else {
        if i == 0 { mdf(amount1(input), v(input, 0), den(v(input, 1))) } else { amount1(input) }
    }
}
pub(crate) open spec fn minted(input: Seq<i128>) -> int {
    let m0 = mdf(used(input, 0), v(input, 2), den(v(input, 0)));
    let m1 = mdf(used(input, 1), v(input, 2), den(v(input, 1)));
    if m0 <= m1 { m0 } else { m1 }
}
pub(crate) open spec fn payout(input: Seq<i128>, i: int) -> int {
    mdf(amount(input), v(input, i), den(v(input, 2)))
}

pub(crate) open spec fn post_reserve(input: Seq<i128>, i: int) -> int {
    if action(input) == 0 {
        if i == 0 { amount(input) } else { amount1(input) }
    } else if is_swap(input) {
        if i == direction(input) { x_in(input) + gross(input) - protocol(gross(input)) }
        else { y_out(input) - paid(input) }
    } else if action(input) == 3 { v(input, i) + used(input, i) }
    else { v(input, i) - payout(input, i) }
}
pub(crate) open spec fn post_supply(input: Seq<i128>) -> int {
    if action(input) == 0 { v(input, 46) }
    else if is_swap(input) { v(input, 2) }
    else if action(input) == 3 { v(input, 2) + minted(input) }
    else { v(input, 2) - amount(input) }
}
pub(crate) open spec fn lp_delta(input: Seq<i128>) -> int {
    if action(input) == 0 { v(input, 46) - 1000 }
    else if is_swap(input) { 0 }
    else if action(input) == 3 { minted(input) }
    else { -amount(input) }
}
pub(crate) open spec fn post_shares(input: Seq<i128>, j: int) -> int {
    if j == 2 { v(input, 7) + if action(input) == 0 { 1000int } else { 0int } }
    else { v(input, 5 + j) + if actor(input) == j { lp_delta(input) } else { 0int } }
}
/// T(asset, from, to, q) effects per complete key: pool 0/1, ingress 2/3,
/// alice 4/5, bob 6/7; the asset of key k is k % 2.
pub(crate) open spec fn debit(input: Seq<i128>, key: int) -> int {
    let asset = key % 2;
    if key < 2 {
        if is_swap(input) { if asset == direction(input) { 0 } else { paid(input) } }
        else if action(input) == 4 { payout(input, asset) } else { 0 }
    } else if key < 4 { 0 }
    else if actor(input) == (key - 4) / 2 {
        if action(input) == 0 { if asset == 0 { amount(input) } else { amount1(input) } }
        else if is_swap(input) { if asset == direction(input) { gross(input) } else { 0 } }
        else if action(input) == 3 { used(input, asset) } else { 0 }
    } else { 0 }
}
pub(crate) open spec fn credit(input: Seq<i128>, key: int) -> int {
    let asset = key % 2;
    if key < 2 {
        if action(input) == 0 { if asset == 0 { amount(input) } else { amount1(input) } }
        else if is_swap(input) {
            if asset == direction(input) { gross(input) - protocol(gross(input)) } else { 0 }
        } else if action(input) == 3 { used(input, asset) } else { 0 }
    } else if key < 4 {
        if is_swap(input) && asset == direction(input) { protocol(gross(input)) } else { 0 }
    } else if recipient(input) == (key - 4) / 2 {
        if is_swap(input) { if asset == direction(input) { 0 } else { paid(input) } }
        else if action(input) == 4 { payout(input, asset) } else { 0 }
    } else { 0 }
}

pub(crate) open spec fn key_exact(input: Seq<i128>, b: Seq<int>, key: int) -> bool {
    value(b, key) == value(pre(input), key) - debit(input, key) + credit(input, key)
        && limbs_in_range(b, key)
}

/// The assigned successor is the specification's, field by field and key by key.
pub(crate) open spec fn exact(input: Seq<i128>, b: Seq<int>) -> bool {
    b.len() == 40
    && b[0] == post_reserve(input, 0) && b[1] == post_reserve(input, 1) && b[2] == post_supply(input)
    && b[3] == 1 && b[4] == 0
    && b[5] == post_shares(input, 0) && b[6] == post_shares(input, 1) && b[7] == post_shares(input, 2)
    && key_exact(input, b, 0) && key_exact(input, b, 1) && key_exact(input, b, 2) && key_exact(input, b, 3)
    && key_exact(input, b, 4) && key_exact(input, b, 5) && key_exact(input, b, 6) && key_exact(input, b, 7)
}

/// The specification's acceptance guards that the contract uses.
pub(crate) open spec fn eligible(input: Seq<i128>) -> bool {
    actor(input) < 2 && recipient(input) < 2 && action(input) <= 4 && direction(input) <= 1
    && amount(input) > 0
    && v(input, 8) == v(input, 0) && v(input, 9) == 0 && v(input, 10) == 0 && v(input, 11) == 0
    && v(input, 12) == v(input, 1) && v(input, 13) == 0 && v(input, 14) == 0 && v(input, 15) == 0
    && v(input, 5) + v(input, 6) + v(input, 7) == v(input, 2)
    && ((v(input, 3) == 0 && v(input, 2) == 0) || (v(input, 3) == 1 && v(input, 7) >= 1000))
    && if action(input) == 0 {
        v(input, 3) == 0 && v(input, 0) == 0 && v(input, 1) == 0 && v(input, 2) == 0
            && amount1(input) > 0 && v(input, 46) > 1000 && actor(input) == recipient(input)
    } else {
        v(input, 3) == 1 && v(input, 4) == 0 && v(input, 0) > 0 && v(input, 1) > 0 && v(input, 2) >= 1000
        && if is_swap(input) {
            net(gross(input)) > 0 && 0 < paid(input) && paid(input) <= quote(input) && paid(input) < y_out(input)
        } else if action(input) == 3 {
            used(input, 0) > 0 && used(input, 1) > 0 && minted(input) > 0 && actor(input) == recipient(input)
        } else {
            amount(input) <= v(input, 5 + actor(input)) && amount(input) <= v(input, 2) - 1000
                && payout(input, 0) > 0 && payout(input, 1) > 0
        }
    }
}

/// One key's limbs, from the program's carry chain and its final-carry guard.
pub(crate) proof fn key_limbs(input: Seq<i128>, key: int, column: int)
    requires
        0 <= key < 8, 0 <= v(input, 8 + 4 * key) < limb(), 0 <= v(input, 9 + 4 * key) < limb(),
        0 <= v(input, 10 + 4 * key) < limb(), 0 <= v(input, 11 + 4 * key) < limb(),
        succ(input, 8 + 4 * key) == column - (column / 4294967296) * 4294967296,
        succ(input, 9 + 4 * key) == (v(input, 9 + 4 * key) + column / 4294967296)
            - ((v(input, 9 + 4 * key) + column / 4294967296) / 4294967296) * 4294967296,
        succ(input, 10 + 4 * key) == (v(input, 10 + 4 * key) + (v(input, 9 + 4 * key) + column / 4294967296) / 4294967296)
            - ((v(input, 10 + 4 * key) + (v(input, 9 + 4 * key) + column / 4294967296) / 4294967296) / 4294967296) * 4294967296,
        succ(input, 11 + 4 * key) == (v(input, 11 + 4 * key) + (v(input, 10 + 4 * key) + (v(input, 9 + 4 * key) + column / 4294967296) / 4294967296) / 4294967296)
            - ((v(input, 11 + 4 * key) + (v(input, 10 + 4 * key) + (v(input, 9 + 4 * key) + column / 4294967296) / 4294967296) / 4294967296) / 4294967296) * 4294967296,
        (v(input, 11 + 4 * key) + (v(input, 10 + 4 * key) + (v(input, 9 + 4 * key) + column / 4294967296) / 4294967296) / 4294967296) / 4294967296 == 0,
    ensures
        value(successor(input), key) == column - v(input, 8 + 4 * key) + value(pre(input), key),
        limbs_in_range(successor(input), key),
{
    limbs(v(input, 8 + 4 * key), v(input, 9 + 4 * key), v(input, 10 + 4 * key), v(input, 11 + 4 * key),
        column, succ(input, 8 + 4 * key), succ(input, 9 + 4 * key), succ(input, 10 + 4 * key),
        succ(input, 11 + 4 * key));
}

/// The column of key k's least limb: pre limb - debit + credit.
pub(crate) open spec fn column(input: Seq<i128>, key: int) -> int {
    v(input, 8 + 4 * key) - debit(input, key) + credit(input, key)
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_63(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 63,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_63(input);
    early_63(input);
    assert(input[40] == 0i128 && input[41] == 0i128 && input[49] == 1i128 && input[3] == 0i128);
    sigma_63_0(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

/// Swap branches: 64/65 direction 0 with and without a protocol part, 66/67
/// direction 1; action 1 is exact input, action 2 exact output.
#[verifier::rlimit(100)]
pub(crate) proof fn branch_64_1(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 64, input[40] == 1i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_64(input);
    early_64(input);
    sigma_64_1(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_64_2(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 64, input[40] == 2i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_64(input);
    early_64(input);
    sigma_64_2(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_65_1(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 65, input[40] == 1i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_65(input);
    early_65(input);
    sigma_65_1(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_65_2(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 65, input[40] == 2i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_65(input);
    early_65(input);
    sigma_65_2(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_66_1(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 66, input[40] == 1i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_66(input);
    early_66(input);
    sigma_66_1(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_66_2(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 66, input[40] == 2i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_66(input);
    early_66(input);
    sigma_66_2(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_67_1(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 67, input[40] == 1i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_67(input);
    early_67(input);
    sigma_67_1(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_67_2(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 67, input[40] == 2i128,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_67(input);
    early_67(input);
    sigma_67_2(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

pub(crate) proof fn gross_nonneg(input: Seq<i128>)
    requires input_bounds(input),
    ensures gross(input) >= 0,
{
    if action(input) == 2 {
        ceil_facts(x_in(input), amount(input), den(y_out(input) - amount(input)));
        ceil_facts(mdc(x_in(input), amount(input), den(y_out(input) - amount(input))), 10000, 9970);
    }
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_68(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 68,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_68(input);
    early_68(input);
    sigma_68_3(input);
    products(amount1(input), v(input, 0), amount(input), v(input, 1));
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

#[verifier::rlimit(100)]
pub(crate) proof fn branch_69(input: Seq<i128>)
    requires input_bounds(input), decision_code(input) == 69,
    ensures exact(input, successor(input)), eligible(input),
{
    chain_69(input);
    early_69(input);
    // Codes 64..=67 are exhausted for swaps: their protocol parts are > 0 or == 0.
    gross_nonneg(input);
    fee_split(gross(input));
    assert(input[40] == 4i128);
    sigma_69_4(input);
    key_limbs(input, 0, column(input, 0));
    key_limbs(input, 1, column(input, 1));
    key_limbs(input, 2, column(input, 2));
    key_limbs(input, 3, column(input, 3));
    key_limbs(input, 4, column(input, 4));
    key_limbs(input, 5, column(input, 5));
    key_limbs(input, 6, column(input, 6));
    key_limbs(input, 7, column(input, 7));
}

// ---------------------------------------------------------------------------
// The integer Step contract of one accepted step, on complete native states.

pub(crate) open spec fn account_key(account: int, asset: int) -> int { 4 + 2 * account + asset }

pub(crate) open spec fn mirrored(st: Seq<int>) -> bool {
    value(st, 0) == st[0] && value(st, 1) == st[1] && st[5] + st[6] + st[7] == st[2]
}

pub(crate) open spec fn live(st: Seq<int>) -> bool {
    st[3] == 1 && st[4] == 0 && st[0] > 0 && st[1] > 0 && st[2] >= 1000 && st[7] >= 1000
}

pub(crate) open spec fn absent(st: Seq<int>) -> bool {
    st[3] == 0 && st[0] == 0 && st[1] == 0 && st[2] == 0 && st[5] == 0 && st[6] == 0 && st[7] == 0
}

/// Every holder of one asset: the pool, fee ingress and both ordinary accounts.
pub(crate) open spec fn holders(st: Seq<int>, asset: int) -> int {
    value(st, asset) + value(st, 2 + asset) + value(st, 4 + asset) + value(st, 6 + asset)
}

pub(crate) open spec fn wellformed(st: Seq<int>) -> bool {
    st.len() == 40
    && limbs_in_range(st, 0) && limbs_in_range(st, 1) && limbs_in_range(st, 2) && limbs_in_range(st, 3)
    && limbs_in_range(st, 4) && limbs_in_range(st, 5) && limbs_in_range(st, 6) && limbs_in_range(st, 7)
    && st[5] >= 0 && st[6] >= 0 && st[7] >= 0
}

pub(crate) open spec fn pool_rule(action: int, actor: int, a: Seq<int>, b: Seq<int>) -> bool {
    if action == 0 {
        absent(a) && b[2] > 1000 && b[7] == 1000 && b[5 + actor] == b[2] - 1000
    } else if action == 1 || action == 2 {
        live(a) && b[2] == a[2] && a[0] * a[1] <= b[0] * b[1] && b[5 + actor] == a[5 + actor] && b[7] == a[7]
    } else if action == 3 {
        live(a) && b[2] > a[2]
            && (b[2] - a[2]) * a[0] <= (b[0] - a[0]) * a[2] && (b[2] - a[2]) * a[1] <= (b[1] - a[1]) * a[2]
            && b[5 + actor] == a[5 + actor] + (b[2] - a[2]) && b[7] == a[7]
    } else {
        live(a) && b[2] < a[2]
            && (a[0] - b[0]) * a[2] <= (a[2] - b[2]) * a[0] && (a[1] - b[1]) * a[2] <= (a[2] - b[2]) * a[1]
            && b[5 + actor] == a[5 + actor] - (a[2] - b[2]) && b[7] == a[7]
    }
}

/// Mirrors before and after, conservation of each asset over every holder,
/// isolation of the fee ingress and of the other ordinary account, and the
/// command's pool rule.
pub(crate) open spec fn native_contract(a: Seq<int>, action: int, actor: int, b: Seq<int>) -> bool {
    (actor == 0 || actor == 1) && 0 <= action <= 4
    && wellformed(a) && wellformed(b) && mirrored(a) && mirrored(b) && live(b)
    && holders(b, 0) == holders(a, 0) && holders(b, 1) == holders(a, 1)
    && value(b, 2) >= value(a, 2) && value(b, 3) >= value(a, 3)
    && value(b, account_key(1 - actor, 0)) >= value(a, account_key(1 - actor, 0))
    && value(b, account_key(1 - actor, 1)) >= value(a, account_key(1 - actor, 1))
    && b[5 + (1 - actor)] == a[5 + (1 - actor)]
    && pool_rule(action, actor, a, b)
}

pub(crate) proof fn pre_wellformed(input: Seq<i128>)
    requires input_bounds(input),
    ensures wellformed(pre(input)),
{
}

/// The quantities each command's rule uses are nonnegative and ordered.
pub(crate) proof fn amounts(input: Seq<i128>)
    requires input_bounds(input), eligible(input),
    ensures
        0 <= protocol(gross(input)) <= fee(gross(input)) <= gross(input),
        is_swap(input) ==> quote(input) * (x_in(input) + net(gross(input))) <= y_out(input) * net(gross(input)),
        action(input) == 3 ==> minted(input) * v(input, 0) <= used(input, 0) * v(input, 2)
            && minted(input) * v(input, 1) <= used(input, 1) * v(input, 2),
        action(input) == 4 ==> payout(input, 0) * v(input, 2) <= amount(input) * v(input, 0)
            && payout(input, 1) * v(input, 2) <= amount(input) * v(input, 1)
            && payout(input, 0) < v(input, 0) && payout(input, 1) < v(input, 1),
        action(input) == 3 ==> used(input, 0) >= 0 && used(input, 1) >= 0,
{
    gross_nonneg(input);
    fee_split(gross(input));
    if is_swap(input) {
        floor_facts(y_out(input), net(gross(input)), den(x_in(input) + net(gross(input))));
    }
    if action(input) == 3 {
        let s = v(input, 2);
        if ratio(input) {
            pro_rata(amount(input), v(input, 1), den(v(input, 0)));
        } else {
            pro_rata(amount1(input), v(input, 0), den(v(input, 1)));
        }
        let m0 = mdf(used(input, 0), s, den(v(input, 0)));
        let m1 = mdf(used(input, 1), s, den(v(input, 1)));
        pro_rata(used(input, 0), s, v(input, 0));
        pro_rata(used(input, 1), s, v(input, 1));
        let m = minted(input);
        assert(m * v(input, 0) <= used(input, 0) * s) by(nonlinear_arith)
            requires m <= m0, m0 * v(input, 0) <= used(input, 0) * s, v(input, 0) > 0;
        assert(m * v(input, 1) <= used(input, 1) * s) by(nonlinear_arith)
            requires m <= m1, m1 * v(input, 1) <= used(input, 1) * s, v(input, 1) > 0;
    }
    if action(input) == 4 {
        let s = v(input, 2);
        let z = amount(input);
        pro_rata(z, v(input, 0), s);
        pro_rata(z, v(input, 1), s);
        let p0 = payout(input, 0);
        let p1 = payout(input, 1);
        assert(p0 < v(input, 0)) by(nonlinear_arith)
            requires p0 * s <= z * v(input, 0), z < s, v(input, 0) > 0, s > 0, p0 >= 0;
        assert(p1 < v(input, 1)) by(nonlinear_arith)
            requires p1 * s <= z * v(input, 1), z < s, v(input, 1) > 0, s > 0, p1 >= 0;
    }
}

pub(crate) proof fn swap_rule(input: Seq<i128>, b: Seq<int>)
    requires input_bounds(input), eligible(input), exact(input, b), is_swap(input),
    ensures pool_rule(action(input), actor(input), pre(input), b), b[0] > 0, b[1] > 0,
{
    amounts(input);
    let x = x_in(input);
    let y = y_out(input);
    let g = gross(input);
    swap_product(x, y, net(g), quote(input), paid(input), g - protocol(g));
    let left = x + (g - protocol(g));
    let right = y - paid(input);
    if direction(input) == 0 {
        assert(b[0] == left && b[1] == right);
    } else {
        assert(b[1] == left && b[0] == right);
        assert(b[0] * b[1] == left * right) by(nonlinear_arith) requires b[1] == left, b[0] == right;
        assert(v(input, 0) * v(input, 1) == x * y) by(nonlinear_arith) requires x == v(input, 1), y == v(input, 0);
    }
}

/// Everything but the pool rule: conservation, mirrors and isolation.
#[verifier::rlimit(100)]
pub(crate) proof fn shared_rules(input: Seq<i128>, b: Seq<int>)
    requires input_bounds(input), eligible(input), exact(input, b),
    ensures
        wellformed(b), mirrored(pre(input)), mirrored(b),
        holders(b, 0) == holders(pre(input), 0), holders(b, 1) == holders(pre(input), 1),
        value(b, 2) >= value(pre(input), 2), value(b, 3) >= value(pre(input), 3),
        value(b, account_key(1 - actor(input), 0)) >= value(pre(input), account_key(1 - actor(input), 0)),
        value(b, account_key(1 - actor(input), 1)) >= value(pre(input), account_key(1 - actor(input), 1)),
        b[5 + (1 - actor(input))] == pre(input)[5 + (1 - actor(input))],
{
    amounts(input);
}

#[verifier::rlimit(100)]
pub(crate) proof fn command_rule(input: Seq<i128>, b: Seq<int>)
    requires input_bounds(input), eligible(input), exact(input, b),
    ensures pool_rule(action(input), actor(input), pre(input), b), live(b),
{
    amounts(input);
    if is_swap(input) {
        swap_rule(input, b);
    }
}

pub(crate) proof fn exact_meets_contract(input: Seq<i128>, b: Seq<int>)
    requires input_bounds(input), eligible(input), exact(input, b),
    ensures native_contract(pre(input), action(input), actor(input), b),
{
    pre_wellformed(input);
    shared_rules(input, b);
    command_rule(input, b);
}

// ---------------------------------------------------------------------------
// One coalition's aggregation: NativeBridge.lean's IntStep and IntCreate.

pub(crate) open spec fn member(mask: int, account: int) -> bool {
    mask == 3 || (mask == 1 && account == 0) || (mask == 2 && account == 1)
}
pub(crate) open spec fn liquid(st: Seq<int>, mask: int, asset: int) -> int {
    (if member(mask, 0) { value(st, 4 + asset) } else { 0int })
        + (if member(mask, 1) { value(st, 6 + asset) } else { 0int })
}
/// Everyone else's spendable holdings: fee ingress and every non-member.
pub(crate) open spec fn outside(st: Seq<int>, mask: int, asset: int) -> int {
    value(st, 2 + asset) + (if member(mask, 0) { 0int } else { value(st, 4 + asset) })
        + (if member(mask, 1) { 0int } else { value(st, 6 + asset) })
}
/// LP shares outside the coalition: the permanent lock and every non-member.
pub(crate) open spec fn outside_shares(st: Seq<int>, mask: int) -> int {
    st[7] + (if member(mask, 0) { 0int } else { st[5] }) + (if member(mask, 1) { 0int } else { st[6] })
}

/// IntPool of NativeBridge.lean for an accepted command (reject is identity).
pub(crate) open spec fn int_pool(action: int, a: Seq<int>, b: Seq<int>) -> bool {
    if action == 1 || action == 2 {
        b[2] == a[2] && a[0] * a[1] <= b[0] * b[1]
    } else if action == 3 {
        (b[2] - a[2]) * a[0] <= (b[0] - a[0]) * a[2] && (b[2] - a[2]) * a[1] <= (b[1] - a[1]) * a[2]
    } else {
        (a[0] - b[0]) * a[2] <= (a[2] - b[2]) * a[0] && (a[1] - b[1]) * a[2] <= (a[2] - b[2]) * a[1]
    }
}

/// IntStep q a b of NativeBridge.lean, fields in order, for the coalition `mask`.
pub(crate) open spec fn int_step(q: int, a: Seq<int>, b: Seq<int>, mask: int, action: int) -> bool {
    (0 < a[0] && 0 < a[1] && 0 < a[2] && q <= a[2])
    && (0 < b[0] && 0 < b[1] && 0 < b[2] && q <= b[2])
    && int_pool(action, a, b)
    && liquid(a, mask, 0) + a[0] + outside(a, mask, 0) == liquid(b, mask, 0) + b[0] + outside(b, mask, 0)
    && liquid(a, mask, 1) + a[1] + outside(a, mask, 1) == liquid(b, mask, 1) + b[1] + outside(b, mask, 1)
    && outside(a, mask, 0) <= outside(b, mask, 0) && outside(a, mask, 1) <= outside(b, mask, 1)
}

/// IntCreate q initial0 initial1 outside0 outside1 b of NativeBridge.lean.
pub(crate) open spec fn int_create(q: int, i0: int, i1: int, o0: int, o1: int, b: Seq<int>, mask: int) -> bool {
    (0 < b[0] && 0 < b[1] && 0 < b[2] && q <= b[2])
    && i0 + o0 == liquid(b, mask, 0) + b[0] + outside(b, mask, 0)
    && i1 + o1 == liquid(b, mask, 1) + b[1] + outside(b, mask, 1)
    && o0 <= outside(b, mask, 0) && o1 <= outside(b, mask, 1)
}

/// What the Lean premises need for one coalition that contains the actor.
pub(crate) open spec fn coalition_step(a: Seq<int>, action: int, b: Seq<int>, mask: int) -> bool {
    if action == 0 {
        int_create(outside_shares(b, mask), liquid(a, mask, 0), liquid(a, mask, 1),
            outside(a, mask, 0), outside(a, mask, 1), b, mask)
    } else {
        int_step(outside_shares(a, mask), a, b, mask, action)
            && outside_shares(b, mask) == outside_shares(a, mask)
    }
}

#[verifier::rlimit(100)]
pub(crate) proof fn coalition(a: Seq<int>, action: int, actor: int, b: Seq<int>, mask: int)
    requires native_contract(a, action, actor, b), 1 <= mask <= 3, member(mask, actor),
    ensures coalition_step(a, action, b, mask),
{
}

/// The successor the decision table assigns from the outputs, in state order.
pub(crate) open spec fn assigned_state(out: Seq<i128>) -> Seq<int> {
    Seq::new(40nat, |j: int| assigned(out, j))
}

pub(crate) proof fn accepted_terms(input: Seq<i128>, code: int)
    requires input_bounds(input), accepting(code), decision_code(input) == code,
    ensures exact(input, successor(input)), eligible(input),
{
    if code == 63 { branch_63(input); }
    else if code == 68 { branch_68(input); }
    else if code == 69 { branch_69(input); }
    else {
        chain_lemma_action(input, code);
        if code == 64 { if input[40] == 1i128 { branch_64_1(input); } else { branch_64_2(input); } }
        else if code == 65 { if input[40] == 1i128 { branch_65_1(input); } else { branch_65_2(input); } }
        else if code == 66 { if input[40] == 1i128 { branch_66_1(input); } else { branch_66_2(input); } }
        else { if input[40] == 1i128 { branch_67_1(input); } else { branch_67_2(input); } }
    }
}

/// The swap codes are reached only by actions 1 and 2.
pub(crate) proof fn chain_lemma_action(input: Seq<i128>, code: int)
    requires input_bounds(input), 64 <= code <= 67, decision_code(input) == code,
    ensures input[40] == 1i128 || input[40] == 2i128,
{
    if code == 64 { chain_64(input); early_64(input); }
    else if code == 65 { chain_65(input); early_65(input); }
    else if code == 66 { chain_66(input); early_66(input); }
    else { chain_67(input); early_67(input); }
}

/// The refinement theorem. On every input the production evaluator
/// specification accepts for the compiled Spot program, if the decision
/// output selects an accepting branch, then the successor that branch assigns
/// is exactly the specification's, it meets the integer Step contract, and
/// every coalition containing the actor gets NativeBridge.lean's IntStep (or
/// IntCreate for pool creation) with unchanged outside shares.
pub(crate) proof fn spot_refinement(inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>,
    roots: Seq<u16>, input: Seq<i128>, out: Seq<i128>)
    requires
        spot_nodes(nodes), spot_roots(roots), spot_inputs(inputs), spot_outputs(outputs),
        execution(inputs, outputs, nodes, roots, input) == Ok::<Seq<i128>, Failure>(out),
        accepting(out[0] as int),
    ensures
        exact(input, assigned_state(out)),
        native_contract(pre(input), action(input), actor(input), assigned_state(out)),
        forall|mask: int| 1 <= mask <= 3 && member(mask, actor(input))
            ==> #[trigger] coalition_step(pre(input), action(input), assigned_state(out), mask),
{
    program_terms(inputs, outputs, nodes, roots, input, out);
    let b = assigned_state(out);
    assert(b =~= successor(input));
    accepted_terms(input, out[0] as int);
    exact_meets_contract(input, b);
    assert forall|mask: int| 1 <= mask <= 3 && member(mask, actor(input))
        implies #[trigger] coalition_step(pre(input), action(input), b, mask) by {
        coalition(pre(input), action(input), actor(input), b, mask);
    }
}

}
