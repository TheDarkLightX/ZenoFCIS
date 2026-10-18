//! Complete, owner-side arithmetic capture. This is a checked observation of a
//! frozen reference, not an application decision or a persisted preparation
//! handle. Acquisition pages and exact lookup pieces have distinct bounds.

use super::*;
use zeno_fcis_loop_doors::subject::{domain_size, tuples};
use zeno_fcis_synthesis::finite::Domain;

#[derive(clap::Args)]
pub(crate) struct Options {
    #[arg(long)]
    spec: PathBuf,
    /// A contained python-function-v1 reference, owned and read only for this call.
    #[arg(long)]
    reference: PathBuf,
    #[arg(
        long,
        conflicts_with = "expressions",
        required_unless_present = "expressions"
    )]
    candidate_json: Option<PathBuf>,
    #[arg(long)]
    expressions: Option<PathBuf>,
    /// Distinct points per acquisition page; the existing 4096 cap remains.
    #[arg(long, default_value_t = 4096)]
    max_tuples: u64,
    /// Cumulative node visits, including lookup consistency, check and replay.
    /// The existing 10,000,000 cap remains; a partial cover is inconclusive.
    #[arg(long, default_value_t = 2_000_000)]
    node_visits: u64,
}

// Small lookup pieces keep even irregular arithmetic tables within MAX_NODES.
// The page, source, protocol, control-frame and invocation limits stay intact.
const LOOKUP_POINTS: u64 = 16;

/// An exclusively owned invocation builder. It cannot issue a completed result
/// before its contiguous cursor reaches the complete declared finite product.
struct WholeCheck<'a> {
    candidate: &'a Program,
    total: u64,
    page_limit: u64,
    work_limit: u64,
    next: u64,
    reserved: u64,
    pieces: u64,
    stopped: bool,
    table_digest: String,
    receipt_digest: String,
}

impl<'a> WholeCheck<'a> {
    fn new(candidate: &'a Program, page_limit: u64, work_limit: u64) -> Result<Self, String> {
        Budget {
            max_tuples: page_limit,
            rounds: 0,
            max_examples: 0,
            node_visits: work_limit,
        }
        .validate()?;
        // This explicit arithmetic cover keeps input kinds unchanged when an
        // interval is restricted. Boolean-input programs retain the ordinary
        // capture-session path; they are not silently coerced to integers.
        if candidate
            .inputs()
            .iter()
            .any(|d| !matches!(d, Domain::Int { min, .. } if *min >= 0))
        {
            return Err("capture-whole-requires-nonnegative-integer-inputs".into());
        }
        let total =
            u64::try_from(tuples(candidate.inputs())).map_err(|_| "capture-whole-domain-budget")?;
        if page_limit == 0 || total == 0 {
            return Err("capture-whole-domain-budget".into());
        }
        let pages = total.div_ceil(page_limit);
        // Two acquisition pages per search stage, one check stage per page.
        // Reserve the existing supervisor's complete frame bound up front.
        if pages.saturating_add(pages.div_ceil(2)).saturating_mul(2)
            > u64::from(super::super::FRAME_BUDGET)
        {
            return Err("capture-whole-domain-budget".into());
        }
        let binding = encode(candidate).map_err(|_| "capture-candidate-encoding")?;
        let seed = sha256_hex(&binding);
        Ok(Self {
            candidate,
            total,
            page_limit,
            work_limit,
            next: 0,
            reserved: 0,
            pieces: 0,
            stopped: false,
            table_digest: seed.clone(),
            receipt_digest: seed,
        })
    }

    fn point(&self, mut ordinal: u64) -> Result<Vec<i128>, String> {
        if ordinal >= self.total {
            return Err("capture-whole-ordinal".into());
        }
        let mut point = vec![0; self.candidate.inputs().len()];
        for (slot, domain) in point.iter_mut().zip(self.candidate.inputs()).rev() {
            let width =
                u64::try_from(domain_size(*domain)).map_err(|_| "capture-whole-domain-budget")?;
            *slot = i128::try_from(i128::from(domain.bounds().0) + i128::from(ordinal % width))
                .map_err(|_| "capture-whole-ordinal")?;
            ordinal /= width;
        }
        Ok(point)
    }

    fn points(&self, start: u64) -> Result<Vec<Vec<i128>>, String> {
        let end = start.saturating_add(self.page_limit).min(self.total);
        (start..end).map(|ordinal| self.point(ordinal)).collect()
    }

