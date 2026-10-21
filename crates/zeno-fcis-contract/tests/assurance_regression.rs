//! Exact before/after observations for the table dependency boundary.
mod tables {
    include!("tables.rs");

    #[test]
    fn capture_table_decisions_and_usage() {
        let Ok(path) = std::env::var("TABLE_OBSERVATIONS") else {
            return;
        };
        let mut rows = String::new();
        for kind in ["map", "set"] {
            for capacity in 0..=3 {
                let compiled = compile(&format!("{kind}-{capacity}"));
                let a = authority(&compiled);
                let states = tables_review::values(&compiled.descriptor, 100, 1000).unwrap();
                for pre in states {
                    for op in 0..=7 {
                        for key in 0..=3 {
                            for value in 0..=1 {
                                let cmd = command(op, key, value);
                                let x = record(vec![]);
                                let result = run(&a, &pre, &cmd, &x);
                                rows.push_str(&format!(
                                    "{kind} {capacity} {pre:?} {cmd:?} {result:?}\n"
                                ));
                            }
                        }
                    }
                }
            }
        }
        std::fs::write(path, rows).unwrap();
    }
}
