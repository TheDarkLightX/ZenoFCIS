//! Complete outbox domains for laws that cannot observe delivery contents.
//! Empty and nonempty outboxes witness multiplicity; they are not samples used
//! to assert uniqueness. The source declarations independently check coverage.
use serde_json::{Value as Json, json};
use zeno_fcis_contract::internals::{
    ContractError,
    declarations::{Declarations, Form, Leaf},
    review::{
        domain::{self, LeafDomain},
        evaluate::{Delivery, Value},
    },
    rules::Rules,
};

#[derive(Clone, Debug)]
struct Channel {
    id: u32,
    destination: (u32, u32),
    payload: Vec<(u16, LeafDomain)>,
    idempotency: u128,
}

#[derive(Clone, Debug)]
pub(crate) struct DeliveryUniverse {
    channels: Vec<Channel>,
    max_deliveries: usize,
}

fn destination(d: &Declarations, id: u32) -> Option<(u32, u32)> {
    match &d.get(id).ok()?.form {
        Form::Leaf(Leaf::Text { min, max }) if min <= max && *min <= 65536 => Some((*min, *max)),
        _ => None,
    }
}
fn idempotency(r: &Rules, channel: u32) -> u128 {
    r.cases
        .iter()
        .flat_map(|c| &c.outbox)
        .filter(|d| d.channel == channel)
        .map(|d| d.idempotency)
        .max()
        .unwrap_or(0)
}
fn maximum(r: &Rules) -> usize {
    r.cases
        .iter()
        .map(|c| c.outbox.len())
        .max()
        .unwrap_or(0)
        .max(1)
}
fn scalar(domain: &LeafDomain, n: i128) -> Value {
    match domain {
        LeafDomain::Bool => Value::Bool(n != 0),
        LeafDomain::Int { .. } => Value::I128(n),
        LeafDomain::Sum { type_id, .. } => Value::Sum {
            type_id: *type_id,
            variant: n as u16,
        },
    }
}
fn fits(domain: &LeafDomain, value: &Value) -> bool {
    match (domain, value) {
        (LeafDomain::Bool, Value::Bool(_)) => true,
        (LeafDomain::Int { min, max }, Value::I128(n)) => min <= n && n <= max,
        (
            LeafDomain::Sum { type_id, variants },
            Value::Sum {
                type_id: actual,
                variant,
            },
        ) => type_id == actual && variants.contains(variant),
        _ => false,
    }
}

impl DeliveryUniverse {
    pub(crate) fn new(d: &Declarations, r: &Rules) -> Option<Self> {
        // Observation-dependent delivery laws need a different complete domain.
        if d.channels.is_empty() || !r.delivery_laws.is_empty() {
            return None;
        }
        let mut channels = Vec::new();
        for c in &d.channels {
            let mut payload = Vec::new();
            for f in d.fields(c.payload).ok()? {
                let domain = domain::leaf_domain(d, f.type_id).ok()?;
                domain.least()?;
                payload.push((f.id, domain));
            }
            channels.push(Channel {
                id: c.id,
                destination: destination(d, c.destination)?,
                payload,
                idempotency: idempotency(r, c.id),
            });
        }
        let universe = Self {
            channels,
            max_deliveries: maximum(r),
        };
        universe.covers(d, r).then_some(universe)
    }

    /// Independent acceptance of the proposed domain against the source.
    pub(crate) fn covers(&self, d: &Declarations, r: &Rules) -> bool {
        if self.max_deliveries != maximum(r)
            || self.channels.len() != d.channels.len()
            || !r.delivery_laws.is_empty()
        {
            return false;
        }
        for (c, source) in self.channels.iter().zip(&d.channels) {
            if c.id != source.id
                || Some(c.destination) != destination(d, source.destination)
                || c.idempotency != idempotency(r, source.id)
            {
                return false;
            }
            let Ok(fields) = d.fields(source.payload) else {
                return false;
            };
            if c.payload.len() != fields.len() {
                return false;
            }
            for ((id, domain), f) in c.payload.iter().zip(fields) {
                if *id != f.id || Some(domain.clone()) != domain::leaf_domain(d, f.type_id).ok() {
                    return false;
                }
            }
        }
        true
    }

    pub(crate) fn allows(&self, deliveries: &[Delivery]) -> bool {
        if deliveries.len() > self.max_deliveries {
            return false;
        }
        for (i, delivery) in deliveries.iter().enumerate() {
            if i > 0 && deliveries[i - 1].ordinal >= delivery.ordinal {
                return false;
            }
            let Some(c) = self.channels.iter().find(|c| c.id == delivery.channel) else {
                return false;
            };
            let Value::Text(text) = &delivery.destination else {
                return false;
            };
            if !text.is_ascii()
                || text.len() < c.destination.0 as usize
                || text.len() > c.destination.1 as usize
            {
                return false;
            }
            let Value::U128(n) = delivery.idempotency else {
                return false;
            };
            if n > c.idempotency || delivery.payload.len() != c.payload.len() {
                return false;
            }
            for ((id, value), (wanted, domain)) in delivery.payload.iter().zip(&c.payload) {
                if id != wanted || !fits(domain, value) {
                    return false;
                }
            }
        }
        true
    }

