//! Frozen-bundle reference, codecs, correlated features and compiler inventory.
//! This gate requires the real prepared bundle; missing evidence is a failure.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    expr::{self, Ast},
    graph::{Graph, ScalarGraph},
    model::{Contract, ScalarDomain},
    policy, program_bytes,
    review::{
        domain,
        evaluate::{Framer, digest, hex},
    },
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{
    Domain, Program, V2Resource, canonical_v2::output, v2_composition as c,
};

fn read(path: &Path, limit: usize) -> Vec<u8> {
    assert!(
        std::fs::symlink_metadata(path)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            .is_file(),
        "regular file: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert!(bytes.len() <= limit, "bounded input: {}", path.display());
    bytes
}
fn value(path: &Path) -> Value {
    let bytes = read(path, 1 << 24);
    let v: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut canonical =
        serde_json::to_vec(&v).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    canonical.push(b'\n');
    assert_eq!(bytes, canonical, "canonical input (no duplicate keys)");
    v
}
fn inventory(root: &Path, path: &Path, rows: &mut Vec<[String; 2]>) {
    let meta = std::fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert!(!meta.file_type().is_symlink());
    if meta.is_dir() {
        for e in std::fs::read_dir(path)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        {
            inventory(
                root,
                &e.unwrap_or_else(|error| panic!("required test result: {error:?}"))
                    .path(),
                rows,
            );
        }
    } else {
        assert!(meta.is_file());
        let name = path
            .strip_prefix(root)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            .to_str()
            .unwrap_or_else(|| panic!("required test value missing"));
        if name != "manifest.json" {
            rows.push([name.into(), hex(&digest(&read(path, 1 << 24)))]);
        }
    }
}
fn bundle() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("ZENO_FCIS_CONSTRUCTION_BUNDLE")
            .unwrap_or_else(|| panic!("real ZENO_FCIS_CONSTRUCTION_BUNDLE required; no skip")),
    );
    let mut rows = Vec::new();
    inventory(&root, &root, &mut rows);
    rows.sort();
    assert_eq!(
        value(&root.join("manifest.json")),
        json!({"schema":"zeno-fcis/priority-artifact-manifest/1","revision":"construction-r2","files":rows})
    );
    root
}
fn abstract_parts(root: &Path) -> (Declarations, Rules) {
    let rules = Rules::read(
        &String::from_utf8(read(&root.join("abstract/v2/policy.json"), 1 << 20))
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let declarations = Declarations::read(
        &String::from_utf8(read(&root.join("abstract/project.zeno"), 1 << 20))
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        &rules.leaf_bindings,
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    (declarations, rules)
}
fn record(fields: &[c::Field<'_>]) -> Vec<u8> {
    output::encode_record(fields, 1 << 20)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
}
fn atom(value: c::Atom<'_>) -> Vec<u8> {
    output::encode_atom(value, 1 << 20)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
}
#[test]
fn frozen_reference_matches_every_native_transition_and_genesis() {
    let root = bundle();
    let relation = value(&root.join("reference/relation.json"));
    let rows = relation["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("required test value missing"));
    assert_eq!(rows.len(), 720);
    let (declarations, rules) = abstract_parts(&root);
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let positions = domain::positions(&declarations)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let framer = Framer::new(&positions, &contract);
    let mut calls = 0;
    let mut usage = [0u128; 8];
    let mut count = |u: zeno_fcis_synthesis::finite::V2Usage| {
        calls += 1;
        for (sum, r) in usage.iter_mut().zip([
            V2Resource::Read,
            V2Resource::Write,
            V2Resource::Candidate,
            V2Resource::Effect,
            V2Resource::Byte,
            V2Resource::WitnessByte,
            V2Resource::Depth,
            V2Resource::Step,
        ]) {
            *sum += u128::from(u.used(r));
        }
    };
    policy::with_authority(&contract, &schema, |authority| {
        let frames = framer.frame(&[0, 0, 0]);
        let genesis = authority.genesis(&frames[0]);
        count(genesis.usage());
        assert!(genesis.result().is_ok());
        let mut observations = BTreeSet::new();
        for (k, row) in rows.iter().enumerate() {
            assert_eq!(row["state"], k / 16);
            assert_eq!(row["input"], k % 16);
            let frames = framer.frame(&[(k / 16) as i128, (k % 16) as i128, 0]);
            let evaluation = authority.evaluate(c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            });
            count(evaluation.usage());
            let candidate = evaluation
                .result()
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            let accepted = row["class"] == "Accept";
            assert_eq!(
                candidate.class(),
                if accepted {
                    c::Class::Accept
                } else {
                    c::Class::Reject
                },
                "class at {k}"
            );
            assert_eq!(
                candidate.reason(),
                if accepted {
                    None
                } else {
                    Some(
                        row["reason"]
                            .as_u64()
                            .unwrap_or_else(|| panic!("required test value missing"))
                            as u32,
                    )
                },
                "reason at {k}"
            );
            assert!(candidate.effects().is_empty());
            let post = if accepted {
                vec![c::Field {
                    id: 110,
                    value: c::Atom::I128(
                        row["successor"]
                            .as_i64()
                            .map(i128::from)
                            .unwrap_or_else(|| panic!("required test value missing")),
                    ),
                }]
            } else {
                vec![]
            };
            assert_eq!(record(candidate.post()), record(&post), "successor at {k}");
            if !accepted {
                assert!(candidate.patch().is_empty());
                assert_eq!(row["successor"], k / 16);
            }
            let deliveries = candidate.outbox();
            assert_eq!(deliveries.len(), usize::from(accepted));
            if accepted {
                let d = &deliveries[0];
                let expected = &row["outbox"][0];
                let fields: Vec<_> = (0..4)
                    .map(|j| c::Field {
                        id: 140 + j as u16,
                        value: c::Atom::I128(
                            expected["payload"][j][1]
                                .as_i64()
                                .map(i128::from)
                                .unwrap_or_else(|| panic!("required test value missing")),
                        ),
                    })
                    .collect();
                assert_eq!(d.ordinal, 0);
                assert_eq!(d.channel, 300);
                assert_eq!(atom(d.destination), atom(c::Atom::Text(b"priority-events")));
                assert_eq!(atom(d.idempotency), atom(c::Atom::U128(0)));
                assert_eq!(record(&d.payload), record(&fields));
            }
            observations.insert(
                serde_json::to_string(&json!([
                    row["class"],
                    row["reason"],
                    row["effects"],
                    row["outbox"]
                ]))
                .unwrap_or_else(|error| panic!("required test result: {error:?}")),
            );
        }
        assert_eq!(observations.len(), 29);
    })
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(calls, 721);
    println!(
        "construction-reference-native: {}",
        json!({"calls":calls,"rows":720,"usage":usage.map(|n|n.to_string())})
    );
}
fn raw(mut ordinal: usize, cells: usize, radix: i128) -> Vec<i128> {
    let mut r = vec![0; cells + 1];
    for c in r[1..].iter_mut().rev() {
        *c = (ordinal % radix as usize) as i128;
        ordinal /= radix as usize;
    }
    r[0] = ordinal as i128;
    r
}
fn decoded(r: &[i128], is_t: bool) -> Option<Value> {
    let mut entries = Vec::new();
    for (j, &cell) in r[1..].iter().enumerate() {
        if cell == 0 {
            continue;
        }
        let (d, p, t) = if is_t {
            ((cell - 1) / 2, (cell - 1) % 2, j as i128)
        } else {
            (j as i128, (cell - 1) % 2, (cell - 1) / 2)
        };
        if t >= r[0] || entries.iter().any(|e: &[i128; 3]| e[0] == d || e[2] == t) {
            return None;
        }
        entries.push([d, p, t]);
    }
    if entries.len() > 2 {
        return None;
    }
    entries.sort();
    Some(json!({"n":r[0],"entries":entries}))
}
#[test]
fn both_decoders_are_total_and_round_trip_the_whole_carrier() {
    let root = bundle();
    let relation = value(&root.join("reference/relation.json"));
    let states = relation["states"]
        .as_array()
        .unwrap_or_else(|| panic!("required test value missing"));
    assert_eq!(states.len(), 45);
    for (name, cells, radix, total, nulls) in [("T", 2, 7, 147, 102), ("I", 3, 5, 375, 330)] {
        let alpha = value(&root.join(name).join("alpha.json"));
        assert_eq!(
            alpha
                .as_array()
                .unwrap_or_else(|| panic!("required test value missing"))
                .len(),
            total
        );
        let mut covered = BTreeSet::new();
        let mut invalid = 0;
        for ordinal in 0..total {
            let r = raw(ordinal, cells, radix);
            match decoded(&r, name == "T") {
                None => {
                    invalid += 1;
                    assert!(alpha[ordinal].is_null());
                }
                Some(state) => {
                    let a = states
                        .iter()
                        .position(|s| *s == state)
                        .unwrap_or_else(|| panic!("required test value missing"));
                    assert_eq!(alpha[ordinal], a);
                    assert!(covered.insert(a));
                    let mut encoded = vec![0; cells + 1];
                    encoded[0] = state["n"]
                        .as_i64()
                        .map(i128::from)
                        .unwrap_or_else(|| panic!("required test value missing"));
                    for e in state["entries"]
                        .as_array()
                        .unwrap_or_else(|| panic!("required test value missing"))
                    {
                        let d = e[0]
                            .as_i64()
                            .map(i128::from)
                            .unwrap_or_else(|| panic!("required test value missing"));
                        let p = e[1]
                            .as_i64()
                            .map(i128::from)
                            .unwrap_or_else(|| panic!("required test value missing"));
                        let t = e[2]
                            .as_i64()
                            .map(i128::from)
                            .unwrap_or_else(|| panic!("required test value missing"));
                        let slot = if name == "T" { t } else { d };
                        encoded[slot as usize + 1] = 1 + 2 * if name == "T" { d } else { t } + p;
                    }
                    assert_eq!(encoded, r);
                }
            }
        }
        assert_eq!(covered.len(), 45);
        assert_eq!(invalid, nulls);
        assert_eq!(alpha[0], 0);
    }
}
fn expected_features(r: &[i128], q: i128, is_t: bool) -> Vec<i128> {
    let cells: Vec<_> = r[1..]
        .iter()
        .enumerate()
        .map(|(j, &c)| {
            let occupied = c != 0;
            let id = if is_t {
                if c == 0 { 0 } else { (c - 1) / 2 }
            } else if occupied {
                j as i128
            } else {
                0
            };
            let p = if occupied { (c - 1) % 2 } else { 0 };
            let t = if is_t {
                if occupied { j as i128 } else { 0 }
            } else if c >= 3 {
                1
            } else {
                0
            };
            (occupied, id, p, t)
        })
        .collect();
    let id = match q {
        0..=5 => q / 2,
        6..=8 => q - 6,
        9..=14 => (q - 9) / 2,
        _ => 0,
    };
    let p = if q < 6 {
        q % 2
    } else if (9..15).contains(&q) {
        (q - 9) % 2
    } else {
        0
    };
    let minimum = cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.0)
        .min_by_key(|(j, c)| (c.2, c.3, *j))
        .map_or(0, |(_, c)| c.1);
    let mut expected = r.to_vec();
    expected.extend([
        q,
        0,
        i128::from(q < 6),
        i128::from((6..9).contains(&q)),
        i128::from((9..15).contains(&q)),
        i128::from(q == 15),
        id,
        p,
        i128::from(cells.iter().any(|c| c.0 && c.1 == id)),
        cells.iter().filter(|c| c.0).count() as i128,
        minimum,
        (r[0] + 1).min(2),
        1 + 2 * if is_t { id } else { r[0].min(1) } + p,
    ]);
    for c in cells {
        expected.extend([
            i128::from(c.0),
            c.1,
            1 + 2 * if is_t { c.1 } else { c.3 } + p,
        ]);
    }
    expected
}
#[test]
fn correlated_columns_compile_over_five_and_six_native_fields() {
    let root = bundle();
    let (_, rules) = abstract_parts(&root);
    for (name, cells, radix, total, width) in [("T", 2, 7, 147, 22), ("I", 3, 5, 375, 26)] {
        let grammar = value(&root.join(name).join("grammar.json"));
        let columns = grammar["columns"]
            .as_array()
            .unwrap_or_else(|| panic!("required test value missing"));
        assert_eq!(columns.len(), width);
        let text = String::from_utf8(read(&root.join(name).join("project.zeno"), 1 << 20))
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let declarations = Declarations::read(&text, &rules.leaf_bindings)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let inputs = declarations
            .inputs()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(inputs.len(), cells + 3);
        let names: Vec<_> = (0..=cells)
            .map(|j| format!("pre.100.{}", 110 + j))
            .chain(["command.101.120".into(), "context.102.130".into()])
            .collect();
        assert_eq!(
            inputs.iter().map(|i| &i.name).collect::<Vec<_>>(),
            names.iter().collect::<Vec<_>>()
        );
        let mut variables: BTreeMap<String, Ast> = grammar["helpers"]
            .as_object()
            .unwrap_or_else(|| panic!("required test value missing"))
            .iter()
            .map(|(n, s)| {
                (
                    n.clone(),
                    expr::parse(
                        s.as_str()
                            .unwrap_or_else(|| panic!("required test value missing")),
                    )
                    .unwrap_or_else(|error| panic!("required test result: {error:?}")),
                )
            })
            .collect();
        for (j, input) in inputs.iter().enumerate() {
            variables.insert(
                columns[j]["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("required test value missing"))
                    .into(),
                Ast::Name(input.name.clone()),
            );
        }
        let mut graph = ScalarGraph::new(&declarations, &inputs);
        let mut roots = Vec::new();
        let mut outputs = Vec::new();
        for column in columns {
            let expanded = expr::expand(
                &Ast::Name(
                    column["name"]
                        .as_str()
                        .unwrap_or_else(|| panic!("required test value missing"))
                        .into(),
                ),
                &variables,
            )
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            let node = graph
                .compile(&expanded)
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            roots.push(
                u16::try_from(node.index)
                    .unwrap_or_else(|error| panic!("required test result: {error:?}")),
            );
            outputs.push(if column["kind"] == "Bool" {
                Domain::Bool
            } else {
                Domain::Int {
                    min: column["min"]
                        .as_i64()
                        .map(i128::from)
                        .unwrap_or_else(|| panic!("required test value missing")),
                    max: column["max"]
                        .as_i64()
                        .map(i128::from)
                        .unwrap_or_else(|| panic!("required test value missing")),
                }
            });
        }
        let mut domains = vec![Domain::Int { min: 0, max: 2 }];
        domains.extend(vec![
            Domain::Int {
                min: 0,
                max: radix - 1
            };
            cells
        ]);
        domains.extend([
            Domain::Int { min: 0, max: 15 },
            Domain::Int { min: 0, max: 0 },
        ]);
        let nodes = graph
            .table
            .nodes
            .iter()
            .map(policy::scalar_op)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let program = Program::try_new_extended(domains, outputs, nodes, roots)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(program.inputs().len(), cells + 3);
        for s in 0..total {
            let r = raw(s, cells, radix);
            for q in 0..16 {
                let mut tuple = r.clone();
                tuple.extend([q, 0]);
                assert_eq!(
                    program
                        .evaluate(&tuple)
                        .unwrap_or_else(|error| panic!("required test result: {error:?}")),
                    expected_features(&r, q, name == "T"),
                    "{name}/{s}/{q}"
                );
            }
        }
    }
}
#[test]
fn literal_reference_root_metadata_preserves_ordinary_compiler_bytes() {
    let root = bundle();
    let (declarations, rules) = abstract_parts(&root);
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let commitment = schema_commitment(&schema)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let ordinary = Contract::build(&declarations, &rules, commitment)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let (compiled, roots) = Contract::build_with_roots(&declarations, &rules, commitment)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        program_bytes(
            &ordinary
                .program()
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        program_bytes(
            &compiled
                .program()
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
    );
    assert_eq!(
        policy::encode(&ordinary, &schema)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        policy::encode(&compiled, &schema)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
    );
    assert_eq!(roots.len(), 1);
    assert!(matches!(
        roots[0].domain,
        ScalarDomain::Int { min: 0, max: 719 }
    ));
    let program = compiled
        .program()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(program.nodes().len(), 2221);
    assert_eq!(program.inputs().len(), 3);
    assert_eq!(program.outputs().len(), 1);
    let inputs = declarations
        .inputs()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut graph = ScalarGraph::new(&declarations, &inputs);
    let indices: Vec<_> = roots
        .iter()
        .map(|r| {
            graph
                .compile(&r.expression)
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
                .index
        })
        .collect();
    assert_eq!(indices, compiled.program_roots);
    assert_eq!(graph.table.nodes, compiled.nodes);
}
