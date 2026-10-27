use zenofcis_zusd_v1::{Failure, Limits, Q, divided_product, fees};
#[test]
fn native_ordered_floors_and_three_factor_products_match_exact_integers() {
    let limits = Limits {
        steps: 10_000_000,
        reads: 100_000,
        bytes: 10_000_000,
    };
    for row in include_str!("test-data/round6_arithmetic.tsv").lines() {
        let cells: Vec<_> = row.split('\t').collect();
        let n: Vec<u128> = cells[1..5].iter().map(|x| x.parse().unwrap()).collect();
        let expected = match cells[5] {
            "Arithmetic" => Err(Failure::Arithmetic),
            "Domain" => Err(Failure::Domain),
            "Reject1045" => Err(Failure::Reject(1045)),
            "Reject1050" => Err(Failure::Reject(1050)),
            "Reject1051" => Err(Failure::Reject(1051)),
            x => Ok(x.parse().unwrap()),
        };
        let actual = if cells[0] == "divided" {
            divided_product(n[0], n[1], n[2], n[3], limits)
                .into_parts()
                .0
        } else {
            fees::redemption(
                fees::State {
                    base: 0,
                    last_epoch: 0,
                },
                n[0],
                n[1],
                n[2],
                Q,
                Some(fees::LogicalTime {
                    epoch: 0,
                    elapsed_minutes: 0,
                }),
                limits,
            )
            .into_parts()
            .0
            .map(|fee| fee.amount)
        };
        assert_eq!(actual, expected, "{row}");
    }
}