    pub(crate) fn example(&self) -> Vec<Delivery> {
        let c = &self.channels[0];
        vec![Delivery {
            ordinal: 0,
            channel: c.id,
            destination: Value::Text(vec![b'a'; c.destination.0 as usize]),
            payload: c
                .payload
                .iter()
                .map(|(id, d)| (*id, scalar(d, d.least().unwrap_or(0))))
                .collect(),
            idempotency: Value::U128(0),
        }]
    }

    pub(crate) fn json(&self) -> Json {
        json!({"max_deliveries":self.max_deliveries,
            "ordinals":"all strictly increasing U32 sequences",
            "channels":self.channels.iter().map(|c| json!({"channel":c.id,
                "destination":{"kind":"ascii-text","min_bytes":c.destination.0,"max_bytes":c.destination.1},
                "payload":c.payload.iter().map(|(id,d)| json!({"field":id,"domain":d.json()})).collect::<Vec<_>>(),
                "idempotency_ordinal":{"min":"0","max":c.idempotency.to_string()}
            })).collect::<Vec<_>>(),
            "scope":"All declared outboxes; selected laws cannot distinguish their contents",
            "determination":"exact","outcome_counts":"lower bounds for unrestricted deliveries"})
    }

    pub(crate) fn read(&self, text: &Json) -> Result<Vec<Delivery>, ContractError> {
        let error = || {
            ContractError::new(
                "delivery answer",
                "expected complete schema-valid deliveries",
            )
        };
        let rows = text.as_array().ok_or_else(error)?;
        if rows.len() > self.max_deliveries {
            return Err(error());
        }
        let mut out = Vec::new();
        for row in rows {
            let object = row.as_object().ok_or_else(error)?;
            if object.len() != 5
                || ![
                    "ordinal",
                    "channel",
                    "destination",
                    "payload",
                    "idempotency_ordinal",
                ]
                .iter()
                .all(|key| object.contains_key(*key))
            {
                return Err(error());
            }
            let channel =
                u32::try_from(row["channel"].as_u64().ok_or_else(error)?).map_err(|_| error())?;
            let c = self
                .channels
                .iter()
                .find(|c| c.id == channel)
                .ok_or_else(error)?;
            let mut payload = Vec::new();
            let object = row["payload"].as_object().ok_or_else(error)?;
            if object.len() != c.payload.len() {
                return Err(error());
            }
            for (id, domain) in &c.payload {
                let v = object.get(&id.to_string()).ok_or_else(error)?;
                let value = match domain {
                    LeafDomain::Bool => Value::Bool(v.as_bool().ok_or_else(error)?),
                    LeafDomain::Int { .. } => Value::I128(
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                            .parse()
                            .map_err(|_| error())?,
                    ),
                    LeafDomain::Sum { type_id, .. } => {
                        if v.as_object().is_none_or(|o| o.len() != 2)
                            || v["type_id"].as_u64() != Some(u64::from(*type_id))
                        {
                            return Err(error());
                        }
                        Value::Sum {
                            type_id: *type_id,
                            variant: u16::try_from(v["variant"].as_u64().ok_or_else(error)?)
                                .map_err(|_| error())?,
                        }
                    }
                };
                payload.push((*id, value));
            }
            let marker = &row["idempotency_ordinal"];
            out.push(Delivery {
                ordinal: u32::try_from(row["ordinal"].as_u64().ok_or_else(error)?)
                    .map_err(|_| error())?,
                channel,
                destination: Value::Text(
                    row["destination"]
                        .as_str()
                        .ok_or_else(error)?
                        .as_bytes()
                        .to_vec(),
                ),
                payload,
                idempotency: Value::U128(
                    marker
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| marker.to_string())
                        .parse()
                        .map_err(|_| error())?,
                ),
            });
        }
        if !self.allows(&out) {
            return Err(error());
        }
        Ok(out)
    }

    #[cfg(test)]
    pub(crate) fn omit_delivery_channel(&mut self) {
        self.channels.pop();
    }
}

pub(crate) fn shown(deliveries: &[Delivery]) -> Json {
    json!(deliveries.iter().map(|d| json!({"ordinal":d.ordinal,"channel":d.channel,
        "destination":d.destination.json(),
        "payload":d.payload.iter().map(|(id,v)| (id.to_string(),v.json())).collect::<serde_json::Map<_,_>>(),
        "idempotency_ordinal":d.idempotency.json()})).collect::<Vec<_>>())
}
