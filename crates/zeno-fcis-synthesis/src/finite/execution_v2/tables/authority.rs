use super::super::{
    authority::{
        canonical::{self, Part},
        framing,
    },
    meter::{self, Meter},
};
use super::*;
use super::{
    collection,
    expression::{self, Env},
};
use alloc::vec;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_codec::RustCryptoSha256;

/// Binding owns all policy/schema state. No mutable caller aliases escape preparation.
#[derive(Debug)]
pub struct Authority {
    descriptor: Descriptor,
    schema: Vec<u8>,
    identity: Vec<u8>,
    schema_hash: [u8; 32],
}
#[derive(Debug)]
pub struct Decision {
    reason: Option<u32>,
    post: Option<Value>,
    patch: Vec<(u16, Value, Value)>,
    removed: Option<Value>,
}
impl Decision {
    pub fn removed(&self) -> Option<&Value> {
        self.removed.as_ref()
    }
    pub fn reason(&self) -> Option<u32> {
        self.reason
    }
    pub fn post(&self) -> Option<&Value> {
        self.post.as_ref()
    }
    pub fn patch(&self) -> &[(u16, Value, Value)] {
        &self.patch
    }
}
/// Audit data only; publication requires the closed Authority path.
#[derive(Debug)]
pub struct Evaluation {
    result: Result<Decision, Failure>,
    usage: Usage,
    subject: Option<Vec<u8>>,
}
impl Evaluation {
    pub fn result(&self) -> Result<&Decision, &Failure> {
        self.result.as_ref()
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
    pub fn subject(&self) -> Option<&[u8]> {
        self.subject.as_deref()
    }
}
#[must_use]
#[derive(Debug)]
pub struct Publication {
    post: Vec<u8>,
    subject: Vec<u8>,
    removed: Option<Value>,
}
impl Publication {
    pub fn removed(&self) -> Option<&Value> {
        self.removed.as_ref()
    }
    pub fn post(&self) -> &[u8] {
        &self.post
    }
    pub fn subject(&self) -> &[u8] {
        &self.subject
    }
    pub fn into_parts(self) -> (Vec<u8>, Vec<u8>) {
        (self.post, self.subject)
    }
}
#[must_use]
#[derive(Debug)]
pub enum PublicationOutcome {
    Commit(Publication),
    Reject(Evaluation),
    Refused(Evaluation),
}

/// Owned ordered observation. It has no publication or monetary authority.
#[derive(Debug)]
pub struct OrderedQuery {
    result: Result<Option<Value>, Failure>,
    usage: Usage,
}
impl OrderedQuery {
    pub fn result(&self) -> Result<Option<&Value>, &Failure> {
        self.result.as_ref().map(Option::as_ref)
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
}
impl Authority {
    /// Nonauthorizing admission probe for the exact declared state domain.
    pub fn check_state(&self, state: &Value) -> Result<Usage, Failure> {
        let mut m = meter::new(self.descriptor.limits);
        self.descriptor
            .check_value(self.descriptor.roots[0], state, &mut m)?;
        collection::check_state(&self.descriptor, state, &mut m)?;
        expression::state_laws(&self.descriptor, state, &mut m)?;
        let bytes = collection::encode(state, &mut m)?;
        if (bytes.len() as u64)
            .checked_add(48)
            .ok_or(Failure::Arithmetic)?
            > self.descriptor.frame_bytes[0]
        {
            return Err(Failure::Wire);
        }
        Ok(m.used)
    }
    /// Minimum returns None for Empty; iteration returns Some(Vector of ID/old Item pairs).
    pub fn ordered_query(&self, state: &[u8], minimum: bool) -> OrderedQuery {
        let mut m = meter::new(self.descriptor.limits);
        let result = (|| {
            let d = &self.descriptor;
            let spec = d.order.as_ref().ok_or(Failure::Descriptor)?;
            let pre = self.input(0, state, &mut m)?;
            collection::check_state(d, &pre, &mut m)?;
            expression::state_laws(d, &pre, &mut m)?;
            let out = if minimum {
                super::ordered::minimum(d, spec, &pre, &mut m)?
                    .map(|(id, item)| Value::tuple(vec![id, item]).map_err(|_| Failure::Wire))
                    .transpose()?
            } else {
                Some(super::ordered::items(d, spec, &pre, &mut m)?)
            };
            if let Some(out) = &out {
                collection::reserve(out, &mut m)?;
            }
            Ok(out)
        })();
        OrderedQuery {
            result,
            usage: m.used,
        }
    }

