//! Typed contract application. Every verdict and publication comes from the library Authority.
#![forbid(unsafe_code)]
mod examples;
pub mod v2_contract;
use zeno_fcis_synthesis::finite::{
    V2NumericAtom as Atom, V2NumericDomain as Domain, canonical_v2::output, v2_authority as a,
    v2_composition as c,
};
pub type AppResult<T> = Result<T, String>;
pub type Authority<'a> = a::Authority<'a, c::NumericDescriptor<'a>>;
#[derive(Clone, Debug)]
pub struct Example {
    pub line: usize,
    pub inputs: Vec<Atom<'static>>,
    pub class: c::Class,
    pub reason: Option<u32>,
    pub post: Vec<Atom<'static>>,
    pub outbox: Vec<(u32, Vec<Atom<'static>>)>,
}
pub fn authority<'a>(descriptor: &'a c::NumericDescriptor<'a>) -> AppResult<Authority<'a>> {
    v2_contract::checked_authority(descriptor).map_err(|e| format!("bind: {e:?}"))
}
pub fn examples(text: &str) -> AppResult<Vec<Example>> {
    let contract = v2_contract::Contract::new();
    examples::parse(text, &contract.descriptor())
        .map_err(|e| format!("example line {}: {}", e.line, e.reason))
        .map(|ls| {
            ls.into_iter()
                .map(|l| Example {
                    line: l.number,
                    inputs: l.inputs,
                    class: match l.class {
                        examples::Class::Accept => c::Class::Accept,
                        examples::Class::Reject => c::Class::Reject,
                        examples::Class::Failure => c::Class::CommittedFailure,
                    },
                    reason: l.reason,
                    post: l.post,
                    outbox: l.outbox,
                })
                .collect()
        })
}
fn reify(domain: Domain<'_>, value: Atom<'static>) -> AppResult<Atom<'static>> {
    match (domain, value) {
        (Domain::Sum { type_id, variants }, Atom::I128(n)) => {
            let variant = u16::try_from(n).map_err(|_| "variant outside u16")?;
            if variants.contains(&variant) {
                Ok(Atom::Sum { type_id, variant })
            } else {
                Err("undeclared variant".into())
            }
        }
        (_, v) => Ok(v),
    }
}
fn root(
    schema: c::Schema<'_, Domain<'_>>,
    values: &[Atom<'static>],
    binding: &c::FrameBinding,
) -> AppResult<Vec<u8>> {
    let max = usize::try_from(binding.max_bytes).map_err(|_| "frame size exceeds host")?;
    let payload = match schema {
        c::Schema::Record(fields) if fields.len() == values.len() => {
            let fs = fields
                .iter()
                .zip(values)
                .map(|(f, v)| {
                    Ok(c::Field {
                        id: f.id,
                        value: reify(f.leaf, *v)?,
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            output::encode_record(&fs, max)
        }
        c::Schema::Leaf(domain) if values.len() == 1 => {
            output::encode_atom(reify(*domain, values[0])?, max)
        }
        _ => return Err("root shape or value count mismatch".into()),
    }
    .map_err(|e| format!("encode root: {e:?}"))?;
    output::encode_envelope(binding.root, &binding.schema, &payload, max)
        .map_err(|e| format!("encode envelope: {e:?}"))
}
fn width(schema: c::Schema<'_, Domain<'_>>) -> usize {
    match schema {
        c::Schema::Record(fs) => fs.len(),
        c::Schema::Leaf(_) => 1,
        _ => 0,
    }
}
pub fn wire(
    descriptor: &c::NumericDescriptor<'_>,
    values: &[Atom<'static>],
) -> AppResult<[Vec<u8>; 3]> {
    let schemas = [descriptor.state, descriptor.command, descriptor.context];
    let widths = schemas.map(width);
    if widths.iter().sum::<usize>() != values.len() {
        return Err("input count mismatch".into());
    }
    let (s, rest) = values.split_at(widths[0]);
    let (c, x) = rest.split_at(widths[1]);
    let f = &v2_contract::FRAMING;
    Ok([
        root(schemas[0], s, &f.state)?,
        root(schemas[1], c, &f.command)?,
        root(schemas[2], x, &f.context)?,
    ])
}
pub fn genesis() -> AppResult<Vec<u8>> {
    let payload = output::encode_record(
        v2_contract::GENESIS,
        v2_contract::FRAMING.state.max_bytes as usize,
    )
    .map_err(|e| format!("genesis: {e:?}"))?;
    let f = &v2_contract::FRAMING.state;
    output::encode_envelope(f.root, &f.schema, &payload, f.max_bytes as usize)
        .map_err(|e| format!("genesis: {e:?}"))
}
pub fn check(authority: &Authority<'_>, example: &Example) -> AppResult<()> {
    let frames = wire(authority.descriptor(), &example.inputs)?;
    let raw = c::Raw {
        state: &frames[0],
        command: &frames[1],
        context: &frames[2],
    };
    let actual = authority.evaluate(raw);
    let candidate = actual
        .result()
        .map_err(|e| format!("line {}: {e:?}", example.line))?;
    let post: Vec<_> = candidate.post().iter().map(|f| f.value).collect();
    if example.class == c::Class::Reject
        && example.post != example.inputs[..width(authority.descriptor().state)]
    {
        return Err(format!(
            "line {}: rejected post differs from pre-state",
            example.line
        ));
    }
    let expected_post = if example.class == c::Class::Reject {
        Vec::new()
    } else {
        example.post.clone()
    };
    let outbox: Vec<_> = candidate
        .outbox()
        .iter()
        .map(|d| {
            (
                d.channel,
                d.payload.iter().map(|f| f.value).collect::<Vec<_>>(),
            )
        })
        .collect();
    if candidate.class() != example.class
        || candidate.reason() != example.reason
        || post != expected_post
        || outbox != example.outbox
    {
        return Err(format!("line {}: whole decision differs", example.line));
    }
    // Publication also validates original output bytes and replays them unchanged.
    match authority.publish(raw) {
        a::PublicationOutcome::Commit(p) => {
            if !matches!(
                authority.replay_publication(raw, p.subject()),
                a::PublicationOutcome::Commit(_)
            ) {
                return Err("publication replay refused".into());
            }
        }
        a::PublicationOutcome::Reject(_) => {}
        _ => return Err("publication refused".into()),
    }
    Ok(())
}
