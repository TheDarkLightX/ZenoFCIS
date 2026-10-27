//! Native access to the production structural-admission predicate and odometer.
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

use serde::Deserialize;

use super::{EntryView, Shape, SummaryView, TallyView, next};
use crate::internals::declarations::Source;
use crate::internals::review::domain::{LeafDomain, Position};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainWire {
    least: u32,
    greatest: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct KeyWire {
    position: usize,
    key_type: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EntryWire {
    map_id: u32,
    key: usize,
    presence: usize,
    fields: Vec<usize>,
    stored_keys: Vec<KeyWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SummaryWire {
    map_id: u32,
    position: usize,
    field: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TallyWire {
    target: u32,
    field: usize,
    source: u32,
    group: usize,
    weight: Option<usize>,
    selector: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeWire {
    domains: Vec<DomainWire>,
    keys: Vec<KeyWire>,
    entries: Vec<EntryWire>,
    summaries: Vec<SummaryWire>,
    tallies: Vec<TallyWire>,
}

#[test]
#[ignore = "requires the exhaustive shape stream supplied by check_collections_reduction.py"]
fn admissible_listing() -> Result<(), Box<dyn std::error::Error>> {
    let input = BufReader::new(File::open(std::env::var("CRED_SHAPES")?)?);
    let mut output = BufWriter::new(File::create(std::env::var("CRED_LISTING")?)?);
    let mut shapes = 0u64;
    let mut tuples = 0u64;
    let mut admitted = 0u64;
    for (index, line) in input.lines().enumerate() {
        let wire: ShapeWire = serde_json::from_str(&line?)?;
        let positions: Vec<_> = wire
            .domains
            .iter()
            .map(|domain| Position {
                name: String::new(),
                source: Source::State,
                field: None,
                type_id: 0,
                domain: LeafDomain::Int {
                    min: i128::from(domain.least),
                    max: i128::from(domain.greatest),
                },
            })
            .collect();
        let keys = wire
            .keys
            .iter()
            .map(|key| (key.position, key.key_type))
            .collect();
        let entries: Vec<_> = wire
            .entries
            .iter()
            .map(|entry| EntryView {
                id: 0,
                map: entry.map_id,
                key: entry.key,
                presence: entry.presence,
                values: entry.fields.clone(),
                stored_keys: entry
                    .stored_keys
                    .iter()
                    .map(|key| (key.position, key.key_type))
                    .collect(),
            })
            .collect();
        let summaries: Vec<_> = wire
            .summaries
            .iter()
            .map(|summary| SummaryView {
                id: 0,
                map: summary.map_id,
                position: summary.position,
                field: summary.field,
                nonnegative: true,
            })
            .collect();
        let tallies: Vec<_> = wire
            .tallies
            .iter()
            .map(|tally| TallyView {
                id: 0,
                target: tally.target,
                field: tally.field,
                source: tally.source,
                group: tally.group,
                weight: tally.weight,
                selector: tally.selector,
            })
            .collect();
        let shape = Shape {
            positions: &positions,
            keys: &keys,
            entries: &entries,
            summaries: &summaries,
            tallies: &tallies,
            ordered: &Default::default(),
            order_reads: &[],
        };
        let values: Vec<Vec<i128>> = wire
            .domains
            .iter()
            .map(|domain| (domain.least..=domain.greatest).map(i128::from).collect())
            .collect();
        let mut tuple: Vec<_> = wire.domains.iter().map(|d| i128::from(d.least)).collect();
        let mut digits = vec![0; values.len()];
        write!(output, "{index}:")?;
        loop {
            tuples += 1;
            if shape.admissible(&tuple) {
                admitted += 1;
                for value in &tuple {
                    write!(output, "{value}")?;
                }
                output.write_all(b",")?;
            }
            if !next(&values, &mut digits, &mut tuple) {
                break;
            }
        }
        output.write_all(b"\n")?;
        shapes += 1;
    }
    output.flush()?;
    let counts = serde_json::json!({"schema":"zeno-fcis/collections-reduction-counts/1",
        "shapes":shapes,"tuples":tuples,"admitted":admitted});
    std::fs::write(std::env::var("CRED_COUNTS")?, format!("{counts}\n"))?;
    println!("{counts}");
    Ok(())
}
