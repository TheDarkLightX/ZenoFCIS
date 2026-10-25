// Included by the inventory local-simulation assembly's app_journey test.
// Reuses the existing authenticated byte-stream, SQLite and FileReceiver harness.
// These tests do not establish physical shipment, TCP or crash/power-loss safety.
fn inventory_assembly_command(action: u16, quantity: u8) -> Vec<Value> {
    vec![action.to_string().into(), quantity.to_string().into()]
}

#[test]
fn assembly_inventory_authority_and_local_intent() {
    let journey = Journey::new("assembly-inventory-authority").expect("inventory parties fixture");
    assert_eq!(journey.parties.len(), 2, "unexpected authority in profile");
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = serve(&journey, &lineage);
    let operator = journey.credential("operator");
    let observer = journey.credential("observer");
    assert_eq!(
        snapshot(&mut server, &operator).1["values"],
        json!(["0", "0"])
    );
    assert_eq!(snapshot(&mut server, &operator).1["context"], json!(["1"]));
    assert_eq!(snapshot(&mut server, &observer).1["context"], json!(["0"]));
    assert_eq!(
        exchange_as(&mut server, "/state", &"ee".repeat(32), &[]).0,
        401
    );
    // Authentication alone does not confer the operator permission.
    for (index, action) in [150, 151, 152, 153].into_iter().enumerate() {
        let before = ok(application::current(&journey.path));
        let (status, reply, _, _) = send(
            &mut server,
            &journey,
            "observer",
            &inventory_assembly_command(action, 1),
            u8::try_from(index + 1).unwrap(),
        );
        assert_eq!(status, 422);
        assert_eq!(reply["class"], "Reject");
        assert_eq!(ok(application::current(&journey.path)), before);
    }
    for (key, action, quantity, expected) in [
        (10, 153, 3, json!(["3", "0"])),
        (11, 150, 3, json!(["0", "3"])),
    ] {
        assert_eq!(
            send(
                &mut server,
                &journey,
                "operator",
                &inventory_assembly_command(action, quantity),
                key
            )
            .0,
            200
        );
        assert_eq!(snapshot(&mut server, &operator).1["values"], expected);
        assert_eq!(
            received(&journey.receiver),
            0,
            "non-shipment emitted an intent"
        );
    }
    // Insufficient stock and capacity are core decisions, not UI-only guards.
    for (key, action, quantity) in [(12, 150, 1), (13, 153, 3)] {
        if key == 13 {
            assert_eq!(
                send(
                    &mut server,
                    &journey,
                    "operator",
                    &inventory_assembly_command(153, 3),
                    14
                )
                .0,
                200
            );
        }
        let before = ok(application::current(&journey.path));
        assert_eq!(
            send(
                &mut server,
                &journey,
                "operator",
                &inventory_assembly_command(action, quantity),
                key
            )
            .0,
            422
        );
        assert_eq!(ok(application::current(&journey.path)), before);
    }
    assert_eq!(
        snapshot(&mut server, &operator).1["values"],
        json!(["3", "3"])
    );
    assert_eq!(
        send(
            &mut server,
            &journey,
            "operator",
            &inventory_assembly_command(152, 2),
            15
        )
        .0,
        200
    );
    assert_eq!(
        snapshot(&mut server, &operator).1["values"],
        json!(["3", "1"])
    );
    assert_eq!(received(&journey.receiver), 1);
    for entry in ok(std::fs::read_dir(&journey.receiver)) {
        let path = ok(entry).path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let envelope: Value = ok(serde_json::from_slice(&ok(std::fs::read(path))));
            assert_eq!(envelope["destination_text"], "warehouse");
        }
    }
}

#[test]
fn assembly_inventory_completed_request_restart_keeps_one_local_intent() {
    let journey = Journey::new("assembly-inventory-restart").expect("inventory parties fixture");
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = serve(&journey, &lineage);
    for (key, action) in [(20, 153), (21, 150)] {
        assert_eq!(
            send(
                &mut server,
                &journey,
                "operator",
                &inventory_assembly_command(action, 3),
                key
            )
            .0,
            200
        );
    }
    let (status, _, original, packet) = send(
        &mut server,
        &journey,
        "operator",
        &inventory_assembly_command(152, 2),
        22,
    );
    assert_eq!(status, 200);
    assert_eq!(received(&journey.receiver), 1);
    let before = ok(application::current(&journey.path));
    assert_eq!(before.state, vec![0, 1]);
    let files: Vec<_> = ok(std::fs::read_dir(&journey.receiver))
        .map(|entry| {
            let entry = ok(entry);
            (entry.file_name(), ok(std::fs::read(entry.path())))
        })
        .collect();
    drop(server);
    let mut server = serve(&journey, &lineage);
    for _ in 0..2 {
        let (status, bytes) = exchange_as(
            &mut server,
            "/command",
            &journey.credential("operator"),
            &original,
        );
        assert_eq!(status, 200);
        assert_eq!(
            bytes, packet,
            "original request recovery changed its packet"
        );
        assert_eq!(ok(application::current(&journey.path)), before);
        assert_eq!(received(&journey.receiver), 1);
        for (name, bytes) in &files {
            assert_eq!(&ok(std::fs::read(journey.receiver.join(name))), bytes);
        }
    }
    // The same key cannot be reused for a different command, nor by another party.
    assert_ne!(
        send(
            &mut server,
            &journey,
            "operator",
            &inventory_assembly_command(151, 1),
            22
        )
        .0,
        200
    );
    assert_ne!(
        exchange_as(
            &mut server,
            "/command",
            &journey.credential("observer"),
            &original
        )
        .0,
        200
    );
    assert_eq!(ok(application::current(&journey.path)), before);
    assert_eq!(
        snapshot(&mut server, &journey.credential("operator")).1["pending"],
        "0"
    );
}
