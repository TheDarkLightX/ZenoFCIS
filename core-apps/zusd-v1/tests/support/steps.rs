// TEST ONLY: checked state-stepping helpers over the shared request fixtures.
pub(crate) fn step(p: &Policy, s: &mut State, r: Request) -> Prepared {
    let frozen = s.clone();
    let r = sign(p, s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let checked = decide(p, s, &r, &TestVerifier, limits());
    let used = checked.usage();
    let out = checked
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("{:?} -> {e:?}", r.command));
    assert_eq!(s, &frozen);
    assert_eq!(out.usage(), used);
    *s = out.post().clone();
    out
}

pub(crate) fn advance(p: &Policy, s: &mut State, epoch: u64, watermark: u64) {
    let r = advance_request(p, s, epoch, watermark);
    let _ = step(p, s, r);
}

pub(crate) fn price(
    p: &Policy,
    s: &mut State,
    action: OracleAction,
    round: u64,
    value: u128,
    event: u8,
) {
    let r = price_request(p, s, action, round, value, event);
    let _ = step(p, s, r);
}

pub(crate) fn ready(p: &Policy) -> State {
    let mut s = genesis(p);
    advance(p, &mut s, 2, 2);
    price(
        p,
        &mut s,
        OracleAction::InitializeFirstMatured,
        1,
        200_000_000,
        110,
    );
    s
}

pub(crate) fn open(p: &Policy, s: &mut State, who: u8, coll: u128, principal: u128) {
    let mut r = raw(
        s,
        who,
        Command::Open {
            collateral: coll,
            principal,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(p, s, &mut r);
    let _ = step(p, s, r);
}
