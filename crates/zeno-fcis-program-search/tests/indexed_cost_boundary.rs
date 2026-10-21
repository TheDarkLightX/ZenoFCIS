//! Expanded tree bounds must stay total even for heavily shared source DAGs.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_program_search::{Outcome, Settings, search};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

#[test]
fn shared_source_cost_saturation_does_not_panic() {
    let mut nodes = vec![Op::Input(0), Op::Input(1), Op::Input(2), Op::Int(1)];
    let mut root = 1;
    for _ in 0..80 {
        let id = u16::try_from(nodes.len()).unwrap_or_else(|e| panic!("{e:?}"));
        nodes.push(Op::Select(0, root, root));
        root = id;
    }
    let output = u16::try_from(nodes.len()).unwrap_or_else(|e| panic!("{e:?}"));
    nodes.push(Op::ProductsGe([root, 3, 3], [2, 3, 3]));
    let program = Program::try_new(
        vec![
            Domain::Bool,
            Domain::Int { min: 0, max: 1 },
            Domain::Int { min: 0, max: 1 },
        ],
        vec![Domain::Bool],
        nodes,
        vec![output],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let bytes = program
        .value()
        .unwrap_or_else(|e| panic!("{e:?}"))
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut settings = Settings::default();
    settings.proposers.indexed_branches = true;
    settings.proposers.top_down = false;
    settings.proposers.diagram = false;
    settings.proposers.resub = false;
    settings.proposers.fold = false;
    assert!(!matches!(
        search(&bytes, &[], &settings),
        Outcome::Stopped(_)
    ));
}