    /// Accept exactly the next complete page. No skip, overlap or partial page
    /// can advance the cursor. Every exact piece is a rectangle in the original
    /// input kinds; only interval bounds change, never operations or roots.
    fn page(&mut self, start: u64, rows: &[Vec<i128>]) -> Result<Option<Value>, String> {
        let expected = self.page_limit.min(self.total.saturating_sub(self.next));
        if self.stopped || start != self.next || expected == 0 || rows.len() as u64 != expected {
            return Err("capture-whole-page-coverage".into());
        }
        let table_bytes = serde_json::to_vec(rows).map_err(|_| "capture-table-json")?;
        extend_digest(&mut self.table_digest, &table_bytes);
        let tail = self
            .candidate
            .inputs()
            .last()
            .map_or(1, |d| domain_size(*d));
        let tail = u64::try_from(tail).map_err(|_| "capture-whole-domain-budget")?;
        let mut offset = 0usize;
        while offset < rows.len() {
            let point = self.point(self.next)?;
            let count = LOOKUP_POINTS
                .min(tail - self.next % tail)
                .min((rows.len() - offset) as u64);
            let mut count = usize::try_from(count).map_err(|_| "capture-whole-page-coverage")?;
            let (table, reference, inputs) = loop {
                let mut inputs = point
                    .iter()
                    .map(|value| Domain::Int {
                        min: *value,
                        max: *value,
                    })
                    .collect::<Vec<_>>();
                if let Some(Domain::Int { max, .. }) = inputs.last_mut() {
                    *max = max
                        .checked_add(
                            i128::try_from(count - 1).map_err(|_| "capture-whole-ordinal")?,
                        )
                        .ok_or("capture-whole-ordinal")?;
                }
                let table = proposal::ReferenceTable::new(
                    inputs.clone(),
                    self.candidate.outputs().to_vec(),
                    rows[offset..offset + count].to_vec(),
                )
                .map_err(|e| e.0)?;
                match table.program() {
                    Ok(reference) => break (table, reference, inputs),
                    Err(error) if error.0 == "reference-encoding-budget" && count > 1 => {
                        count = count.div_ceil(2)
                    }
                    Err(error) => return Err(error.0),
                }
            };
            let candidate = Program::try_new(
                inputs,
                self.candidate.outputs().to_vec(),
                self.candidate.nodes().to_vec(),
                self.candidate.roots().to_vec(),
            )
            .map_err(|_| "capture-whole-candidate-restriction")?;
            // One table consistency evaluation and two exact evaluations (the
            // comparison and its receipt replay), plus a possible diagnostic.
            let cost = (count as u64)
                .saturating_mul(
                    3 * reference.nodes().len() as u64 + 2 * candidate.nodes().len() as u64,
                )
                .saturating_add(candidate.nodes().len() as u64);
            if self.reserved.saturating_add(cost) > self.work_limit {
                self.stopped = true;
                return Err("capture-whole-work-budget".into());
            }
            self.reserved += cost;
            if !table
                .points()
                .iter()
                .zip(table.rows())
                .all(|(p, r)| reference.evaluate(p).ok().as_ref() == Some(r))
            {
                self.stopped = true;
                return Err("capture-table-encoding-defect".into());
            }
            let original = encode(&reference).map_err(|_| "capture-reference-encoding")?;
            let encoded = encode(&candidate).map_err(|_| "capture-candidate-encoding")?;
            let (check, receipt) = compare_exact(&original, &encoded);
            if check["verdict"] == "counterexample" {
                self.stopped = true;
                let ordinal = crate::report_numbers::read(&check["ordinal"])
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or("capture-whole-checker-witness")?;
                let at = usize::try_from(ordinal).map_err(|_| "capture-whole-checker-witness")?;
                let input = table
                    .points()
                    .get(at)
                    .ok_or("capture-whole-checker-witness")?;
                let actual = match candidate.evaluate(input) {
                    Ok(output) => {
                        json!({"status":"returned","output":crate::report_numbers::integers(&output)})
                    }
                    Err(error) => json!({"status":"refused","reason":format!("{error:?}")}),
                };
                return Ok(Some(
                    json!({"ordinal":self.next + ordinal,"input":crate::report_numbers::integers(&input),
                    "expected":crate::report_numbers::integers(&table.rows()[at]),"actual":actual,"check":check}),
                ));
            }
            if check["verdict"] != "equivalent" || check["replay"] != "matched" || receipt.is_none()
            {
                self.stopped = true;
                return Err(if check["verdict"] == "inconclusive" {
                    "capture-whole-work-budget"
                } else {
                    "capture-whole-check-or-replay-refused"
                }
                .into());
            }
            let receipt = serde_json::to_vec(&receipt).map_err(|_| "capture-receipt-json")?;
            extend_digest(&mut self.receipt_digest, &receipt);
            self.next += count as u64;
            self.pieces += 1;
            offset += count;
        }
        Ok(None)
    }

