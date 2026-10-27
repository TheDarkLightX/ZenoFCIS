//! Paired measurements; timings and search reports grant no authority.
#[path = "../tests/support/selectors.rs"]
mod selectors;
use zeno_fcis_program_search::{Outcome, Settings, search};
use zeno_fcis_synthesis::finite_runtime::import_program;

fn main() {
    if std::env::args().any(|a| a == "--write-data") {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/selectors");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e:?}"));
        for name in selectors::NAMES {
            std::fs::write(
                dir.join(format!("{name}.zcve")),
                selectors::bytes(&selectors::original(name)),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
        }
        return;
    }
    let cases: Vec<_> = selectors::CORE
        .iter()
        .map(|name| (*name, selectors::core(name)))
        .chain(
            selectors::NAMES
                .iter()
                .map(|name| (*name, selectors::bytes(&selectors::original(name)))),
        )
        .collect();
    // Freeze and validate all inputs before any search is timed.
    for (name, bytes) in &cases {
        let p = import_program(bytes).unwrap_or_else(|e| panic!("{e:?}"));
        if selectors::NAMES.contains(name) {
            for i in 0..4096 {
                let tuple = selectors::tuple(i);
                assert_eq!(
                    p.evaluate(&tuple).unwrap_or_else(|e| panic!("{e:?}")),
                    vec![selectors::reference(name, &tuple)]
                );
            }
        }
    }
    let mut counts = std::collections::BTreeMap::new();
    for run in 0..3 {
        for on in if run % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            for (name, original) in &cases {
                let mut settings = Settings::default();
                settings.proposers.indexed_branches = on;
                let start = std::time::Instant::now();
                let result = search(original, &[], &settings);
                let ns = start.elapsed().as_nanos();
                let (bytes, p) = match result {
                    Outcome::Improved(c) => (c.bytes, c.provenance),
                    Outcome::Unimproved(p) => (original.clone(), p),
                    Outcome::Stopped(s) => panic!("{name} {on}: {s:?}"),
                };
                let program = import_program(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
                if selectors::NAMES.contains(name) {
                    for i in 0..4096 {
                        let tuple = selectors::tuple(i);
                        assert_eq!(
                            program.evaluate(&tuple).unwrap_or_else(|e| panic!("{e:?}")),
                            vec![selectors::reference(name, &tuple)]
                        );
                    }
                }
                counts.insert((run, *name, on), program.nodes().len());
                let hex: String = bytes.iter().map(|v| format!("{v:02x}")).collect();
                println!(
                    "{{\"run\":{run},\"name\":\"{name}\",\"on\":{on},\"nodes\":{},\"work\":{},\"ns\":{ns},\"bytes\":\"{hex}\"}}",
                    program.nodes().len(),
                    p.work_used
                );
            }
        }
    }
    for run in 0..3 {
        for (name, _) in &cases {
            assert!(
                counts[&(run, *name, true)] <= counts[&(run, *name, false)],
                "{name}: indexed result grew"
            );
        }
    }
}
