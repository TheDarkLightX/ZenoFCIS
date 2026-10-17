//! Minimal native semantic witnesses; no application or publication entry point.
mod spot;
mod spot_bounds;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/wide.rs"]
mod wide;

fn main() {
    println!("fee {:?}", spot::fee_split(1, 1));
    println!("fee_full {:?}", spot::fee_split(u128::MAX, 10_000));
    println!("swap {:?}", spot::exact_in(2, 0, 3, 4));
    println!("burn {:?}", spot::lp_burn(1, 3, 2));
    println!("minimum {}", spot::minimum(2, 7));
    println!(
        "min_bounds {:?}",
        spot::min_bounds(
            spot::Range { lower: 1, upper: 3 },
            spot::Range { lower: 5, upper: 7 }
        )
    );
    println!("bounds {:?}", spot_bounds::analyse(3, 5, 7));
}
