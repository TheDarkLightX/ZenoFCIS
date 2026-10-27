//! Instruction facts of an evaluated program, read off the production
//! evaluator specification (`evaluation/spec.rs`).
//!
//! `prefix_evaluated` turns a successful `prefix` into one equation per node:
//! the node's value is what `node` computes from the earlier values. `fact`
//! restates that equation per operator without the `Result` plumbing; the
//! nonlinear operators are kept behind opaque functions so the solver treats
//! them as uninterpreted until a lemma reveals them.

use crate::evaluation::spec::{node, prefix, projection};
use crate::evaluation::{Failure, Op};
use crate::value::scalar::{division_value, mul_div_value, multiply_value, products_ge_value};
use vstd::arithmetic::div_mod::*;
use vstd::prelude::*;

verus! {

/// floor(a*b/d) for d > 0.
#[verifier::opaque]
pub(crate) open spec fn mdf(a: int, b: int, d: int) -> int { (a * b) / d }

/// ceil(a*b/d) for d > 0.
#[verifier::opaque]
pub(crate) open spec fn mdc(a: int, b: int, d: int) -> int {
    (a * b) / d + if (a * b) % d == 0 { 0int } else { 1int }
}

/// a0*a1*a2 >= b0*b1*b2.
#[verifier::opaque]
pub(crate) open spec fn pge(a0: int, a1: int, a2: int, b0: int, b1: int, b2: int) -> bool {
    a0 * a1 * a2 >= b0 * b1 * b2
}

pub(crate) open spec fn bit(b: bool) -> i128 { if b { 1i128 } else { 0i128 } }

/// The node at `k` evaluated successfully to `vals[k]` from the earlier values.
pub(crate) open spec fn node_ok(nodes: Seq<Op>, input: Seq<i128>, vals: Seq<i128>, k: int) -> bool {
    node(nodes[k], input, vals.subrange(0, k)) == Ok::<i128, Failure>(vals[k])
}

pub(crate) open spec fn evaluated(nodes: Seq<Op>, input: Seq<i128>, vals: Seq<i128>) -> bool {
    vals.len() == nodes.len()
        && forall|k: int| 0 <= k < vals.len() ==> #[trigger] node_ok(nodes, input, vals, k)
}

pub(crate) proof fn prefix_evaluated(nodes: Seq<Op>, input: Seq<i128>, n: nat, vals: Seq<i128>)
    requires
        n <= nodes.len(),
        prefix(nodes, input, n) == Ok::<Seq<i128>, Failure>(vals),
    ensures
        vals.len() == n,
        forall|k: int| 0 <= k < n ==> #[trigger] node_ok(nodes, input, vals, k),
    decreases n,
{
    if n > 0 {
        let previous = prefix(nodes, input, (n - 1) as nat);
        match previous {
            Err(_) => {},
            Ok(prior) => {
                prefix_evaluated(nodes, input, (n - 1) as nat, prior);
                match node(nodes[n as int - 1], input, prior) {
                    Err(_) => {},
                    Ok(value) => {
                        assert(vals == prior.push(value));
                        assert forall|k: int| 0 <= k < n implies #[trigger] node_ok(nodes, input, vals, k) by {
                            if k < n - 1 {
                                assert(node_ok(nodes, input, prior, k));
                                assert(vals.subrange(0, k) =~= prior.subrange(0, k));
                            } else {
                                assert(vals.subrange(0, k) =~= prior);
                            }
                        }
                    },
                }
            },
        }
    }
}

pub(crate) proof fn projection_values(vals: Seq<i128>, roots: Seq<u16>, n: nat, out: Seq<i128>)
    requires
        n <= roots.len(),
        projection(vals, roots, n) == Ok::<Seq<i128>, Failure>(out),
    ensures
        out.len() == n,
        forall|i: int| 0 <= i < n ==> #[trigger] out[i] == vals[roots[i] as int] && (roots[i] as int) < vals.len(),
    decreases n,
{
    if n == 0 {
        assert(out == Seq::<i128>::empty());
    } else {
        match projection(vals, roots, (n - 1) as nat) {
            Err(e) => {
                assert(projection(vals, roots, n) == Err::<Seq<i128>, Failure>(e));
            },
            Ok(prior) => {
                projection_values(vals, roots, (n - 1) as nat, prior);
                if (roots[n as int - 1] as int) < vals.len() {
                    assert(out == prior.push(vals[roots[n as int - 1] as int]));
                    assert forall|i: int| 0 <= i < n implies #[trigger] out[i] == vals[roots[i] as int]
                        && (roots[i] as int) < vals.len() by {
                        if i < n - 1 {
                            assert(out[i] == prior[i]);
                        }
                    }
                } else {
                    assert(projection(vals, roots, n) == Err::<Seq<i128>, Failure>(Failure::Reference));
                }
            },
        }
    }
}

/// Floor division by a positive divisor is Euclidean division.
pub(crate) proof fn positive_floor(a: i128, b: i128)
    requires b > 0,
    ensures division_value(false, a, b) == crate::value::scalar::narrow(a as int / b as int),
{
    let x: int = if a < 0 { -(a as int) } else { a as int };
    let y = b as int;
    lemma_fundamental_div_mod(x, y);
    lemma_mod_bound(x, y);
    let q = x / y;
    let r = x % y;
    if a < 0 {
        if r == 0 {
            assert(a as int == (-q) * y) by(nonlinear_arith) requires x == y * q + r, r == 0, x == -(a as int);
            lemma_fundamental_div_mod_converse_div(a as int, y, -q, 0);
        } else {
            assert(a as int == (-(q + 1)) * y + (y - r)) by(nonlinear_arith)
                requires x == y * q + r, x == -(a as int);
            lemma_fundamental_div_mod_converse_div(a as int, y, -(q + 1), y - r);
        }
    } else {
        lemma_fundamental_div_mod_converse_div(a as int, y, q, r);
    }
}


}
