// Included only by the scalar-task assembly's generated app_journey test.
// Reuses the factory's authenticated byte-stream harness and actual SQLite /
// FileReceiver implementations. It does not qualify TCP or power-loss behavior.
fn assembly_server<'a, 'p, T: zeno_fcis_shell_sqlite::effects::production::Transport>(
    journey: &Journey,
    lineage: &'a Lineage<'a, 'p>,
    transport: T,
) -> Server<Application<'a, 'p, Production<'a, 'p, T, Clock>>, Sink> {
    let authority = &lineage.authorities()[0];
    let table = ok(application::app::party_contexts(
        authority,
        &journey.parties,
        None,
    ));
    let production = ok(Production::open_exclusive(
        &journey.path,
        lineage,
        1,
        transport,
        Some(Clock::default()),
    ));
    let store = production.store();
    let endpoint = ok(Application::with_parties(
        lineage,
        production,
        store,
        &journey.path,
        table,
    ));
    let config = ok(application::app::ingress_config(
        &journey.parties,
        RustCryptoSha256::hash(authority.identity()),
        "http://localhost".into(),
    ));
    Server::new(config, endpoint, Sink::default())
}

fn assembly_command(action: u16, attempt: u8, artifact: u8) -> Vec<Value> {
    vec![
        action.to_string().into(),
        attempt.to_string().into(),
        artifact.to_string().into(),
    ]
}

fn assembly_start_body<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    journey: &Journey,
) -> Vec<u8> {
    let credential = journey.credential("adapter");
    let (_, state) = snapshot(server, &credential);
    let contract = Hash32::new(ok(<[u8; 32]>::try_from(ok(application::app::unhex(
        state["contract"].as_str().unwrap(),
    )))));
    let revision = state["revision"].as_str().unwrap().parse().unwrap();
    let mut inputs = state["values"].as_array().unwrap().clone();
    inputs.extend(assembly_command(161, 1, 1));
    inputs.extend(state["context"].as_array().unwrap().clone());
    let preview = encode_command(
        contract,
        Hash32::new([42; 32]),
        revision,
        &ok(serde_json::to_vec(&inputs)),
    );
    let (status, bytes) = exchange_as(server, "/preview", &credential, &preview);
    assert_eq!(status, 200);
    let preview: Value = ok(serde_json::from_slice(&bytes));
    assert_eq!(preview["reply"]["class"], "Accept");
    encode_command(
        contract,
        Hash32::new([42; 32]),
        revision,
        &ok(application::app::unhex(
            preview["command"].as_str().unwrap(),
        )),
    )
}

#[test]
fn assembly_crash_child() {
    use zeno_fcis_shell::effects::port::{Delivery, Sent};
    use zeno_fcis_shell_sqlite::effects::production::Transport;
    let Some(root) = std::env::var_os("ZENO_ASSEMBLY_CRASH_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let journey = Journey {
        path: root.join("app.sqlite"),
        receiver: root.join("received"),
        parties: ok(application::app::load_parties(&root.join("parties.json"))),
        fixture: Value::Null,
        root,
    };
    struct CrashAfterApply {
        receiver: FileReceiver,
        path: std::path::PathBuf,
    }
    impl Transport for CrashAfterApply {
        fn send(&mut self, delivery: &Delivery) -> Sent {
            // No send before the checked state and outbox have committed.
            assert_eq!(ok(application::current(&self.path)).state, vec![152, 1]);
            assert!(matches!(self.receiver.send(delivery), Sent::Sent(_)));
            // Abrupt process termination: no acknowledgement, reply or Drop.
            std::process::exit(91);
        }
    }
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let transport = CrashAfterApply {
        receiver: ok(FileReceiver::new(&journey.receiver)),
        path: journey.path.clone(),
    };
    let mut server = assembly_server(&journey, &lineage, transport);
    let body = ok(std::fs::read(journey.root.join("original-request")));
    let _ = exchange_as(
        &mut server,
        "/command",
        &journey.credential("adapter"),
        &body,
    );
    panic!("send crash point not reached");
}

