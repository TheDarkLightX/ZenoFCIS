//! Exact numeric examples for the typed contract ABI; decimal text never narrows U128.
use zeno_fcis_synthesis::finite::{
    V2NumericAtom as Atom, V2NumericDomain as Domain, v2_composition as c,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Class {
    Accept,
    Reject,
    Failure,
}
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub number: usize,
    pub inputs: Vec<Atom<'static>>,
    pub class: Class,
    pub reason: Option<u32>,
    pub post: Vec<Atom<'static>>,
    pub outbox: Vec<(u32, Vec<Atom<'static>>)>,
}
#[derive(Debug)]
pub(crate) struct Error {
    pub line: usize,
    pub reason: String,
}
fn value(word: &str, d: Domain<'_>) -> Result<Atom<'static>, String> {
    let fail = || format!("`{word}` is outside its exact numeric domain");
    match d {
        Domain::Sum { type_id, variants } => {
            let variant = word.parse::<u16>().map_err(|_| fail())?;
            if variants.contains(&variant) {
                Ok(Atom::Sum { type_id, variant })
            } else {
                Err(fail())
            }
        }
        Domain::Bool => match word {
            "0" => Ok(Atom::Bool(false)),
            "1" => Ok(Atom::Bool(true)),
            _ => Err(fail()),
        },
        Domain::I128 { min, max } => {
            let n = word.parse::<i128>().map_err(|_| fail())?;
            if min <= n && n <= max {
                Ok(Atom::I128(n))
            } else {
                Err(fail())
            }
        }
        Domain::U128 { min, max } => {
            if word.is_empty()
                || !word.bytes().all(|b| b.is_ascii_digit())
                || (word.len() > 1 && word.starts_with('0'))
            {
                return Err(fail());
            }
            let n = word.parse::<u128>().map_err(|_| fail())?;
            if min <= n && n <= max {
                Ok(Atom::U128(n))
            } else {
                Err(fail())
            }
        }
        _ => Err("unsupported numeric example domain".into()),
    }
}
fn values(text: &str, domains: &[Domain<'_>]) -> Result<Vec<Atom<'static>>, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    if words.len() != domains.len() {
        return Err(format!(
            "expected {} numbers, found {}",
            domains.len(),
            words.len()
        ));
    }
    words
        .iter()
        .zip(domains)
        .map(|(w, d)| value(w, *d))
        .collect()
}
pub(crate) fn parse(text: &str, d: &c::NumericDescriptor<'_>) -> Result<Vec<Line>, Error> {
    let mut result = Vec::new();
    let state: Vec<_> = match d.state {
        c::Schema::Record(fs) => fs.iter().map(|f| f.leaf).collect(),
        c::Schema::Leaf(v) => vec![*v],
        _ => {
            return Err(Error {
                line: 0,
                reason: "unsupported example state schema".into(),
            });
        }
    };
    let mut input_domains = Vec::new();
    for schema in [d.state, d.command, d.context] {
        match schema {
            c::Schema::Record(fs) => input_domains.extend(fs.iter().map(|f| f.leaf)),
            c::Schema::Leaf(v) => input_domains.push(*v),
            _ => {
                return Err(Error {
                    line: 0,
                    reason: "unsupported example input schema".into(),
                });
            }
        }
    }
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let number = index + 1;
        let row = (|| {
            let parts: Vec<_> = line.split('|').map(str::trim).collect();
            let [inputs @ .., decision, deliveries] = parts.as_slice() else {
                return Err("expected inputs | class reason post | deliveries".into());
            };
            if inputs.is_empty() {
                return Err("missing example inputs".into());
            }
            let input = values(&inputs.join(" "), &input_domains)?;
            let mut words = decision.split_whitespace();
            let class = match words.next() {
                Some("accept") => Class::Accept,
                Some("reject") => Class::Reject,
                Some("failure") => Class::Failure,
                _ => return Err("invalid decision class".into()),
            };
            let reason = match words.next() {
                Some("-") => None,
                Some(w) => Some(w.parse::<u32>().map_err(|_| "invalid reason")?),
                None => return Err("missing reason".into()),
            };
            let post = values(&words.collect::<Vec<_>>().join(" "), &state)?;
            let mut outbox = Vec::new();
            if *deliveries != "-" {
                for item in deliveries.split(';') {
                    let words: Vec<_> = item.split_whitespace().collect();
                    let explicit =
                        words
                            .first()
                            .and_then(|w| w.parse::<u32>().ok())
                            .and_then(|id| {
                                d.channels
                                    .iter()
                                    .find(|c| c.id == id && c.payload.len() + 1 == words.len())
                            });
                    let (channel, payload) = if let Some(c) = explicit {
                        (c, &words[1..])
                    } else if d.channels.len() == 1 && d.channels[0].payload.len() == words.len() {
                        (&d.channels[0], words.as_slice())
                    } else {
                        return Err("invalid delivery channel or payload arity".into());
                    };
                    let domains: Vec<_> = channel.payload.iter().map(|f| f.domain).collect();
                    outbox.push((channel.id, values(&payload.join(" "), &domains)?));
                }
            }
            Ok(Line {
                number,
                inputs: input,
                class,
                reason,
                post,
                outbox,
            })
        })()
        .map_err(|reason| Error {
            line: number,
            reason,
        })?;
        result.push(row);
    }
    Ok(result)
}
