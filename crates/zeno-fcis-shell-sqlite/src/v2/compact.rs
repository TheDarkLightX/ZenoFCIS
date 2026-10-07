//! Storage-only bijection; complete core replay still admits every reconstructed byte.
use super::Error;

fn word(bytes: &[u8], offset: usize) -> Result<u128, Error> {
    let end = offset.checked_add(17).ok_or(Error::Range)?;
    let part = bytes.get(offset..end).ok_or(Error::History)?;
    if part[0] != 0 {
        return Err(Error::History);
    }
    Ok(u128::from_be_bytes(
        part[1..].try_into().map_err(|_| Error::History)?,
    ))
}
fn header(bytes: &[u8], magic: u128) -> Result<u128, Error> {
    if word(bytes, 0)? != magic || word(bytes, 17)? != 1 {
        return Err(Error::History);
    }
    let kind = word(bytes, 34)?;
    if kind > 1 {
        return Err(Error::History);
    }
    Ok(kind)
}
fn audited(publication: &[u8]) -> Result<(&[u8], &[u8]), Error> {
    let kind = header(publication, 0x5a505532)?;
    let field = publication.get(51..68).ok_or(Error::History)?;
    if field[0] != 1 {
        return Err(Error::History);
    }
    let length = usize::try_from(u128::from_be_bytes(
        field[1..].try_into().map_err(|_| Error::History)?,
    ))
    .map_err(|_| Error::Range)?;
    let end = 68_usize.checked_add(length).ok_or(Error::Range)?;
    let inner = publication.get(68..end).ok_or(Error::History)?;
    if header(inner, 0x5a525032)? != kind {
        return Err(Error::History);
    }
    Ok((inner, &publication[end..]))
}
fn prefix(publication: &[u8], length: usize, total: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(total).map_err(|_| Error::Range)?;
    bytes.extend_from_slice(&publication[..51]);
    bytes.push(1);
    bytes.extend_from_slice(&(length as u128).to_be_bytes());
    Ok(bytes)
}

/// Remove the checked embedded identity from the inner audited subject.
/// This storage encoding owns its bytes and confers no publication authority.
/// Full suffix validation remains the functional core's exact replay obligation.
pub fn compact_publication(identity: &[u8], publication: &[u8]) -> Result<Vec<u8>, Error> {
    let (inner, tail) = audited(publication)?;
    let field = inner.get(51..68).ok_or(Error::History)?;
    if field[0] != 1 {
        return Err(Error::History);
    }
    let length = usize::try_from(u128::from_be_bytes(
        field[1..].try_into().map_err(|_| Error::History)?,
    ))
    .map_err(|_| Error::Range)?;
    let end = 68_usize.checked_add(length).ok_or(Error::Range)?;
    if inner.get(68..end).ok_or(Error::History)? != identity {
        return Err(Error::Identity);
    }
    let removed = 17_usize.checked_add(length).ok_or(Error::Range)?;
    let size = inner.len().checked_sub(removed).ok_or(Error::Range)?;
    let total = publication.len().checked_sub(removed).ok_or(Error::Range)?;
    let mut bytes = prefix(publication, size, total)?;
    bytes.extend_from_slice(&inner[..51]);
    bytes.extend_from_slice(&inner[end..]);
    bytes.extend_from_slice(tail);
    Ok(bytes)
}

/// Restore the admitted identity into a stored compact publication.
/// The returned bytes are untrusted until the Authority replays them exactly.
pub fn expand_publication(identity: &[u8], compact: &[u8]) -> Result<Vec<u8>, Error> {
    let (inner, tail) = audited(compact)?;
    let added = 17_usize.checked_add(identity.len()).ok_or(Error::Range)?;
    let size = inner.len().checked_add(added).ok_or(Error::Range)?;
    let total = compact.len().checked_add(added).ok_or(Error::Range)?;
    let mut bytes = prefix(compact, size, total)?;
    bytes.extend_from_slice(&inner[..51]);
    bytes.push(1);
    bytes.extend_from_slice(&(identity.len() as u128).to_be_bytes());
    bytes.extend_from_slice(identity);
    bytes.extend_from_slice(&inner[51..]);
    bytes.extend_from_slice(tail);
    Ok(bytes)
}
