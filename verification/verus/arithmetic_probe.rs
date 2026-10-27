//! Native observations of the exact production arithmetic; the oracle is Python.
#![allow(dead_code)]
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/scalar.rs"]
mod scalar;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/wide.rs"]
mod wide;
use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    for line in input.lines() {
        let (op, operands) = line.split_once(' ').unwrap();
        let a: Vec<i128> = operands.split(' ').map(|x| x.parse().unwrap()).collect();
        let value = match op {
            "add" => a[0].checked_add(a[1]),
            "sub" => a[0].checked_sub(a[1]),
            "mul" => scalar::multiply(a[0], a[1]),
            "div_floor" | "div_ceil" => scalar::divide(op == "div_ceil", a[0], a[1]),
            "mul_div_floor" | "mul_div_ceil" => {
                scalar::mul_div(op == "mul_div_ceil", a[0], a[1], a[2])
            }
            "signed_mul_div_floor" => scalar::signed_mul_div_floor(a[0], a[1], a[2]),
            "products_ge" => {
                let (b, c) = if a.len() == 4 {
                    ([a[0], a[1], 1], [a[2], a[3], 1])
                } else {
                    ([a[0], a[1], a[2]], [a[3], a[4], a[5]])
                };
                scalar::products_ge(b[0], b[1], b[2], c[0], c[1], c[2]).map(i128::from)
            }
            _ => panic!("unrecognized probe operation"),
        };
        match value {
            Some(n) => println!("{n}"),
            None => println!("null"),
        }
    }
}