    fn detail(&self) -> Value {
        json!({"coverage":{"declared":self.total,"checked":self.next,"complete":!self.stopped && self.next==self.total,
                "order":transform::ENUMERATION,"acquisition_page_limit":self.page_limit,"lookup_point_limit":LOOKUP_POINTS},
            "decided_by":"complete-cover-transform-check-and-replay","checker":transform::CHECKER,
            "pieces_checked_and_replayed":self.pieces,"reserved_node_visits":self.reserved,
            "node_visit_limit":self.work_limit,"table_stream_sha256":self.table_digest,
            "receipt_stream_sha256":self.receipt_digest})
    }

    fn finish(&self) -> Result<Value, String> {
        if self.stopped || self.next != self.total {
            return Err("capture-whole-incomplete".into());
        }
        Ok(self.detail())
    }
}

fn extend_digest(chain: &mut String, bytes: &[u8]) {
    let mut framed = chain.as_bytes().to_vec();
    framed.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    framed.extend_from_slice(bytes);
    *chain = sha256_hex(&framed);
}

pub(super) fn run(options: Options, stages: &mut super::super::Stages) -> (u8, Value) {
    let mut reference_work = ReferenceWork::default();
    let mut progress = json!({});
    let mut bindings = json!({});
    let outcome: Result<(String, Value), String> = (|| {
        Budget {
            max_tuples: options.max_tuples,
            rounds: 0,
            max_examples: 0,
            node_visits: options.node_visits,
        }
        .validate()?;
        let spec = strict_json(&read(&options.spec, FILE_LIMIT)?)?;
        let skeleton = spec_program(&spec)?;
        let path = options
            .candidate_json
            .as_ref()
            .or(options.expressions.as_ref())
            .ok_or("capture-candidate-required")?;
        let candidate = proposed_program_for_abi(
            &read(path, FILE_LIMIT)?,
            options.expressions.is_some(),
            &skeleton,
        )?;
        if candidate.inputs() != skeleton.inputs() || candidate.outputs() != skeleton.outputs() {
            return Err("capture-candidate-abi".into());
        }
        let source = String::from_utf8(read(&options.reference, FILE_LIMIT)?)
            .map_err(|_| "capture-source-encoding")?;
        let mut cover = WholeCheck::new(&candidate, options.max_tuples, options.node_visits)?;
        bindings = json!({"source_sha256":sha256_hex(source.as_bytes()),
            "candidate_sha256":sha256_hex(&encode(&candidate).map_err(|_| "capture-candidate-encoding")?),"abi":spec});
        progress = cover.detail();
        while cover.next < cover.total {
            let start = cover.next;
            // At most two existing-size pages per stage keeps a full 41^3
            // cover within the existing 62-frame supervisor budget.
            let pages = stages.run(true, || {
                let mut pages = Vec::new();
                for start in [start, start.saturating_add(cover.page_limit)] {
                    if start >= cover.total {
                        break;
                    }
                    let points = cover.points(start)?;
                    let rows =
                        observed_table_with(&skeleton, &points, &mut reference_work, |schedule| {
                            runtime::invoke(&source, schedule)
                        })?;
                    pages.push((start, rows));
                }
                Ok::<_, String>(pages)
            })??;
            for (start, rows) in pages {
                let checked = stages.run(false, || cover.page(start, &rows));
                progress = cover.detail();
                if let Some(difference) = checked?? {
                    progress["difference"] = difference;
                    return Ok(("refuted".into(), progress.clone()));
                }
            }
        }
        Ok(("captured-exact".into(), cover.finish()?))
    })();
    let (code, mut result) = match outcome {
        Ok((status, detail)) => report(&status, detail),
        Err(reason) => {
            progress["reason"] = json!(reason);
            // Even a late deadline/control failure cannot label partial work exact.
            progress["coverage"]["complete"] = json!(false);
            report(
                if budget_stop(&reason) {
                    "inconclusive"
                } else {
                    "refused"
                },
                progress,
            )
        }
    };
    if let Some(bindings) = bindings.as_object() {
        for (name, value) in bindings {
            result["detail"][name] = value.clone();
        }
    }
    result["detail"]["exact_subject"] =
        json!("partitioned observations over the declared finite domain");
    result["detail"]["assumption"] =
        json!("reference is a pure deterministic function of all declared inputs");
    result["detail"]["reference_work"] = json!({"reservation":reference_work,"worker_internal_work":"not-measured","partial_failed_schedule":"unknown"});
    (code, result)
}

#[cfg(test)]
#[path = "loop_capture_whole_tests.rs"]
mod tests;
