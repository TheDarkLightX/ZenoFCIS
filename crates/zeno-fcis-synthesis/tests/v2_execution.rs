//! Compare the public V2 convenience adapter with the checked entry point.
use zeno_fcis_synthesis::finite::{
    Domain, Op, Program, V2ExecutionFailure, V2Resource, execute_v2, v2_zero_limits,
};

#[test]
fn public_program_adapter_matches_checked_entry_and_restarts_usage() {
    let domain = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let program = Program::try_new(
        vec![domain],
        vec![domain],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap_or_else(|error| panic!("program: {error}"));
    for quota in [0, 1, 2, 3, 4, u64::MAX] {
        for value in [i64::MIN, -1, 0, i64::MAX] {
            let limits = v2_zero_limits().with_limit(V2Resource::Step, quota);
            let actual = program.execute_v2(&[value], limits);
            assert_eq!(
                actual,
                execute_v2(
                    program.inputs(),
                    program.outputs(),
                    program.nodes(),
                    program.roots(),
                    &[value],
                    limits
                )
            );
            let (result, usage) = actual.into_parts();
            assert_eq!(usage.used(V2Resource::Step), quota.min(3));
            if quota >= 3 && value == i64::MAX {
                assert_eq!(result, Err(V2ExecutionFailure::Arithmetic));
            }
            assert_eq!(program.execute_v2(&[value], limits).into_parts().1, usage);
        }
    }
}
