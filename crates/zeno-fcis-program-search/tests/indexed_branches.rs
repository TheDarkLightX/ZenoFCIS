//! Selector references, incumbent retention, sampled synthesis and ownership.
#[path = "support/selectors.rs"]
mod selectors;
use zeno_fcis_program_search::{Outcome, Settings, search};
use zeno_fcis_synthesis::finite_runtime::import_program;

#[test]
fn selector_data_matches_independent_references_on_every_tuple() {
    for name in selectors::NAMES {
        let bytes = std::fs::read(format!(
            "{}/tests/data/selectors/{name}.zcve",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(bytes, selectors::bytes(&selectors::original(name)));
        let p = import_program(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
        for ordinal in 0..4096 {
            let x = selectors::tuple(ordinal);
            assert_eq!(
                p.evaluate(&x).unwrap_or_else(|e| panic!("{e:?}")),
                vec![selectors::reference(name, &x)],
                "{name} {x:?}"
            );
        }
    }
}

fn result(original: &[u8], settings: &Settings) -> (usize, Vec<u8>) {
    match search(original, &[], settings) {
        Outcome::Improved(c) => (c.nodes, c.bytes),
        Outcome::Unimproved(_) => (
            import_program(original)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .nodes()
                .len(),
            original.to_vec(),
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn indexed_portfolio_never_loses_the_ordinary_incumbent() {
    let off = Settings::default();
    assert!(!off.proposers.indexed_branches);
    let mut on = off;
    on.proposers.indexed_branches = true;
    for name in selectors::NAMES {
        let source = selectors::original(name);
        let small = zeno_fcis_synthesis::finite::Program::try_new(
            vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 1 }; 4],
            vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 1 }],
            source.nodes().to_vec(),
            source.roots().to_vec(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let bytes = selectors::bytes(&small);
        let before = result(&bytes, &off);
        let after = result(&bytes, &on);
        assert!(after.0 <= before.0, "{name}: {} > {}", after.0, before.0);
    }
}

#[test]
fn indexed_sampled_search_is_exact_deterministic_and_owns_results() {
    let original = selectors::bytes(&selectors::original("lower-median4"));
    let mut settings = Settings::default();
    settings.proposers.indexed_branches = true;
    settings.proposers.top_down = false;
    settings.proposers.diagram = false;
    settings.proposers.resub = false;
    settings.proposers.fold = false;
    settings.shape.cegis_tuples = 16;
    settings.shape.sample = 32;
    let first = search(&original, &[], &settings);
    assert_eq!(first, search(&original, &[], &settings));
    let Outcome::Improved(mut candidate) = first else {
        panic!("{first:?}")
    };
    let saved = candidate.bytes.clone();
    candidate.bytes.fill(0);
    let (nodes, bytes) = result(&original, &settings);
    assert_eq!(bytes, saved);
    assert!(
        nodes
            < import_program(&original)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .nodes()
                .len()
    );
    let p = import_program(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
    for ordinal in 0..4096 {
        let x = selectors::tuple(ordinal);
        assert_eq!(
            p.evaluate(&x).unwrap_or_else(|e| panic!("{e:?}")),
            vec![selectors::reference("lower-median4", &x)]
        );
    }
}

#[test]
fn indexed_proposals_cross_full_domain_comparison() {
    let source = selectors::original("lower-median4");
    let p = zeno_fcis_synthesis::finite::Program::try_new(
        vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 2 }; 4],
        vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 2 }],
        source.nodes().to_vec(),
        source.roots().to_vec(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let original = selectors::bytes(&p);
    let mut settings = Settings::default();
    settings.proposers.indexed_branches = true;
    settings.proposers.top_down = false;
    settings.proposers.diagram = false;
    settings.proposers.resub = false;
    settings.proposers.fold = false;
    let (candidate, provenance) = match search(&original, &[], &settings) {
        Outcome::Improved(c) => (Some(c.bytes), c.provenance),
        Outcome::Unimproved(p) => (None, p),
        other => panic!("{other:?}"),
    };
    let offered = provenance
        .offers
        .iter()
        .find(|o| o.proposer == "indexed-branches")
        .unwrap_or_else(|| panic!("indexed proposer did not run"));
    assert_eq!(offered.offered, 1);
    for refusal in &provenance.refused {
        assert!(matches!(
            refusal.refusal,
            zeno_fcis_program_search::Refusal::Differs { .. }
        ));
        println!("portfolio full-domain refused: {:?}", refusal.refusal);
    }
    if let Some(bytes) = candidate {
        let candidate = import_program(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
        for a in 0..=2 {
            for b in 0..=2 {
                for c in 0..=2 {
                    for d in 0..=2 {
                        let x = [a, b, c, d];
                        assert_eq!(
                            candidate.evaluate(&x).unwrap_or_else(|e| panic!("{e:?}")),
                            vec![selectors::reference("lower-median4", &x)]
                        );
                    }
                }
            }
        }
    }
}
