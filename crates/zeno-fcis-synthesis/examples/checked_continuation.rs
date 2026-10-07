//! Complete original bounded fold through the normal library-owned cursor.
//! The result is untrusted computation data; publication remains a separate gate.
use zeno_fcis_synthesis::finite::{
    Domain, Op, V2Resource as Resource, v2_continuation as c, v2_zero_limits,
};
fn main() -> Result<(), String> {
    let accumulator = Domain::Int {
        min: -100,
        max: 100,
    };
    let item = Domain::Int { min: -3, max: 3 };
    let graph = c::admit_graph(
        vec![accumulator, item],
        vec![accumulator],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
        vec![2],
    )
    .map_err(|e| format!("{e:?}"))?;
    let context = c::Context {
        state_root: [1; 32],
        state_version: 0,
        invocation_hash: [2; 32],
    };
    let budget = v2_zero_limits()
        .with_limit(Resource::Read, 6)
        .with_limit(Resource::Write, 3)
        .with_limit(Resource::Candidate, 3)
        .with_limit(Resource::Byte, 1000)
        .with_limit(Resource::Step, 9);
    let mut fold = c::start(
        graph,
        vec![0],
        vec![vec![1], vec![2], vec![3]],
        context,
        c::default_limits(),
        budget,
    )
    .map_err(|e| format!("{e:?}"))?;
    fold.advance(0, 2).map_err(|e| format!("{e:?}"))?;
    assert_eq!(fold.finish(context), Err(c::Failure::Incomplete));
    fold.advance(2, 1).map_err(|e| format!("{e:?}"))?;
    assert_eq!(fold.finish(context), Ok(vec![6]));
    assert_eq!(fold.reserved_budget().used(Resource::Byte), 120);
    assert_eq!(fold.usage().used(Resource::Step), 9);
    println!(
        "complete checked fold [1,2,3], chunks2+1 =>6; Read6 Write3 Candidate3 Byte120 Step9; publication_authority:false"
    );
    Ok(())
}
