use order_fulfillment::{
    Authority, Shell, authority, command, create, delivery::Destination, generated::*, invoke,
    journey,
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
            "order-fulfillment-{}-{}",
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

/// Sends one command and returns the outcome.
fn send(
    shell: &mut Shell,
    authority: &Authority,
    action: OrderAction,
    callback_attempt: i128,
    caller: Caller,
    message: &str,
) -> &'static str {
    invoke(
        shell,
        authority,
        command(action, callback_attempt),
        caller,
        message,
    )
    .unwrap()
}

#[test]
fn complete_order_journey() {
    let temp = Temp::new();
    let path = temp.0.join("order.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"bundles\":6,\"pending\":0,\"deliveries\":3"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn a_late_decline_cannot_decline_a_newer_attempt() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("order.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            "c1"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentDeclined,
            1,
            Caller::PaymentProvider,
            "d1"
        ),
        "CommittedFailure"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            "c2"
        ),
        "Accept"
    );
    // The provider delivers its decline of attempt 1 again, late. It names
    // attempt 1, so it cannot decline attempt 2.
    let before = shell.snapshot().unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentDeclined,
            1,
            Caller::PaymentProvider,
            "d1-late"
        ),
        "Reject"
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    // The capture of attempt 2 then pays the order.
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentCaptured,
            2,
            Caller::PaymentProvider,
            "cap2"
        ),
        "Accept"
    );
    let order = Order::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap();
    assert_eq!(order.status, OrderStatus::Paid);
}

#[test]
fn a_repeated_capture_queues_no_second_shipping_request() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("order.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            "checkout"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentCaptured,
            1,
            Caller::PaymentProvider,
            "webhook-1"
        ),
        "Accept"
    );
    // The provider retries the webhook under a new delivery ID. The order has
    // already moved on, so the retry is refused and nothing new is queued.
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentCaptured,
            1,
            Caller::PaymentProvider,
            "webhook-2"
        ),
        "Reject"
    );
    let snapshot = shell.snapshot().unwrap();
    assert_eq!((snapshot.bundle_count(), snapshot.pending_outbox()), (2, 2));
}

#[test]
fn cancelling_while_awaiting_payment_voids_that_attempt() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("order.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            "checkout"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::CancelOrder,
            0,
            Caller::Customer,
            "cancel"
        ),
        "Accept"
    );
    // A capture that arrives after the cancellation is refused; the void
    // request already queued tells the provider to release that attempt.
    assert_eq!(
        send(
            shell,
            &authority,
            OrderAction::PaymentCaptured,
            1,
            Caller::PaymentProvider,
            "late-capture"
        ),
        "Reject"
    );
    let snapshot = shell.snapshot().unwrap();
    let order = Order::try_from_value(snapshot.state().clone()).unwrap();
    assert_eq!(order.status, OrderStatus::Cancelled);
    assert_eq!((snapshot.bundle_count(), snapshot.pending_outbox()), (2, 2));
}
