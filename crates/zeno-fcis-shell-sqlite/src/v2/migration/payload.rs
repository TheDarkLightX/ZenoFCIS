//! Explicit lossless payload representation maps. Historical deliveries are
//! interpreted by their original catalog, never translated or rewritten here.

use std::collections::BTreeSet;

use zeno_fcis_synthesis::finite::V2InputVariant as Variant;
use zeno_fcis_synthesis::finite::canonical_v2::schema as original;

use super::*;

/// Maximum full old payload records checked for totality and injectivity.
pub const MAX_PAYLOAD_TUPLES: u64 = 1 << 16;
/// Maximum retained canonical result bytes in the injectivity check.
const MAX_MAPPING_BYTES: usize = 64 * 1024 * 1024;

/// The complete new payload record on one channel. Sources refer only to
/// fields of that channel's old payload, never state/command/context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Payload<'m> {
    /// Existing channel ID; channel/destination/order/idempotency stay exact.
    pub channel: u32,
    /// Every new payload field in increasing ID order.
    pub fields: &'m [Target<'m>],
}

/// One consecutive payload representation step used to check a shortcut.
/// An empty map list means identity only on structurally unchanged payloads.
#[derive(Clone, Copy, Debug)]
pub struct PayloadHop<'a, 'p> {
    /// Catalog owning the source representation.
    pub from: &'a BoundCatalog<'p>,
    /// Catalog owning the next representation.
    pub to: &'a BoundCatalog<'p>,
    /// Complete, explicit per-channel maps for this hop.
    pub payloads: &'a [Payload<'a>],
}

/// Format 2 binds the state sources followed by the ordered per-channel
/// payload sources. Format-1 state-only encodings remain byte-identical.
///
/// # Errors
/// `Range` for an unencodable source or a length outside `u32`.
pub fn encode_with_payloads(
    migration: &Migration<'_>,
    payloads: &[Payload<'_>],
) -> Result<Vec<u8>, Range> {
    let mut bytes = MAGIC.to_vec();
    bytes.push(2);
    migration.encode_fields(&mut bytes)?;
    bytes.extend_from_slice(&count(payloads.len())?);
    for payload in payloads {
        bytes.extend_from_slice(&payload.channel.to_be_bytes());
        Migration {
            fields: payload.fields,
        }
        .encode_fields(&mut bytes)?;
    }
    Ok(bytes)
}

fn same_domain(old: c::Domain<'_>, new: c::Domain<'_>) -> bool {
    match (old, new) {
        (c::Domain::Bool, c::Domain::Bool)
        | (c::Domain::Bytes, c::Domain::Bytes)
        | (c::Domain::Text, c::Domain::Text) => true,
        (c::Domain::I128 { min: a, max: b }, c::Domain::I128 { min: c, max: d }) => {
            (a, b) == (c, d)
        }
        (c::Domain::U128 { min: a, max: b }, c::Domain::U128 { min: c, max: d }) => {
            (a, b) == (c, d)
        }
        (
            c::Domain::Enum {
                type_id: a,
                variants: b,
            },
            c::Domain::Enum {
                type_id: c,
                variants: d,
            },
        )
        | (
            c::Domain::Sum {
                type_id: a,
                variants: b,
            },
            c::Domain::Sum {
                type_id: c,
                variants: d,
            },
        ) => a == c && b == d,
        (c::Domain::Canonical { type_id: b, .. }, c::Domain::Canonical { type_id: d, .. }) => {
            b == d
        }
        _ => false,
    }
}

fn same_fields(old: &[c::TypedField<'_>], new: &[c::TypedField<'_>]) -> bool {
    old.len() == new.len()
        && old
            .iter()
            .zip(new)
            .all(|(old, new)| old.field == new.field && same_domain(old.domain, new.domain))
}

/// Original reachable forms, including leaf type IDs and bounds omitted by
/// projected Text/Bytes domains. Names alone do not alter representation.
fn same_original(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>, root: u32) -> bool {
    let (old, new) = (
        from.original_description().definitions,
        to.original_description().definitions,
    );
    let mut remaining = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = remaining.pop() {
        if !seen.insert(id) {
            continue;
        }
        let (Some(old), Some(new)) = (
            old.iter().find(|def| def.id == id),
            new.iter().find(|def| def.id == id),
        ) else {
            return false;
        };
        use original::Kind;
        let same = match (old.kind, new.kind) {
            (Kind::Unit, Kind::Unit) | (Kind::Bool, Kind::Bool) => true,
            (Kind::I128 { min: a, max: b }, Kind::I128 { min: c, max: d }) => (a, b) == (c, d),
            (Kind::U128 { min: a, max: b }, Kind::U128 { min: c, max: d }) => (a, b) == (c, d),
            (Kind::Bytes { min: a, max: b }, Kind::Bytes { min: c, max: d })
            | (Kind::Text { min: a, max: b }, Kind::Text { min: c, max: d }) => (a, b) == (c, d),
            (Kind::Enum(a), Kind::Enum(b)) | (Kind::Sum(a), Kind::Sum(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.id == b.id)
            }
            (Kind::Tuple(a), Kind::Tuple(b)) => {
                remaining.extend_from_slice(a);
                a == b
            }
            (Kind::Record(a), Kind::Record(b)) => {
                remaining.extend(a.iter().map(|field| field.type_id));
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(a, b)| (a.id, a.type_id) == (b.id, b.type_id))
            }
            (Kind::SumPayload(a), Kind::SumPayload(b)) => {
                remaining.extend(a.iter().filter_map(|variant| variant.payload));
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(a, b)| (a.id, a.payload) == (b.id, b.payload))
            }
            (
                Kind::Vector {
                    element: a,
                    min: b,
                    max: c,
                },
                Kind::Vector {
                    element: d,
                    min: e,
                    max: f,
                },
            ) => {
                remaining.push(a);
                (a, b, c) == (d, e, f)
            }
            (
                Kind::Map {
                    key: a,
                    value: b,
                    min: c,
                    max: d,
                },
                Kind::Map {
                    key: e,
                    value: f,
                    min: g,
                    max: h,
                },
            ) => {
                remaining.extend([a, b]);
                (a, b, c, d) == (e, f, g, h)
            }
            _ => false,
        };
        if !same {
            return false;
        }
    }
    true
}