#[test]
fn assembly_real_process_loss_retains_delivery_and_original_request() {
    let journey = Journey::new("assembly-process-loss").expect("task parties fixture");
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = serve(&journey, &lineage);
    assert_eq!(
        send(
            &mut server,
            &journey,
            "operator",
            &assembly_command(160, 0, 1),
            41
        )
        .0,
        200
    );
    let original = assembly_start_body(&mut server, &journey);
    ok(std::fs::write(
        journey.root.join("original-request"),
        &original,
    ));
    drop(server);
    let child = ok(std::process::Command::new(ok(std::env::current_exe()))
        .args(["--exact", "assembly_crash_child", "--nocapture"])
        .env("ZENO_ASSEMBLY_CRASH_TEST_ROOT", &journey.root)
        .status());
    assert_eq!(
        child.code(),
        Some(91),
        "child did not terminate at the intended boundary"
    );
    assert_eq!(received(&journey.receiver), 1);
    let before_files: Vec<_> = ok(std::fs::read_dir(&journey.receiver))
        .map(|entry| {
            let entry = ok(entry);
            (entry.file_name(), ok(std::fs::read(entry.path())))
        })
        .collect();
    let mut server = serve(&journey, &lineage);
    let credential = journey.credential("adapter");
    let (_, before) = snapshot(&mut server, &credential);
    assert_eq!(before["values"], json!(["152", "1"]));
    assert_eq!(
        before["pending"], "1",
        "crash incorrectly acknowledged delivery"
    );
    let (status, bytes) = exchange_as(&mut server, "/command", &credential, &original);
    assert_eq!(status, 200);
    let first: Value = ok(serde_json::from_slice(&bytes));
    assert_eq!(
        received(&journey.receiver),
        1,
        "restart duplicated receiver effect"
    );
    for (name, bytes) in before_files {
        assert_eq!(ok(std::fs::read(journey.receiver.join(name))), bytes);
    }
    let (_, after) = snapshot(&mut server, &credential);
    assert_eq!(after["pending"], "0");
    assert_eq!(
        after["revision"], before["revision"],
        "replay published again"
    );
    let (status, bytes) = exchange_as(&mut server, "/command", &credential, &original);
    assert_eq!(status, 200);
    let repeated: Value = ok(serde_json::from_slice(&bytes));
    assert_eq!(first["packet"], repeated["packet"]);
    // Another party cannot recover this request under its own identity.
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
}

