//! `zeno-fcis check --expand`: the exact expansion of a project's tables and
//! collections.
//!
//! The command prints the source with every table form replaced by the
//! declarations and formulas it expands to, and every collection declaration
//! by its view. It parses that text again and refuses to print it unless its
//! canonical project is the original's view, so the printed text is the
//! project every check of the original decides. It also reports the size of
//! the expanded input domain, which the finite checks enumerate.

use std::fs;
use std::path::Path;

use serde_json::json;
use zeno_fcis_spec::{
    DiagnosticSet, ProjectLimits, ProjectSpec, ProjectionPath, ProjectionRoot, SourceLimits,
    StableId, TableExpansion, TypeKind, declared_table_index_range, elaborate_project,
    expand_tables, parse_project,
};

use crate::{FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, print_json};

/// The input tuples a finite contract check enumerates at most by default.
const FINITE_CHECK_CAP: u128 = 1 << 20;

struct CheckedExpansion {
    expansion: TableExpansion,
    inputs: Vec<(String, Option<u128>)>,
    domain: Option<u128>,
    entries: u32,
}

enum ExpansionError {
    Diagnostics(DiagnosticSet),
    CanonicalMismatch,
}

fn checked_expansion(
    source: &str,
    limits: SourceLimits,
) -> Result<CheckedExpansion, ExpansionError> {
    let elaborated = |text: &str| {
        parse_project(text, limits)
            .and_then(|parsed| elaborate_project(parsed, ProjectLimits::default()))
    };
    let expansion = expand_tables(source, limits).map_err(ExpansionError::Diagnostics)?;
    let original = elaborated(source).map_err(ExpansionError::Diagnostics)?;
    let same = elaborated(&expansion.text).ok().and_then(|expanded| {
        Some(expanded.canonical_bytes().ok()? == original.view().canonical_bytes().ok()?)
    });
    if same != Some(true) {
        return Err(ExpansionError::CanonicalMismatch);
    }
    let inputs = inputs(&original);
    let domain = inputs
        .iter()
        .try_fold(1u128, |product, (_, size)| product.checked_mul((*size)?));
    let entries = expansion.tables.iter().map(|table| table.size).sum();
    Ok(CheckedExpansion {
        expansion,
        inputs,
        domain,
        entries,
    })
}

pub(crate) fn expand(path: &Path, format: OutputFormat) -> u8 {
    let limits = SourceLimits::default();
    let source =
        match fs::File::open(path).and_then(|file| crate::read_project_source(file, limits)) {
            Ok(source) => source,
            Err(error) => {
                let message = format!("read {}: {error}", path.display());
                crate::print_command_error(path, "project-read-failed", &message, format);
                return FAILURE;
            }
        };
    let checked = match checked_expansion(&source, limits) {
        Ok(checked) => checked,
        Err(ExpansionError::Diagnostics(set)) => {
            crate::print_diagnostics(path, &set, format);
            return INVALID;
        }
        Err(ExpansionError::CanonicalMismatch) => {
            let message = "the expanded text does not parse to the same project; nothing printed";
            crate::print_command_error(path, "expansion-mismatch", message, format);
            return FAILURE;
        }
    };
    let CheckedExpansion {
        expansion,
        inputs,
        domain,
        entries,
    } = checked;
    match format {
        OutputFormat::Human => {
            print!("{}", expansion.text);
            eprintln!(
                "expanded {}: tables={} entries={entries} inputs={} domain={}",
                path.display(),
                expansion.tables.len(),
                inputs.len(),
                domain.map_or("unknown (an input has no declared range)".to_owned(), |n| n
                    .to_string()),
            );
            for count in &expansion.counts {
                eprintln!(
                    "count at line {} column {}: {} {} per expansion, expanded {} times",
                    count.line,
                    count.column,
                    count.clauses,
                    if count.linear {
                        "Select terms"
                    } else {
                        "clauses"
                    },
                    count.expansions
                );
            }
            if domain.is_some_and(|n| n > FINITE_CHECK_CAP) {
                eprintln!(
                    "note: the finite contract checks enumerate at most {FINITE_CHECK_CAP} input tuples by default; beyond it review reports bounded probes and the judges are inconclusive"
                );
            }
        }
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "expanded", "authority": "none",
            "runtime_admission": "not-run", "path": path.display().to_string(),
            "tables": expansion.tables.iter().map(|table| json!({
                "id": table.id, "owner": table.owner, "name": table.name,
                "field_type": table.field_type, "size": table.size,
                "fields": [table.id, table.id + (table.size - 1)],
            })).collect::<Vec<_>>(),
            "inputs": inputs.iter().map(|(name, size)| json!({
                "input": name, "values": size.map(|n| n.to_string()),
            })).collect::<Vec<_>>(),
            "counts": expansion.counts.iter().map(|count| json!({
                "line": count.line, "column": count.column,
                "clauses": count.clauses.to_string(), "expansions": count.expansions,
                "form": if count.linear { "linear-select" } else { "combinations" },
            })).collect::<Vec<_>>(),
            "domain_size": domain.map(|n| n.to_string()),
            "finite_check_cap": FINITE_CHECK_CAP.to_string(),
            "canonical_match": true,
            "expansion": expansion.text,
        })),
    }
    OK
}

/// Every decision input in program order, the state fields, then the command
/// and the context, with its number of values when `project.zeno` alone
/// determines it.
fn inputs(spec: &ProjectSpec) -> Vec<(String, Option<u128>)> {
    let mut out = Vec::new();
    for (root, id, name) in [
        (ProjectionRoot::Pre, 100, "pre"),
        (ProjectionRoot::Command, 101, "command"),
        (ProjectionRoot::Context, 102, "context"),
    ] {
        let Some(owner) = StableId::new(id) else {
            continue;
        };
        let fields: Vec<StableId> = spec
            .fields()
            .iter()
            .filter(|field| field.owner() == owner)
            .map(|field| field.id())
            .collect();
        let paths: Vec<Vec<StableId>> = if fields.is_empty() {
            vec![vec![owner]]
        } else {
            fields.into_iter().map(|field| vec![owner, field]).collect()
        };
        for segments in paths {
            let label = format!(
                "{name}.{}",
                segments
                    .iter()
                    .map(|s| s.get().to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            );
            let size = ProjectionPath::try_new(root, segments).and_then(|path| values(spec, &path));
            out.push((label, size));
        }
    }
    out
}

/// The number of values at `path`: a declared range, a bool, or the
/// payload-free variants of an enumerated type.
fn values(spec: &ProjectSpec, path: &ProjectionPath) -> Option<u128> {
    let leaf = leaf(spec, path)?;
    let variants: Vec<_> = spec
        .variants()
        .iter()
        .filter(|v| v.owner() == leaf)
        .collect();
    if !variants.is_empty() {
        return variants
            .iter()
            .all(|variant| variant.payload_type().is_none())
            .then_some(variants.len() as u128);
    }
    let declared = spec.types().iter().find(|t| t.id() == leaf)?;
    match declared.kind() {
        TypeKind::Bool => Some(2),
        TypeKind::Int | TypeKind::U128 => declared_table_index_range(spec, path)?.cardinality(),
        _ => None,
    }
}

fn leaf(spec: &ProjectSpec, path: &ProjectionPath) -> Option<StableId> {
    let (first, rest) = path.segments().split_first()?;
    let mut leaf = *first;
    for segment in rest {
        leaf = spec
            .fields()
            .iter()
            .find(|field| field.owner() == leaf && field.id() == *segment)?
            .field_type();
    }
    Some(leaf)
}
