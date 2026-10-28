use super::{interval_cutters, outside_failure_cutters};

#[test]
fn shared_cut_groups_match_the_original_range_filter() {
    for n in 0usize..=7 {
        for flags in 0usize..1usize << n {
            let failure_case: Vec<bool> = (0..n).map(|at| flags & (1 << at) != 0).collect();
            for selected in 0usize..1usize << n {
                let members: Vec<usize> = (0..n).filter(|at| selected & (1 << at) != 0).collect();
                let mut previous = 0usize;
                let expected: Vec<Vec<usize>> = members
                    .iter()
                    .enumerate()
                    .map(|(position, member)| {
                        let from = if position == 0 { 0 } else { previous + 1 };
                        previous = *member;
                        (from..*member).filter(|at| failure_case[*at]).collect()
                    })
                    .collect();
                assert_eq!(outside_failure_cutters(&failure_case, &members), expected);
            }
            for from in 0..=n {
                for until in from..=n {
                    let expected: Vec<usize> =
                        (from..until).filter(|at| failure_case[*at]).collect();
                    assert_eq!(interval_cutters(&failure_case, from, until), expected);
                }
            }
        }
    }
}

#[test]
fn the_cut_immediately_before_a_member_is_retained() {
    assert_eq!(
        outside_failure_cutters(&[false, true, false], &[0, 2]),
        [vec![], vec![1]]
    );
    assert_eq!(
        outside_failure_cutters(&[true, true, true, true], &[1, 2]),
        [vec![0], vec![]]
    );
}
