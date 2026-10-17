//! The receipt is canonical and replays; every tampered count, digest,
//! identity, pin and source is refused; and each table digest is the
//! digest of the Authority's typed outcomes.

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition as c};

use super::leaf::{self, sha256};
use super::manifest::Manifest;
use super::receipt::canonical;
use super::table::{Table, TableKind};
use super::tests::{Ring, assert_refused, fail};
use super::{
    AuthorityRefusal, CompositionSources, RECEIPT_SCHEMA, ReceiptForm, ReplayRefusal, hex, replay,
};
use crate::internals::model::Contract;
use crate::internals::policy;
use crate::internals::review::evaluate::Framer;

fn replay_ring(ring: &Ring, receipt: &str) -> Result<super::CheckedComposition, ReplayRefusal> {
    let manifest = ring.manifest_text();
    replay(
        receipt,
        CompositionSources {
            manifest: &manifest,
            leaves: &ring.leaves,
            aggregate: None,
        },
    )
}

fn receipt_of(ring: &Ring) -> String {
    ring.check().unwrap_or_else(fail).receipt()
}

#[test]
fn the_receipt_is_canonical_and_replays_byte_for_byte() {
    let ring = Ring::new(2);
    let receipt = receipt_of(&ring);
    assert!(receipt.ends_with("}\n") && !receipt.contains("\n{"));
    let value: Value = serde_json::from_str(&receipt).unwrap_or_else(fail);
    assert_eq!(canonical(&value), receipt);
    assert_eq!(value["schema"], RECEIPT_SCHEMA);
    let result = &value["result"];
    for key in [
        "manifest",
        "checker",
        "composition_spec",
        "leaves",
        "totals",
    ] {
        assert!(result.get(key).is_some(), "{key}");
    }
    assert!(result["checker"]["sources"]["evaluator"].is_string());
    for leaf in result["leaves"]
        .as_array()
        .unwrap_or_else(|| fail("leaves"))
    {
        for table in ["local_sha256", "state_fact_sha256", "interface_sha256"] {
            assert_eq!(
                leaf["tables"][table].as_str().map(str::len),
                Some(64),
                "{table}"
            );
        }
    }
    let replayed = replay_ring(&ring, &receipt).unwrap_or_else(fail);
    assert_eq!(replayed.receipt(), receipt);
    // A second run writes the same bytes.
    assert_eq!(receipt_of(&ring), receipt);
}

/// The receipt with one value replaced, written canonically.
fn tampered(receipt: &str, path: &[&str], value: Value) -> String {
    let mut document: Value = serde_json::from_str(receipt).unwrap_or_else(fail);
    let mut slot = &mut document;
    for key in path {
        slot = match key.parse::<usize>() {
            Ok(index) => &mut slot[index],
            Err(_) => &mut slot[*key],
        };
    }
    assert!(!slot.is_null(), "{path:?} names a value");
    *slot = value;
    canonical(&document)
}

#[test]
fn a_tampered_receipt_is_refused_at_its_first_difference() {
    let ring = Ring::new(2);
    let receipt = receipt_of(&ring);
    let zero = json!("0".repeat(64));
    for (path, value, expected) in [
        (
            vec!["result", "totals", "local_rows"],
            json!(641),
            "result.totals.local_rows",
        ),
        (
            vec!["result", "edges", "0", "facts"],
            json!([1, 2]),
            "result.edges[0].facts[1]",
        ),
        (
            vec!["result", "leaves", "0", "tables", "local_sha256"],
            zero.clone(),
            "result.leaves[0].tables.local_sha256",
        ),
        (
            vec!["result", "leaves", "1", "tables", "interface_sha256"],
            zero.clone(),
            "result.leaves[1].tables.interface_sha256",
        ),
        (
            vec!["result", "leaves", "0", "bindings", "program_sha256"],
            zero.clone(),
            "result.leaves[0].bindings.program_sha256",
        ),
        (
            vec!["result", "checker", "source_digest"],
            zero.clone(),
            "result.checker.source_digest",
        ),
        (
            vec!["result", "checker", "sources", "evaluator"],
            zero.clone(),
            "result.checker.sources.evaluator",
        ),
        (
            vec!["result", "manifest", "commitment"],
            zero.clone(),
            "result.manifest.commitment",
        ),
        (
            vec!["result", "composition_spec", "commitment"],
            zero,
            "result.composition_spec.commitment",
        ),
    ] {
        let forged = tampered(&receipt, &path, value);
        match replay_ring(&ring, &forged) {
            Err(ReplayRefusal::Mismatch {
                path, recomputed, ..
            }) => {
                assert_eq!(path, expected);
                assert!(!recomputed.is_null() || expected.ends_with("[1]"), "{path}");
            }
            other => panic!("{expected}: expected a mismatch, got {other:?}"),
        }
    }
}