#[test]
fn assembly_timeout_is_unknown_and_correlated_observations_use_authority() {
    use zeno_fcis_shell::effects::port::{Delivery, Fault, Sent};
    use zeno_fcis_shell_sqlite::effects::production::Transport;
    struct Unavailable;
    impl Transport for Unavailable {
        fn send(&mut self, _: &Delivery) -> Sent {
            Sent::Unknown(Fault::Io)
        }
    }
    let journey = Journey::new("assembly-domain-observations").expect("task parties fixture");
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = assembly_server(&journey, &lineage, Unavailable);
    assert_eq!(
        exchange_as(&mut server, "/state", &"ff".repeat(32), &[]).0,
        401
    );
    assert_eq!(
        send(
            &mut server,
            &journey,
            "observer",
            &assembly_command(160, 0, 1),
            1
        )
        .0,
        422
    );
    assert_eq!(
        send(
            &mut server,
            &journey,
            "operator",
            &assembly_command(160, 0, 1),
            2
        )
        .0,
        200
    );
    let (status, _, _, bytes) = send(
        &mut server,
        &journey,
        "adapter",
        &assembly_command(161, 1, 1),
        3,
    );
    assert_eq!(status, 200);
    let reply: Value = ok(serde_json::from_slice(&bytes));
    assert_eq!(reply["delivery"], "pending");
    let credential = journey.credential("adapter");
    let (_, state) = snapshot(&mut server, &credential);
    assert_eq!(
        state["values"],
        json!(["152", "1"]),
        "transport uncertainty invented a business outcome"
    );
    for (key, party, command) in [
        (4, "operator", assembly_command(162, 1, 1)),
        (5, "adapter", assembly_command(162, 0, 1)),
        (6, "adapter", assembly_command(162, 1, 2)),
    ] {
        let before = ok(application::current(&journey.path));
        assert_eq!(send(&mut server, &journey, party, &command, key).0, 422);
        assert_eq!(ok(application::current(&journey.path)), before);
    }
    assert_eq!(
        send(
            &mut server,
            &journey,
            "adapter",
            &assembly_command(164, 1, 1),
            7
        )
        .0,
        200
    );
    assert_eq!(
        snapshot(&mut server, &credential).1["values"],
        json!(["155", "1"])
    );
    drop(server);
    let mut server = assembly_server(&journey, &lineage, Unavailable);
    let (_, reopened) = snapshot(&mut server, &credential);
    assert_eq!(
        reopened["values"],
        json!(["155", "1"]),
        "restart erased Unknown"
    );
    assert_eq!(reopened["pending"], "1");
    assert_eq!(
        send(
            &mut server,
            &journey,
            "operator",
            &assembly_command(166, 1, 1),
            8
        )
        .0,
        422,
        "Unknown allowed unsafe new attempt"
    );
    // Late success is an authenticated observation, not inferred from delivery ack.
    assert_eq!(
        send(
            &mut server,
            &journey,
            "adapter",
            &assembly_command(162, 1, 1),
            9
        )
        .0,
        200
    );
    assert_eq!(
        snapshot(&mut server, &credential).1["values"],
        json!(["153", "1"])
    );
    let before = ok(application::current(&journey.path));
    assert_eq!(
        send(
            &mut server,
            &journey,
            "adapter",
            &assembly_command(162, 1, 1),
            10
        )
        .0,
        422
    );
    assert_eq!(ok(application::current(&journey.path)), before);
}

#[test]
fn assembly_old_pending_delivery_cannot_complete_new_attempt() {
    use zeno_fcis_shell::effects::port::{Delivery, Fault, Sent};
    use zeno_fcis_shell_sqlite::effects::production::Transport;
    struct Unavailable;
    impl Transport for Unavailable {
        fn send(&mut self, _: &Delivery) -> Sent {
            Sent::Unknown(Fault::Io)
        }
    }
    let journey = Journey::new("assembly-attempt-binding").expect("task parties fixture");
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = assembly_server(&journey, &lineage, Unavailable);
    for (key, party, command) in [
        (20, "operator", assembly_command(160, 0, 1)),
        (21, "adapter", assembly_command(161, 1, 1)),
        // This is a trusted definitive task observation, NOT a send failure.
        (22, "adapter", assembly_command(163, 1, 1)),
        (23, "operator", assembly_command(166, 1, 1)),
        (24, "adapter", assembly_command(161, 2, 1)),
    ] {
        assert_eq!(send(&mut server, &journey, party, &command, key).0, 200);
    }
    let credential = journey.credential("adapter");
    let (_, state) = snapshot(&mut server, &credential);
    assert_eq!(state["values"], json!(["152", "2"]));
    assert_eq!(state["pending"], "2", "old delivery was silently forgotten");
    let before = ok(application::current(&journey.path));
    assert_eq!(
        send(
            &mut server,
            &journey,
            "adapter",
            &assembly_command(162, 1, 1),
            25
        )
        .0,
        422
    );
    assert_eq!(
        ok(application::current(&journey.path)),
        before,
        "stale attempt report completed the new attempt"
    );
    assert_eq!(
        send(
            &mut server,
            &journey,
            "adapter",
            &assembly_command(162, 2, 1),
            26
        )
        .0,
        200
    );
    let (_, state) = snapshot(&mut server, &credential);
    assert_eq!(state["values"], json!(["153", "2"]));
    assert_eq!(
        state["pending"], "2",
        "business completion invented transport acknowledgement"
    );
}