fn channel<'a, 'p>(
    catalog: &'a BoundCatalog<'p>,
    id: u32,
) -> Option<(&'a c::Channel<'p>, u32, u32)> {
    let channel = catalog
        .descriptor()
        .channels
        .iter()
        .find(|channel| channel.id == id)?;
    let (_, destination, payload) = catalog
        .channel_roots()
        .iter()
        .find(|(channel, _, _)| *channel == id)?;
    Some((channel, *destination, *payload))
}

fn fields(channel: &c::Channel<'_>) -> Option<Vec<InputField>> {
    channel
        .payload
        .iter()
        .map(|field| {
            let leaf = match field.domain {
                c::Domain::Bool => Leaf::Bool,
                c::Domain::I128 { min, max } => Leaf::I128 { min, max },
                c::Domain::U128 { min, max } => Leaf::U128 { min, max },
                c::Domain::Enum { type_id, variants } | c::Domain::Sum { type_id, variants } => {
                    let mut ids = variants.to_vec();
                    ids.sort_unstable();
                    let variants = ids
                        .into_iter()
                        .enumerate()
                        .map(|(code, id)| {
                            Some(Variant {
                                id,
                                code: i128::try_from(code).ok()?,
                            })
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let max = i128::try_from(variants.len()).ok()?.checked_sub(1)?;
                    if matches!(field.domain, c::Domain::Enum { .. }) {
                        Leaf::Enum {
                            type_id,
                            variants,
                            min: 0,
                            max,
                        }
                    } else {
                        Leaf::Sum {
                            type_id,
                            variants,
                            min: 0,
                            max,
                        }
                    }
                }
                _ => return None,
            };
            Some(InputField {
                id: field.field,
                leaf,
            })
        })
        .collect()
}

fn with_roots<R>(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    id: u32,
    f: impl FnOnce(&Root<'_>, &Root<'_>, &[InputField]) -> Option<R>,
) -> Option<R> {
    let ((old, _, old_id), (new, _, new_id)) = (channel(from, id)?, channel(to, id)?);
    let (old_fields, new_fields) = (fields(old)?, fields(new)?);
    let binding = |catalog: &BoundCatalog<'_>, root| FrameBinding {
        root,
        schema: [0; 32],
        max_bytes: catalog.descriptor().limits.limit(Resource::Byte),
    };
    let old = Root::of(Schema::Record(&old_fields), binding(from, old_id))?;
    let new = Root::of(Schema::Record(&new_fields), binding(to, new_id))?;
    f(&old, &new, &old_fields)
}

pub(super) fn validate(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    payloads: &[Payload<'_>],
) -> Result<(), Unsimulated> {
    let invalid = |channel| Unsimulated::Payload { channel };
    if payloads
        .windows(2)
        .any(|pair| pair[0].channel >= pair[1].channel)
    {
        return Err(invalid(0));
    }
    let (old_channels, new_channels) = (from.channel_roots(), to.channel_roots());
    if old_channels.len() != new_channels.len() {
        return Err(Unsimulated::Shape);
    }
    for (old, new) in old_channels.iter().zip(new_channels) {
        if (old.0, old.1) != (new.0, new.1) {
            return Err(Unsimulated::Shape);
        }
        let ((old_channel, _, _), (new_channel, _, _)) = (
            channel(from, old.0).ok_or(Unsimulated::Shape)?,
            channel(to, old.0).ok_or(Unsimulated::Shape)?,
        );
        if !same_domain(old_channel.destination, new_channel.destination)
            || !same_original(from, to, old.1)
        {
            return Err(Unsimulated::Shape);
        }
        let mapping = payloads.iter().find(|mapping| mapping.channel == old.0);
        if let Some(mapping) = mapping {
            let checked = with_roots(from, to, old.0, |old, new, _| {
                let migration = Migration {
                    fields: mapping.fields,
                };
                let size = old.size()?;
                if size > u128::from(MAX_PAYLOAD_TUPLES) {
                    return None;
                }
                // Tables list exactly the declared old scalar domain. An
                // unused out-of-domain case is not accepted metadata either.
                for target in mapping.fields {
                    if let Source::Table { field, cases } = target.source {
                        let position = old.ids.iter().position(|id| *id == field)?;
                        let leaf = old.leaves[position];
                        let (min, max) = bounds(leaf)?;
                        let domain = u128::try_from(max.checked_sub(min)?).ok()?.checked_add(1)?;
                        if u128::try_from(cases.len()).ok()? != domain
                            || cases.iter().any(|(value, _)| code(leaf, *value).is_none())
                        {
                            return None;
                        }
                    }
                }
                fits(&migration, old, new).ok()?;
                carries_every_field(&migration, old).ok()?;
                let mut codes = old.first();
                let mut seen = BTreeSet::new();
                let mut retained = 0usize;
                for _ in 0..size {
                    let mapped = new.payload(&apply(&migration, old, new, &codes).ok()?)?;
                    retained = retained.checked_add(mapped.len())?;
                    if retained > MAX_MAPPING_BYTES || !seen.insert(mapped) {
                        return None;
                    }
                    old.next(&mut codes);
                }
                Some(())
            });
            checked.ok_or_else(|| invalid(mapping.channel))?;
        } else if old.2 != new.2
            || !same_fields(old_channel.payload, new_channel.payload)
            || !same_original(from, to, old.2)
        {
            return Err(invalid(old.0));
        }
    }
    if let Some(mapping) = payloads
        .iter()
        .find(|mapping| !old_channels.iter().any(|(id, _, _)| *id == mapping.channel))
    {
        return Err(invalid(mapping.channel));
    }
    Ok(())
}

/// Translate one canonical old payload value through the explicit map. This
/// pure function changes no stored historical delivery or replay identity.
/// Callers admitting a migration must also run `simulate_with_payloads`.
///
/// # Errors
/// An unsupported schema, invalid canonical payload or invalid mapped value.
pub fn migrate_payload(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    mapping: &Payload<'_>,
    bytes: &[u8],
) -> Result<Vec<u8>, Unsimulated> {
    with_roots(from, to, mapping.channel, |old, new, fields| {
        let migration = Migration {
            fields: mapping.fields,
        };
        fits_shape(&migration, old, new).ok()?;
        carries_every_field(&migration, old).ok()?;
        let codes = old.decode_payload(bytes, fields)?;
        new.payload(&apply(&migration, old, new, &codes).ok()?)
    })
    .ok_or(Unsimulated::Payload {
        channel: mapping.channel,
    })
}

pub(super) fn exact_delivery(old: &WireDelivery, new: &WireDelivery) -> bool {
    same_delivery(old, new)
        && old.payload_root() == new.payload_root()
        && old.payload() == new.payload()
}

fn same_delivery(old: &WireDelivery, new: &WireDelivery) -> bool {
    old.ordinal() == new.ordinal()
        && old.channel() == new.channel()
        && old.destination_root() == new.destination_root()
        && old.destination() == new.destination()
        && old.idempotency() == new.idempotency()
}

pub(super) fn delivery_agrees(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    payloads: &[Payload<'_>],
    old: &WireDelivery,
    new: &WireDelivery,
) -> bool {
    if !same_delivery(old, new) {
        return false;
    }
    match payloads
        .iter()
        .find(|mapping| mapping.channel == old.channel())
    {
        None => exact_delivery(old, new),
        Some(mapping) => {
            let roots = channel(from, old.channel()).zip(channel(to, old.channel()));
            roots.is_some_and(|((_, _, old_root), (_, _, new_root))| {
                old.payload_root() == old_root && new.payload_root() == new_root
            }) && migrate_payload(from, to, mapping, old.payload()).as_deref() == Ok(new.payload())
        }
    }
}

pub(super) fn agrees_route(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    shortcut: &[Payload<'_>],
    route: &[PayloadHop<'_, '_>],
) -> Result<(), Unsimulated> {
    validate(from, to, shortcut)?;
    let Some(first) = route.first() else {
        return Err(Unsimulated::Shape);
    };
    let Some(last) = route.last() else {
        return Err(Unsimulated::Shape);
    };
    let same_catalog = |old: &BoundCatalog<'_>, new: &BoundCatalog<'_>| {
        old.original_schema() == new.original_schema()
            && old.original_contract() == new.original_contract()
    };
    if !same_catalog(first.from, from)
        || !same_catalog(last.to, to)
        || route
            .windows(2)
            .any(|hops| !same_catalog(hops[0].to, hops[1].from))
    {
        return Err(Unsimulated::Shape);
    }
    for hop in route {
        validate(hop.from, hop.to, hop.payloads)?;
    }
    for (id, _, _) in from.channel_roots() {
        let direct = shortcut.iter().find(|mapping| mapping.channel == *id);
        // Every identity hop was established above from the linked root and
        // complete typed payload schema; no arbitrary byte domain is sampled.
        if direct.is_none()
            && route
                .iter()
                .all(|hop| hop.payloads.iter().all(|map| map.channel != *id))
        {
            continue;
        }
        let checked = with_roots(from, to, *id, |old, new, _| {
            let size = old.size()?;
            if size > u128::from(MAX_PAYLOAD_TUPLES) {
                return None;
            }
            let mut codes = old.first();
            for _ in 0..size {
                let bytes = old.payload(&codes)?;
                let mapped = match direct {
                    Some(mapping) => new.payload(
                        &apply(
                            &Migration {
                                fields: mapping.fields,
                            },
                            old,
                            new,
                            &codes,
                        )
                        .ok()?,
                    )?,
                    None => bytes.clone(),
                };
                let mut composed = bytes;
                for hop in route {
                    if let Some(mapping) =
                        hop.payloads.iter().find(|mapping| mapping.channel == *id)
                    {
                        composed = migrate_payload(hop.from, hop.to, mapping, &composed).ok()?;
                    }
                }
                if mapped != composed {
                    return None;
                }
                old.next(&mut codes);
            }
            Some(())
        });
        checked.ok_or(Unsimulated::Payload { channel: *id })?;
    }
    Ok(())
}