#[test]
fn changed_sources_do_not_replay_an_old_receipt() {
    let ring = Ring::new(2);
    let receipt = receipt_of(&ring);
    // A changed manifest that is still valid: its bytes and commitment differ.
    let mut renamed = ring.clone();
    renamed.manifest["name"] = json!("register-ring-renamed");
    match replay_ring(&renamed, &receipt) {
        Err(ReplayRefusal::Mismatch { path, .. }) => {
            assert!(path.starts_with("result.manifest."), "{path}");
        }
        other => panic!("expected a mismatch, got {other:?}"),
    }
    // A changed leaf file is refused against its pin before any comparison.
    let mut changed = ring.clone();
    changed.leaves[1].project = changed.leaves[1].project.replace("in 0..=7", "in 0..=8");
    match replay_ring(&changed, &receipt) {
        Err(ReplayRefusal::Check(refusal)) => {
            assert_refused(&refusal, "binding", "source binding");
        }
        other => panic!("expected the check's refusal, got {other:?}"),
    }
    // A re-pinned changed leaf binds another program: another receipt.
    changed.repin(1);
    match replay_ring(&changed, &receipt) {
        Err(ReplayRefusal::Check(_) | ReplayRefusal::Mismatch { .. }) => {}
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_receipt_that_is_not_canonical_is_refused_before_the_check() {
    let ring = Ring::new(2);
    let receipt = receipt_of(&ring);
    let value: Value = serde_json::from_str(&receipt).unwrap_or_else(fail);
    let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(fail) + "\n";
    let mut extra = value.clone();
    extra["note"] = json!("x");
    let mut schema = value.clone();
    schema["schema"] = json!("zeno-fcis/checked-composition-receipt/2");
    for (text, form) in [
        ("not json".to_owned(), ReceiptForm::NotJson),
        (pretty, ReceiptForm::NotCanonical),
        (receipt.trim_end().to_owned(), ReceiptForm::NotCanonical),
        (canonical(&extra), ReceiptForm::Schema),
        (canonical(&schema), ReceiptForm::Schema),
        (canonical(&json!([value])), ReceiptForm::Schema),
    ] {
        assert_eq!(
            replay_ring(&ring, &text).err(),
            Some(ReplayRefusal::Malformed(form))
        );
    }
}

/// Rebuilds a leaf's local table from the Authority's own outcomes, with
/// each refusal encoded from its typed value, as an independent account of
/// the digest the receipt records.
#[test]
fn the_local_table_digest_is_the_digest_of_typed_outcomes() {
    let ring = Ring::new(2);
    let checked = ring.check().unwrap_or_else(fail);
    let manifest = Manifest::read(&ring.manifest_text()).unwrap_or_else(fail);
    for (index, certificate) in checked.leaves().iter().enumerate() {
        let entry = &manifest.leaves[index];
        let files = &ring.leaves[index];
        let compiled = leaf::compile(entry, &files.project, &files.rules).unwrap_or_else(fail);
        let contract =
            Contract::build(&compiled.declarations, &compiled.rules, compiled.commitment)
                .unwrap_or_else(fail);
        let framer = Framer::new(&compiled.positions, &contract);
        let values: Vec<Vec<i128>> = compiled
            .positions
            .iter()
            .map(|position| {
                position
                    .domain
                    .values_within(leaf::MAX_LOCAL_ROWS)
                    .unwrap_or_else(|| fail("domain"))
            })
            .collect();
        let (digest, refused) = policy::with_authority(&contract, &compiled.schema, |authority| {
            let mut table = Table::new(TableKind::Local);
            let mut refused = Vec::new();
            leaf::for_each_tuple(&values, |tuple| {
                let pre = framer.fields(&framer.roots[0], tuple);
                let mut input: Vec<c::Field<'static>> = framer
                    .fields(&framer.roots[1], tuple)
                    .into_iter()
                    .chain(framer.fields(&framer.roots[2], tuple))
                    .collect();
                input.sort_by_key(|field| field.id);
                let holds = |truth: super::predicate::Truth| {
                    matches!(truth, super::predicate::Truth::Holds)
                };
                let pre_holds = holds(compiled.invariant.on_state(&pre));
                let assumed = holds(compiled.assumption.on_input(&input));
                let frames = framer.frame(tuple);
                let outcome = authority.publish(c::Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                });
                let record = table.row(tuple);
                record.extend_from_slice(&[u8::from(pre_holds), u8::from(assumed)]);
                match &outcome {
                    PublicationOutcome::Commit(publication) => {
                        let class = publication
                            .evaluation()
                            .result()
                            .map(|candidate| candidate.class())
                            .unwrap_or_else(fail);
                        record.push(if class == c::Class::Accept { 0 } else { 2 });
                        record.extend_from_slice(&sha256(publication.subject()));
                    }
                    PublicationOutcome::Reject(_) => {
                        record.push(1);
                        let subject = outcome.evaluation().subject().unwrap_or_default();
                        record.extend_from_slice(&sha256(subject));
                    }
                    PublicationOutcome::Refused { evaluation, error } => {
                        record.push(3);
                        let typed = AuthorityRefusal::of(error, evaluation.diagnostics());
                        typed.encode(record);
                        refused.push(typed);
                    }
                    _ => fail("an unknown outcome"),
                }
                Ok::<(), ()>(())
            })
            .unwrap_or_else(fail);
            (table.digest(), refused)
        })
        .unwrap_or_else(fail);
        assert_eq!(
            hex(&digest),
            hex(&certificate.local_table_sha256()),
            "leaf {index}"
        );
        // Law 500 refuses each copy of an unsafe peer, by its law ID and kind.
        assert!(!refused.is_empty());
        assert!(
            refused
                .iter()
                .all(|typed| matches!(typed, AuthorityRefusal::Law { law: Some(500), .. }))
        );
    }
}
