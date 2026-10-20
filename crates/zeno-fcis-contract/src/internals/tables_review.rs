//! Complete canonical-domain table review. Limits produce inconclusive, never samples.
use super::{ContractError, tables::Compiled};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use t::{Shape, Value};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn error(message: impl ToString) -> ContractError {
    ContractError::new("tables review", message.to_string())
}
fn product(parts: &[Vec<Value>], cap: usize) -> Result<Vec<Vec<Value>>, ContractError> {
    let mut rows = vec![Vec::new()];
    for values in parts {
        if rows.len().checked_mul(values.len()).is_none_or(|n| n > cap) {
            return Err(error("domain-over-limit"));
        }
        rows = rows
            .into_iter()
            .flat_map(|row| {
                values.iter().map(move |v| {
                    let mut next = row.clone();
                    next.push(v.clone());
                    next
                })
            })
            .collect();
    }
    Ok(rows)
}
/// Enumerates exact typed values, including all canonical maps within every limit.
pub fn values(d: &t::Descriptor, id: u32, cap: usize) -> Result<Vec<Value>, ContractError> {
    let ty = d
        .types
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| error("unknown type"))?;
    Ok(match &ty.shape {
        Shape::Unit => vec![Value::unit()],
        Shape::Bool => vec![Value::boolean(false), Value::boolean(true)],
        Shape::U128 { min, max } => {
            let _count = max
                .checked_sub(*min)
                .and_then(|n| n.checked_add(1))
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n <= cap)
                .ok_or_else(|| error("domain-over-limit"))?;
            (*min..=*max).map(Value::unsigned).collect()
        }
        Shape::I128 { min, max } => {
            let count = max
                .checked_sub(*min)
                .and_then(|v| v.checked_add(1))
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| error("domain-over-limit"))?;
            if count > cap {
                return Err(error("domain-over-limit"));
            }
            (*min..=*max).map(Value::signed).collect()
        }
        Shape::Bytes { min, max } => {
            if *max > 1 {
                return Err(error("domain-over-limit"));
            }
            let mut out = Vec::new();
            if *min == 0 {
                out.push(Value::bytes(vec![]).map_err(error)?);
            }
            if *max == 1 {
                for b in 0..=255 {
                    out.push(Value::bytes(vec![b]).map_err(error)?);
                }
            }
            if out.len() > cap {
                return Err(error("domain-over-limit"));
            }
            out
        }
        Shape::Maybe(element) => {
            let mut out = vec![Value::sum(id, 0, None)];
            for x in values(d, *element, cap)? {
                out.push(Value::sum(id, 1, Some(x)));
            }
            if out.len() > cap {
                return Err(error("domain-over-limit"));
            }
            out
        }
        Shape::Vector {
            element,
            max,
            bytes,
        } => {
            let elems = values(d, *element, cap)?;
            let mut out = Vec::new();
            for n in 0..=*max {
                let rows = product(&vec![elems.clone(); n as usize], cap)?;
                for row in rows {
                    let v = Value::vector(row).map_err(error)?;
                    if v.encoded_length().map_err(error)? as u64 <= *bytes {
                        out.push(v);
                        if out.len() > cap {
                            return Err(error("domain-over-limit"));
                        }
                    }
                }
            }
            out
        }
        Shape::Tuple(ids) => {
            let parts = ids
                .iter()
                .map(|id| values(d, *id, cap))
                .collect::<Result<Vec<_>, _>>()?;
            product(&parts, cap)?
                .into_iter()
                .map(|row| Value::tuple(row).map_err(error))
                .collect::<Result<_, _>>()?
        }
        Shape::Record(fs) => {
            let parts = fs
                .iter()
                .map(|f| values(d, f.type_id, cap))
                .collect::<Result<Vec<_>, _>>()?;
            product(&parts, cap)?
                .into_iter()
                .map(|row| {
                    Value::record_canonical(
                        fs.iter()
                            .zip(row)
                            .map(|(f, v)| t::Field::new(f.id, v))
                            .collect(),
                    )
                    .map_err(error)
                })
                .collect::<Result<_, _>>()?
        }
        Shape::Table {
            key, value, limits, ..
        } => {
            let keys = values(d, *key, cap)?;
            let vals = values(d, *value, cap)?;
            let mut keyed = keys
                .into_iter()
                .map(|k| {
                    let e = t::MapEntry::try_new(k.clone(), Value::unit()).map_err(error)?;
                    Ok((e.encoded_key().to_vec(), k))
                })
                .collect::<Result<Vec<_>, ContractError>>()?;
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            fn extend(
                (keys, vals): (&[(Vec<u8>, Value)], &[Value]),
                start: usize,
                chosen: &mut Vec<t::MapEntry>,
                limits: t::CollectionLimits,
                bytes: u64,
                cap: usize,
                out: &mut Vec<Value>,
            ) -> Result<(), ContractError> {
                if out.len() >= cap {
                    return Err(error("domain-over-limit"));
                }
                out.push(Value::map_canonical(chosen.clone()).map_err(error)?);
                if chosen.len() >= limits.max_entries as usize {
                    return Ok(());
                }
                for i in start..keys.len() {
                    for val in vals {
                        let item = 16u64
                            .checked_add(keys[i].0.len() as u64)
                            .and_then(|n| n.checked_add(val.encoded_length().ok()? as u64))
                            .ok_or_else(|| error("byte overflow"))?;
                        let next = bytes
                            .checked_add(item)
                            .ok_or_else(|| error("byte overflow"))?;
                        if item > limits.max_item_bytes || next > limits.max_snapshot_bytes {
                            continue;
                        }
                        chosen.push(
                            t::MapEntry::try_new(keys[i].1.clone(), val.clone()).map_err(error)?,
                        );
                        extend((keys, vals), i + 1, chosen, limits, next, cap, out)?;
                        chosen.pop();
                    }
                }
                Ok(())
            }
            let mut out = Vec::new();
            extend(
                (&keyed, &vals),
                0,
                &mut Vec::new(),
                *limits,
                29,
                cap,
                &mut out,
            )?;
            out
        }
    })
}
fn bytes(v: &Value) -> Result<Vec<u8>, ContractError> {
    v.zcve_bytes().map_err(error)
}
fn digest(rows: &[Vec<u8>]) -> Result<String, ContractError> {
    use zeno_fcis_codec::CommitmentHasher;
    let mut all = Vec::new();
    for row in rows {
        all.extend_from_slice(&(row.len() as u64).to_be_bytes());
        all.extend_from_slice(row);
    }
    let h = zeno_fcis_crypto::RustCryptoSha256::hash(&all);
    Ok(h.as_bytes().iter().map(|b| format!("{b:02x}")).collect())
}
/// All declared State laws are the inductive invariant; actual genesis/commits must satisfy them.
pub fn review(
    compiled: &Compiled,
    max_tuples: u64,
    max_work: u64,
    gaps: bool,
) -> Result<Json, ContractError> {
    let d = &compiled.descriptor;
    let cap = usize::try_from(max_tuples).map_err(error)?;
    let domains = (|| {
        Ok::<_, ContractError>((
            values(d, d.roots[0], cap)?,
            values(d, d.roots[1], cap)?,
            values(d, d.roots[2], cap)?,
        ))
    })();
    let (states, commands, contexts) = match domains {
        Ok(v) => v,
        Err(e) if e.reason() == "domain-over-limit" => {
            return Ok(
                json!({"schema":"zeno-fcis/tables-review/1","status":"inconclusive","reason":"domain-over-limit"}),
            );
        }
        Err(e) => return Err(e),
    };
    let wire_states = states.len();
    let states = if d.order.is_some() {
        let admission = t::Authority::bind(&compiled.schema, &compiled.policy)
            .map_err(|e| error(format!("bind {e:?}")))?;
        let mut admitted = Vec::new();
        for state in states {
            match admission.check_state(&state) {
                Ok(_) => admitted.push(state),
                Err(t::Failure::Budget(_)) => {
                    return Err(error("state admission budget insufficient"));
                }
                Err(t::Failure::Law(980)) => {}
                Err(e) => return Err(error(format!("state domain admission {e:?}"))),
            }
        }
        admitted
    } else {
        states
    };
    let tuples = (states.len() as u64)
        .checked_mul(commands.len() as u64)
        .and_then(|v| v.checked_mul(contexts.len() as u64))
        .ok_or_else(|| error("tuple overflow"))?;
    if tuples > max_tuples {
        return Ok(
            json!({"schema":"zeno-fcis/tables-review/1","status":"inconclusive","reason":"tuple-over-limit","tuples":tuples}),
        );
    }
    let reasons = &d.reasons;
    let work = tuples
        .checked_mul((states.len() + reasons.len()) as u64)
        .and_then(|n| n.checked_mul(d.laws.len() as u64))
        .ok_or_else(|| error("work overflow"))?;
    if gaps && work > max_work {
        return Ok(
            json!({"schema":"zeno-fcis/tables-review/1","status":"inconclusive","reason":"work-over-limit","work":work}),
        );
    }
    let authority = t::Authority::bind(&compiled.schema, &compiled.policy)
        .map_err(|e| error(format!("bind {e:?}")))?;
    let mut independent = d.clone();
    independent.laws.retain(|law| law.id() != 991);
    let judge = t::Authority::bind(
        &compiled.schema,
        &independent
            .policy_bytes()
            .map_err(|e| error(format!("independent policy {e:?}")))?,
    )
    .map_err(|e| error(format!("independent bind {e:?}")))?;
    let genesis = authority.genesis(
        &authority
            .frame(0, &d.genesis)
            .map_err(|e| error(format!("frame {e:?}")))?,
    );
    if let Err(e) = genesis.result() {
        return Err(error(format!("genesis refused: {e:?}")));
    }
    let mut table = Vec::new();
    let mut total = 0;
    let mut determined = 0;
    let mut committed = 0;
    let mut rejects = 0;
    let mut zero_decisions = 0;
    let mut ambiguous = 0;
    let state_ids = states
        .iter()
        .enumerate()
        .map(|(i, v)| Ok((bytes(v)?, i)))
        .collect::<Result<BTreeMap<_, _>, ContractError>>()?;
    for state in &states {
        let sw = authority
            .frame(0, state)
            .map_err(|e| error(format!("frame {e:?}")))?;
        for command in &commands {
            let cw = authority
                .frame(1, command)
                .map_err(|e| error(format!("frame {e:?}")))?;
            for context in &contexts {
                let xw = authority
                    .frame(2, context)
                    .map_err(|e| error(format!("frame {e:?}")))?;
                let raw = t::Raw {
                    state: &sw,
                    command: &cw,
                    context: &xw,
                };
                let evaluation = authority.evaluate(raw);
                let decision = evaluation
                    .result()
                    .map_err(|e| error(format!("valid finite tuple technically refused: {e:?}")))?;
                if let Some(post) = decision.post() {
                    if !state_ids.contains_key(&bytes(post)?) {
                        return Err(error("successor outside complete declared domain"));
                    }
                    committed += 1;
                } else {
                    rejects += 1;
                }
                table.push(
                    evaluation
                        .subject()
                        .ok_or_else(|| error("missing subject"))?
                        .to_vec(),
                );
                total += 1;
                if gaps {
                    let mut lawful = 0;
                    for post in &states {
                        match judge.check_laws(state, command, context, Some(post), None) {
                            Ok(_) => lawful += 1,
                            Err(t::Failure::Budget(_)) => {
                                return Err(error("candidate-law budget insufficient"));
                            }
                            Err(_) => {}
                        }
                    }
                    for reason in reasons {
                        match judge.check_laws(state, command, context, None, Some(*reason)) {
                            Ok(_) => lawful += 1,
                            Err(t::Failure::Budget(_)) => {
                                return Err(error("candidate-law budget insufficient"));
                            }
                            Err(_) => {}
                        }
                    }
                    if lawful == 1 {
                        determined += 1
                    } else if lawful == 0 {
                        zero_decisions += 1
                    } else {
                        ambiguous += 1
                    }
                }
            }
        }
    }
    Ok(
        json!({"schema":"zeno-fcis/tables-review/1","authority":"none","status":if gaps&&(zero_decisions>0||ambiguous>0){"law-gaps"}else{"finite-checked"},"profile":if d.order.is_some(){"tables-ordered/1"}else{t::PROFILE},"wire_states":wire_states,"scope":"complete declared finite domain only; tested journeys are separate","states":states.len(),"commands":commands.len(),"contexts":contexts.len(),"tuples":total,"table_sha256":digest(&table)?,"identity_sha256":digest(&[authority.identity().to_vec()])?,"committed":committed,"rejected_without_changes":rejects,"technical_refusals":0,"gaps":{"enabled":gaps,"case_conformance_991_used":false,"determined":determined,"zero_lawful_decisions":zero_decisions,"ambiguous":ambiguous,"candidate_work_bound":work},"lemma":{"genesis":"checked","invariant":"original state schema, collection limits and declared State laws","consecution":"checked for every declared state and input; rejects preserve pre-state","histories":"arbitrary finite histories inside this exact finite domain"}}),
    )
}
