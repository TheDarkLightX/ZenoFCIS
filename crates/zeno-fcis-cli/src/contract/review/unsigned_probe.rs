//! Exact count queries on numeric decisions; the observed law cannot decide.
use super::super::probe::Answer;
use super::*;

pub(super) fn count_query(
    project: &str,
    text: &str,
    condition: &str,
    max: u64,
) -> Result<Answer, ContractError> {
    let rules = Rules::read_for_project(project, text)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let bytes = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&bytes)?)?;
    let space = Space::new(&contract, max)?;
    if !space.full {
        return Ok(Answer::Inconclusive(Inconclusive {
            reason: "domain-exceeds-limit",
            size: space.size,
            limit: max,
        }));
    }
    let id = declarations
        .laws
        .iter()
        .map(|l| l.id)
        .chain([990, 991, rules.failure_law, rules.reject_law])
        .max()
        .unwrap_or(1000)
        .checked_add(1)
        .ok_or_else(|| error("probe ID overflow"))?;
    let applicability =
        zeno_fcis_spec::LawApplicability::new(zeno_fcis_spec::LawScope::Always, false);
    let parsed = zeno_fcis_spec::parse_law_formula(
        project,
        condition,
        applicability,
        zeno_fcis_spec::SourceLimits::default(),
    )
    .map_err(|d| error(d.to_string()))?;
    let formula = super::super::super::expr::law(&parsed).map_err(error)?;
    let mut extended_rules = rules.clone();
    extended_rules
        .law_kinds
        .insert(id, LawKind::AuthoritySubjectRecipient);
    let mut extended = Declarations::read(project, &rules.leaf_bindings)?;
    extended.laws.push(super::super::super::declarations::Law {
        id,
        name: "numeric_probe_condition".to_owned(),
        scope: applicability.scope(),
        genesis: false,
        formula,
    });
    let extended_bytes = schema::encode(&extended)?;
    let query = Contract::build(
        &extended,
        &extended_rules,
        schema_commitment(&extended_bytes)?,
    )?;
    policy::with_numeric_authority(&contract, &bytes, |authority| {
        policy::with_numeric_authority(&query, &extended_bytes, |observer| {
            let laws = LawSet::new_numeric(observer.descriptor(), &[id]);
            let (mut holds, mut fails, mut undefined, mut refused) =
                (0usize, 0usize, 0usize, 0usize);
            for row in &space.rows {
                // Only the original bound Authority supplies the transition.
                match actual_laws(&contract, authority, row, &laws)? {
                    None => refused += 1,
                    Some((_, diagnostics)) => match diagnostics
                        .first()
                        .filter(|d| d.id == id)
                        .map(|d| d.verdict)
                    {
                        Some(l::Verdict::Satisfied) => holds += 1,
                        Some(l::Verdict::Refused(l::Failure::Violated)) => fails += 1,
                        _ => undefined += 1,
                    },
                }
            }
            Ok(Answer::Answered(
                json!({"answer":holds,"fails":fails,"undefined":undefined,"refused":refused,"of":space.rows.len()}),
            ))
        })?
    })?
}