    pub fn bind(schema: &[u8], policy: &[u8]) -> Result<Self, Failure> {
        let descriptor = Descriptor::from_policy(policy)?;
        if descriptor.schema_bytes()?.as_slice() != schema {
            return Err(Failure::Schema);
        }
        let policy = canonical::encode(&[
            Part::Word(0x5a504f32),
            Part::Word(if descriptor.ordered() { 4 } else { 3 }),
            Part::Bytes(schema),
            Part::Bytes(policy),
        ])
        .ok_or(Failure::Policy)?;
        let identity = framing::identity_bytes(&policy, &super::super::authority::EVALUATOR)
            .ok_or(Failure::Policy)?;
        let hash = zeno_fcis_codec::commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::SCHEMA,
            schema,
        )
        .map_err(|_| Failure::Schema)?;
        Ok(Self {
            descriptor,
            schema: schema.to_vec(),
            identity,
            schema_hash: *hash.as_bytes(),
        })
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn schema(&self) -> &[u8] {
        &self.schema
    }
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }
    pub fn frame(&self, source: usize, value: &Value) -> Result<Vec<u8>, Failure> {
        let root = *self.descriptor.roots.get(source).ok_or(Failure::Kind)?;
        zeno_fcis_codec::Envelope::new(
            root,
            zeno_fcis_codec::Hash32::new(self.schema_hash),
            value.clone(),
        )
        .canonical_bytes()
        .map_err(|_| Failure::Wire)
    }
    fn input(&self, source: usize, wire: &[u8], m: &mut Meter) -> Result<Value, Failure> {
        collection::step(m)?;
        m.charge(Resource::Byte, 48)?;
        let raw = crate::finite::canonical_v2::envelope::frame(
            wire,
            self.descriptor.roots[source],
            &self.schema_hash,
            self.descriptor.frame_bytes[source],
        )
        .map_err(|_| Failure::Wire)?;
        collection::decode(
            &self.descriptor,
            self.descriptor.roots[source],
            raw.bytes(),
            m,
        )
    }
    fn run(&self, raw: Raw<'_>, genesis: bool, m: &mut Meter) -> Result<Decision, Failure> {
        let pre = self.input(0, raw.state, m)?;
        collection::check_state(&self.descriptor, &pre, m)?;
        if genesis {
            collection::reserve(&pre, m)?;
            collection::reserve(&self.descriptor.genesis, m)?;
            if pre != self.descriptor.genesis {
                return Err(Failure::Law(990));
            }
            let mut env = Env {
                pre: &pre,
                command: None,
                context: None,
                post: Some(&pre),
                payload: None,
                entry: None,
                error: None,
                candidate_reason: Some(0),
            };
            expression::state_laws(&self.descriptor, &pre, m)?;
            expression::laws(&self.descriptor, &mut env, None, true, m)?;
            m.charge(Resource::Candidate, 1)?;
            return Ok(Decision {
                reason: None,
                post: Some(pre),
                patch: Vec::new(),
                removed: None,
            });
        }
        let command = self.input(1, raw.command, m)?;
        let context = self.input(2, raw.context, m)?;
        let mut env = Env {
            pre: &pre,
            command: Some(&command),
            context: Some(&context),
            post: None,
            payload: None,
            entry: None,
            error: None,
            candidate_reason: None,
        };
        expression::state_laws(&self.descriptor, &pre, m)?;
        let mut chosen = None;
        for case in &self.descriptor.cases {
            if env.eval(&self.descriptor, &case.when, m)?.boolean()? {
                chosen = Some(&case.plan);
                break;
            }
        }
        let plan = chosen.ok_or(Failure::Descriptor)?;
        m.charge(Resource::Candidate, 1)?;
        let (reason, post) = env.plan(&self.descriptor, plan, m)?;
        if let Some(post) = &post {
            self.descriptor
                .check_value(self.descriptor.roots[0], post, m)?;
            collection::check_state(&self.descriptor, post, m)?;
            let encoded = collection::encode(post, m)?;
            if encoded.len() as u64 + 48 > self.descriptor.frame_bytes[0] {
                return Err(Failure::Wire);
            }
        }
        env.post = post.as_ref();
        env.candidate_reason = Some(reason.map(i128::from).unwrap_or(0));
        expression::laws(&self.descriptor, &mut env, reason, false, m)?;
        if let Some(post) = &post {
            expression::state_laws(&self.descriptor, post, m)?;
        }
        let mut patch = Vec::new();
        if let Some(post) = &post {
            for field in self.descriptor.record(self.descriptor.roots[0])? {
                collection::visit(m)?;
                let a = super::schema::record_field(&pre, field.id)?;
                let b = super::schema::record_field(post, field.id)?;
                collection::reserve(a, m)?;
                collection::reserve(b, m)?;
                if a != b {
                    patch.push((field.id, collection::copy(a, m)?, collection::copy(b, m)?));
                }
            }
        }
        let removed = if reason.is_none() {
            if let Some(spec) = &self.descriptor.order {
                let (op, _, _, _, _) =
                    super::ordered::command(&self.descriptor, self.descriptor.roots[1], &command)?;
                if op == 3 {
                    super::ordered::minimum(&self.descriptor, spec, &pre, m)?
                        .map(|(id, item)| Value::tuple(vec![id, item]).map_err(|_| Failure::Wire))
                        .transpose()?
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(spec) = &self.descriptor.order {
            let expected =
                super::order_laws::removed(&self.descriptor, spec, &pre, &command, reason, m)?;
            for value in removed.iter().chain(expected.iter()) {
                collection::reserve(value, m)?;
            }
            collection::step(m)?;
            if removed != expected {
                return Err(Failure::Law(989));
            }
        }
        if let Some(value) = &removed {
            for _ in 0..3 {
                collection::reserve(value, m)?;
            }
        }
        Ok(Decision {
            removed,
            reason,
            post,
            patch,
        })
    }
    fn evaluate_kind(&self, raw: Raw<'_>, genesis: bool) -> Evaluation {
        let mut meter = meter::new(self.descriptor.limits);
        let mut result = self.run(raw, genesis, &mut meter);
        let subject = match &result {
            Ok(decision) => {
                let artifact = (|| {
                    let post = match &decision.post {
                        Some(v) => collection::encode(v, &mut meter)?,
                        None => Vec::new(),
                    };
                    meter.charge(
                        Resource::Byte,
                        48u64
                            .checked_add(post.len() as u64)
                            .ok_or(Failure::Arithmetic)?,
                    )?;
                    collection::step(&mut meter)?;
                    for (_, a, b) in &decision.patch {
                        collection::reserve(a, &mut meter)?;
                        collection::reserve(b, &mut meter)?;
                    }
                    let changes = decision
                        .patch
                        .iter()
                        .map(|(id, a, b)| {
                            Value::tuple(vec![Value::signed((*id).into()), a.clone(), b.clone()])
                                .map_err(|_| Failure::Wire)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let changes = Value::vector(changes).map_err(|_| Failure::Wire)?;
                    let changes = if self.descriptor.order.is_some() {
                        let removed = decision
                            .removed
                            .as_ref()
                            .map(|x| collection::copy(x, &mut meter))
                            .transpose()?;
                        Value::tuple(vec![
                            changes,
                            Value::vector(removed.into_iter().collect())
                                .map_err(|_| Failure::Wire)?,
                        ])
                        .map_err(|_| Failure::Wire)?
                    } else {
                        changes
                    };
                    let value = Value::tuple(vec![
                        Value::signed(decision.reason.map(i128::from).unwrap_or(0)),
                        Value::bytes(post).map_err(|_| Failure::Wire)?,
                        changes,
                        Value::vector(
                            meter
                                .used
                                .counters
                                .iter()
                                .map(|x| Value::signed((*x).into()))
                                .collect(),
                        )
                        .map_err(|_| Failure::Wire)?,
                    ])
                    .map_err(|_| Failure::Wire)?;
                    collection::reserve(&value, &mut meter)?;
                    let ValueRef::Tuple(fields) = value.view() else {
                        return Err(Failure::Kind);
                    };
                    for value in &fields[..3] {
                        collection::reserve(value, &mut meter)?;
                    }
                    let mut subject_bytes = 8u64 * 17;
                    for size in [
                        self.identity.len(),
                        raw.state.len(),
                        raw.command.len(),
                        raw.context.len(),
                        value.encoded_length().map_err(|_| Failure::Wire)?,
                    ] {
                        subject_bytes = subject_bytes
                            .checked_add(size as u64)
                            .ok_or(Failure::Arithmetic)?;
                    }
                    meter.charge(Resource::Step, 8)?;
                    meter.charge(Resource::Byte, subject_bytes)?;
                    let final_usage = Value::vector(
                        meter
                            .used
                            .counters
                            .iter()
                            .map(|x| Value::signed((*x).into()))
                            .collect(),
                    )
                    .map_err(|_| Failure::Wire)?;
                    let final_value = Value::tuple(vec![
                        fields[0].clone(),
                        fields[1].clone(),
                        fields[2].clone(),
                        final_usage,
                    ])
                    .map_err(|_| Failure::Wire)?;
                    final_value.canonical_bytes().map_err(|_| Failure::Wire)
                })();
                match artifact {
                    Ok(bytes) => framing::subject_bytes(
                        if genesis {
                            super::super::composition::Kind::Genesis
                        } else {
                            super::super::composition::Kind::Transition
                        },
                        &self.identity,
                        raw.state,
                        raw.command,
                        raw.context,
                        &bytes,
                    ),
                    Err(e) => {
                        result = Err(e);
                        None
                    }
                }
            }
            Err(_) => None,
        };
        if result.is_ok() && subject.is_none() {
            result = Err(Failure::Arithmetic)
        }
        Evaluation {
            result,
            usage: meter.used,
            subject,
        }
    }
    /// Nonauthorizing finite-check probe under this already admitted immutable policy.
    pub fn check_laws(
        &self,
        pre: &Value,
        command: &Value,
        context: &Value,
        post: Option<&Value>,
        reason: Option<u32>,
    ) -> Result<Vec<expression::LawReport>, Failure> {
        expression::probe(&self.descriptor, pre, command, context, post, reason)
    }
    /// Independent result probe, including remove_lowest's complete old item.
    pub fn check_ordered_result(
        &self,
        pre: &Value,
        command: &Value,
        context: &Value,
        post: Option<&Value>,
        reason: Option<u32>,
        removed: Option<&Value>,
    ) -> Result<Vec<LawReport>, Failure> {
        let mut m = meter::new(self.descriptor.limits);
        let reports = expression::probe_meter(
            &self.descriptor,
            pre,
            command,
            context,
            post,
            reason,
            &mut m,
        )?;
        let spec = self.descriptor.order.as_ref().ok_or(Failure::Descriptor)?;
        let expected =
            super::order_laws::removed(&self.descriptor, spec, pre, command, reason, &mut m)?;
        for value in expected.iter().chain(removed) {
            collection::reserve(value, &mut m)?;
        }
        collection::step(&mut m)?;
        if expected.as_ref() != removed {
            return Err(Failure::Law(989));
        }
        Ok(reports)
    }
    pub fn evaluate(&self, raw: Raw<'_>) -> Evaluation {
        self.evaluate_kind(raw, false)
    }
    pub fn genesis(&self, state: &[u8]) -> Evaluation {
        self.evaluate_kind(
            Raw {
                state,
                command: &[],
                context: &[],
            },
            true,
        )
    }
    pub fn replay(&self, raw: Raw<'_>, expected: &[u8]) -> Evaluation {
        let mut e = self.evaluate(raw);
        if e.result.is_ok() && e.subject.as_deref() != Some(expected) {
            e.result = Err(Failure::Replay);
            e.subject = None;
        }
        e
    }
    pub fn publish(&self, raw: Raw<'_>) -> PublicationOutcome {
        let mut e = self.evaluate(raw);
        match &e.result {
            Err(_) => PublicationOutcome::Refused(e),
            Ok(d) if d.reason.is_some() => PublicationOutcome::Reject(e),
            Ok(d) => {
                let out = (|| {
                    let post = d.post.as_ref().ok_or(Failure::Undefined)?;
                    let bytes = self.frame(0, post)?;
                    let subject = e.subject.take().ok_or(Failure::Undefined)?;
                    Ok(Publication {
                        post: bytes,
                        subject,
                        removed: d.removed.clone(),
                    })
                })();
                match out {
                    Ok(p) => PublicationOutcome::Commit(p),
                    Err(err) => {
                        e.result = Err(err);
                        e.subject = None;
                        PublicationOutcome::Refused(e)
                    }
                }
            }
        }
    }
}
