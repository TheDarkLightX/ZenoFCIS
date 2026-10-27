//! Actual compiler and Authority route; reference comparison is independent of
//! the literal-table renderer and compares canonical semantic fields.

use super::{common::*, reference::Relation};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::review::{domain, evaluate::Framer};
use zeno_fcis_contract::internals::{
    declarations::Declarations, model::Contract, policy, program_bytes, rules::Rules, schema,
    schema_commitment,
};
use zeno_fcis_synthesis::finite::{V2Resource, V2Usage, canonical_v2::output, v2_composition as c};

#[derive(Default)]
pub(super) struct Counts {
    pub(super) calls: u64,
    pub(super) rows: u64,
    pub(super) usage: [u128; 8],
}
impl Counts {
    fn record(&mut self, usage: V2Usage) {
        self.calls += 1;
        for (sum, resource) in self.usage.iter_mut().zip([
            V2Resource::Read,
            V2Resource::Write,
            V2Resource::Candidate,
            V2Resource::Effect,
            V2Resource::Byte,
            V2Resource::WitnessByte,
            V2Resource::Depth,
            V2Resource::Step,
        ]) {
            *sum += u128::from(usage.used(resource));
        }
    }
    pub(super) fn json(&self) -> Value {
        json!({"authority_evaluations":self.calls,"checked_rows":self.rows,
            "authority_usage":self.usage.map(|v|v.to_string())})
    }
}

pub(super) fn reference(
    relation: &Relation,
    project: &str,
    policy_text: &str,
    counts: &mut Counts,
) -> Result<Value> {
    let rules = Rules::read(policy_text)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let (contract, roots) = Contract::build_with_roots(&declarations, &rules, commitment)?;
    let program = contract.program()?;
    let bytes = program_bytes(&program)?;
    let inventory = json!({"nodes":program.nodes().len(),"inputs":program.inputs().len(),
        "outputs":program.outputs().len(),"bytes":bytes.len(),"sha256":sha(&bytes),
        "root_count":roots.len(),"law_steps":contract.law_steps(),"profile":program.profile(),
        "laws":contract.laws.iter().map(|law|json!({"id":law.id,"kind":law.kind.name(),"nodes":law.nodes.len()})).collect::<Vec<_>>()});
    let positions = domain::positions(&declarations)?;
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| -> Result<()> {
        let frames = framer.frame(&[0, 0, 0]);
        let genesis = authority.genesis(&frames[0]);
        counts.record(genesis.usage());
        if genesis.result().is_err() {
            return fail(1, "abstract-genesis-refused");
        }
        for (k, row) in relation.rows.iter().enumerate() {
            let frames = framer.frame(&[(k / 16) as i128, (k % 16) as i128, 0]);
            let evaluation = authority.evaluate(c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            });
            counts.record(evaluation.usage());
            let candidate = evaluation
                .result()
                .map_err(|e| gate(format!("abstract-authority-refused:{k}: {e:?}")))?;
            let accept = row["class"] == "Accept";
            let expected_class = if accept {
                c::Class::Accept
            } else {
                c::Class::Reject
            };
            let expected_reason = if accept {
                None
            } else {
                Some(number(&row["reason"], 201, 205)? as u32)
            };
            if candidate.class() != expected_class
                || candidate.reason() != expected_reason
                || !candidate.effects().is_empty()
            {
                return fail(1, format!("abstract-observation-mismatch:{k}"));
            }
            let successor = number(&row["successor"], 0, 44)?;
            let post = if accept {
                vec![c::Field {
                    id: 110,
                    value: c::Atom::I128(successor),
                }]
            } else {
                vec![]
            };
            let encode_record = |fields: &[c::Field<'_>]| {
                output::encode_record(fields, SOURCE_LIMIT)
                    .map_err(|e| gate(format!("canonical-record:{e:?}")))
            };
            if encode_record(candidate.post())? != encode_record(&post)? {
                return fail(1, format!("abstract-successor-mismatch:{k}"));
            }
            if !accept && !candidate.patch().is_empty() {
                return fail(1, format!("abstract-reject-patch:{k}"));
            }
            let deliveries = candidate.outbox();
            if deliveries.len() != usize::from(accept) {
                return fail(1, format!("abstract-outbox-count:{k}"));
            }
            if accept {
                let delivery = &deliveries[0];
                let expected = &row["outbox"][0];
                let mut payload = Vec::new();
                for j in 0..4 {
                    payload.push(c::Field {
                        id: 140 + j as u16,
                        value: c::Atom::I128(number(&expected["payload"][j][1], 0, 3)?),
                    });
                }
                let encode_atom = |atom| {
                    output::encode_atom(atom, SOURCE_LIMIT)
                        .map_err(|e| gate(format!("canonical-atom:{e:?}")))
                };
                if delivery.ordinal != 0
                    || delivery.channel != 300
                    || encode_atom(delivery.destination)?
                        != encode_atom(c::Atom::Text(b"priority-events"))?
                    || encode_atom(delivery.idempotency)? != encode_atom(c::Atom::U128(0))?
                    || encode_record(&delivery.payload)? != encode_record(&payload)?
                {
                    return fail(1, format!("abstract-delivery-mismatch:{k}"));
                }
            }
            counts.rows += 1;
        }
        Ok(())
    })??;
    Ok(json!({"status":"reference-agrees","counts":counts.json(),"inventory":inventory}))
}
