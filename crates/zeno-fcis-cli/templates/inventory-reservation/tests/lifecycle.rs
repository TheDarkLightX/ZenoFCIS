use inventory_reservation::{
    authority, create, delivery::Destination, generated::*, invoke, journey, request,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "inventory-reservation-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn complete_stock_journey() {
    let temp = Temp::new();
    let path = temp.0.join("stock.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"available\":0,\"reserved\":2,\"restocked\":5,\"shipped\":3"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn rejections_leave_stock_and_outbox_unchanged() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let mut shell = create(
        &temp.0.join("stock.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            request(StockAction::Restock, 3),
            true,
            "restock"
        )
        .unwrap(),
        "Accept"
    );
    let before = shell.snapshot().unwrap();
    for (n, (action, quantity, authorized)) in [
        // Not authorized.
        (StockAction::Reserve, 1, false),
        // Nothing is reserved.
        (StockAction::Ship, 1, true),
        (StockAction::Release, 1, true),
        // Three more units would exceed the capacity of five.
        (StockAction::Restock, 3, true),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            invoke(
                &mut shell,
                &authority,
                request(action, quantity),
                authorized,
                &format!("refused-{n}")
            )
            .unwrap(),
            "Reject",
            "case {n}"
        );
        assert_eq!(shell.snapshot().unwrap(), before, "case {n}");
    }
}
