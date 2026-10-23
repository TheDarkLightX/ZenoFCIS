//! The actual emitted application behind the HTTP parser, over local byte streams.
mod support;
use application::app::{Application, identity};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_ingress::{Config, Server, Trace, TraceSink, encode_command};
use zeno_fcis_shell_sqlite::collections::CollectionStore;

struct Sink;
impl TraceSink for Sink {
    fn record(&mut self, _: Trace) {}
}
fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|e| panic!("{e:?}"))
}
fn fixture() -> Value {
    ok(serde_json::from_str(include_str!(
        "collection-journey.json"
    )))
}
#[test]
fn full_map_capacity_refuses_without_publication() {
    let fixture = fixture();
    let Some(capacity) = fixture["capacity_regression"].as_u64() else {
        return;
    };
    assert!(matches!(capacity, 1 | 2));
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let context: Vec<String> = ok(serde_json::from_value(fixture["contexts"][0].clone()));
    let parties = || vec![(Hash32::new([7; 32]), context.clone())];
    let directory = configuration_directory();
    let path = directory.join("capacity.sqlite");
    let mut app = ok(Application::open(&authority, &path, parties()));
    assert_eq!(app.interface()["maps"][0]["capacity"], capacity.to_string());
    let first = [0; 32];
    let mut second = first;
    second[31] = 1;
    let mint = |key: &[u8; 32], amount: &str| {
        vec![
            "150".into(),
            application::app::hex(&first),
            application::app::hex(key),
            amount.into(),
        ]
    };
    let binding = |n: u8| zeno_fcis_shell_sqlite::collections::RequestBinding {
        key: [n; 32],
        request: [n + 20; 32],
    };
    let snapshot = |app: &Application<'_, '_>| {
        (
            ok(app.store().version()),
            ok(app.store().scalar_state()),
            ok(app.store().root(110)),
            ok(app.store().entries(110)),
            ok(app.store().commits()),
        )
    };
    let first_result = ok(app.decide(
        Hash32::new([7; 32]),
        0,
        &mint(&first, "0"),
        Some(binding(1)),
    ));
    assert_eq!(first_result["reply"]["class"], "Accept");
    assert_eq!(ok(app.store().len(110)), 1);
    let before = snapshot(&app);
    let preview = ok(app.decide(Hash32::new([7; 32]), 1, &mint(&second, "0"), None));
    assert_eq!(
        preview["reply"]["class"], "Accept",
        "capacity is outside the view decision"
    );
    assert_eq!(snapshot(&app), before, "preview changed publication state");
    let second_result = app.decide(
        Hash32::new([7; 32]),
        1,
        &mint(&second, "0"),
        Some(binding(2)),
    );
    if capacity == 1 {
        assert!(
            matches!(second_result, Err(zeno_fcis_ingress::Refusal::Unavailable)),
            "full map insertion was committed: {second_result:?}"
        );
        assert_eq!(
            snapshot(&app),
            before,
            "capacity refusal changed publication state"
        );
        assert!(ok(app.store().request(&binding(2).key)).is_none());
    } else {
        assert_eq!(ok(second_result)["reply"]["class"], "Accept");
        assert_eq!(ok(app.store().len(110)), 2);
    }
    let revision = ok(app.store().version());
    let update = ok(app.decide(
        Hash32::new([7; 32]),
        revision,
        &mint(&first, "1"),
        Some(binding(3)),
    ));
    assert_eq!(
        update["reply"]["class"], "Accept",
        "existing-key update at capacity refused"
    );
    assert_eq!(ok(app.store().len(110)), capacity);
    let after = snapshot(&app);
    assert!(
        app.audit().is_ok(),
        "capacity path failed independent replay"
    );
    drop(app);
    let reopened = ok(Application::open(&authority, &path, parties()));
    assert_eq!(
        snapshot(&reopened),
        after,
        "capacity path changed after restart"
    );
    drop(reopened);
    ok(std::fs::remove_dir_all(directory));
}
fn command(step: &Value) -> Vec<String> {
    ok(serde_json::from_value(step["command"].clone()))
}
fn http(
    server: &mut Server<Application<'_, '_>, Sink>,
    route: &str,
    body: &[u8],
    secret: &str,
) -> (u16, Vec<u8>) {
    let mut request = format!("POST {route} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {secret}\r\nContent-Length: {}\r\n\r\n",body.len()).into_bytes();
    request.extend_from_slice(body);
    let response = support::exchange(server, request, false);
    let split = response
        .windows(4)
        .position(|x| x == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("HTTP response"));
    let status = std::str::from_utf8(&response[..split])
        .ok()
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, response[split + 4..].to_vec())
}
fn party_file(directory: &Path, write_credentials: bool) -> PathBuf {
    let contexts: Vec<Vec<String>> = ok(serde_json::from_value(fixture()["contexts"].clone()));
    let rows: Vec<_> = contexts.into_iter().enumerate().map(|(i, context)| {
        let file = format!("party-{i}.secret");
        if write_credentials {
            ok(std::fs::write(directory.join(&file), format!("{:02x}", 0xab + i as u8).repeat(32)));
        }
        serde_json::json!({"principal":application::app::hex(&[7+i as u8;32]),"context":context,"secret_file":file})
    }).collect();
    let path = directory.join("parties.json");
    ok(std::fs::write(&path, ok(serde_json::to_vec(&rows))));
    path
}
fn configuration_directory() -> PathBuf {
    let nonce = ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)).as_nanos();
    let path =
        std::env::temp_dir().join(format!("collection-parties-{}-{nonce}", std::process::id()));
    ok(std::fs::create_dir(&path));
    path
}
fn server_configuration(
    authority: &zeno_fcis_synthesis::finite::v2_authority::Authority<'_>,
) -> (Config, Vec<(Hash32, Vec<String>)>) {
    let directory = configuration_directory();
    let path = party_file(&directory, true);
    let settings = ok(application::app::load_parties(&path));
    let credentials = ok(application::app::read_credentials(&settings));
    let parties = settings
        .into_iter()
        .map(|party| (party.principal, party.context))
        .collect();
    let config = ok(Config::with_parties(
        credentials,
        identity(authority),
        "http://localhost".into(),
        std::time::Duration::from_secs(10),
    ));
    ok(std::fs::remove_dir_all(directory));
    (config, parties)
}
fn make_server<'a, 'p>(
    authority: &'a zeno_fcis_synthesis::finite::v2_authority::Authority<'p>,
) -> Server<Application<'a, 'p>, Sink> {
    let (config, parties) = server_configuration(authority);
    let endpoint = ok(Application::with_store(
        authority,
        ok(CollectionStore::in_memory()),
        parties,
    ));
    Server::new(config, endpoint, Sink)
}
fn configured_server<'a, 'p>(
    authority: &'a zeno_fcis_synthesis::finite::v2_authority::Authority<'p>,
    endpoint: Application<'a, 'p>,
) -> Server<Application<'a, 'p>, Sink> {
    let (config, _) = server_configuration(authority);
    Server::new(config, endpoint, Sink)
}
#[test]
fn party_configuration_uses_existing_ingress_and_application_guards() {
    let directory = configuration_directory();
    let path = party_file(&directory, true);
    let valid: Value = ok(serde_json::from_slice(&ok(std::fs::read(&path))));
    let settings = ok(application::app::load_parties(&path));
    assert_eq!(settings.len(), 2, "registered role-spanning configuration");
    let credentials = ok(application::app::read_credentials(&settings));
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let ingress = |credentials| {
        Config::with_parties(
            credentials,
            identity(&authority),
            "http://localhost".into(),
            std::time::Duration::from_secs(10),
        )
    };
    let mut duplicate = credentials.clone();
    duplicate[1].0 = duplicate[0].0;
    assert!(ingress(duplicate).is_err(), "duplicate credential accepted");
    let mut zero = credentials;
    zero[0].0 = [0; 32];
    assert!(ingress(zero).is_err(), "zero credential accepted");
    let mut bad_context: Vec<_> = settings
        .iter()
        .map(|p| (p.principal, p.context.clone()))
        .collect();
    bad_context[0].1 = vec!["999".into()];
    assert!(
        Application::with_store(&authority, ok(CollectionStore::in_memory()), bad_context).is_err(),
        "out-of-domain context accepted"
    );
    let mut parties: Vec<_> = settings
        .into_iter()
        .map(|p| (p.principal, p.context))
        .collect();
    parties[1].0 = parties[0].0;
    assert!(
        Application::with_store(&authority, ok(CollectionStore::in_memory()), parties).is_err(),
        "duplicate principal accepted"
    );
    let mut extra = valid.clone();
    extra[0]["extra"] = serde_json::json!(0);
    let mut principal = valid.clone();
    principal[0]["principal"] = serde_json::json!("0");
    let mut context = valid.clone();
    context[0]["context"] = serde_json::json!([false]);
    for value in [
        extra,
        principal,
        context,
        serde_json::json!([]),
        Value::Array(vec![valid[0].clone(); zeno_fcis_ingress::MAX_PARTIES + 1]),
    ] {
        ok(std::fs::write(&path, ok(serde_json::to_vec(&value))));
        assert!(
            application::app::load_parties(&path).is_err(),
            "malformed party configuration accepted"
        );
    }
    let mut large = valid;
    large[0]["secret_file"] = serde_json::json!("x".repeat(65_536));
    ok(std::fs::write(&path, ok(serde_json::to_vec(&large))));
    assert!(
        matches!(application::app::load_parties(&path), Err(e) if e == "configuration size"),
        "oversized party configuration accepted"
    );
    let path = party_file(&directory, true);
    ok(std::fs::write(
        directory.join("party-0.secret"),
        "ab".repeat(31),
    ));
    assert!(
        application::app::read_credentials(&ok(application::app::load_parties(&path))).is_err(),
        "short credential accepted"
    );
    ok(std::fs::remove_dir_all(directory));
}
#[test]
fn collection_app_http_journey() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let mut server = make_server(&authority);
    let fixture = fixture();
    let steps = fixture["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("journey steps"));
    assert!(
        !steps.is_empty(),
        "collection contract needs a registered HTTP journey"
    );
    let mut revision = 0;
    for (index, step) in steps.iter().enumerate() {
        let party = step["party"].as_u64().unwrap_or(0) as u8;
        let secret = format!("{:02x}", 0xab + party).repeat(32);
        let body = encode_command(
            identity(&authority),
            Hash32::new([index as u8 + 1; 32]),
            revision,
            &ok(serde_json::to_vec(&command(step))),
        );
        let preview = http(&mut server, "/preview", &body, &secret);
        assert_eq!(preview.0, 200, "preview {index}: {:?}", preview);
        let preview: Value = ok(serde_json::from_slice(&preview.1));
        assert_eq!(
            preview["reply"]["class"], step["class"],
            "journey preview {index}"
        );
        assert!(
            preview["key_fields"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|f| f["domain"]["kind"] == "key")),
            "preview omitted key fields"
        );
        let response = http(&mut server, "/command", &body, &secret);
        assert_eq!(response.0, 200, "command {index}: {:?}", response);
        let report: Value = ok(serde_json::from_slice(&response.1));
        assert_eq!(
            report["reply"], preview["reply"],
            "HTTP preview and command disagree"
        );
        assert_eq!(report["loads"], step["loads"], "named-key load count");
        if step["class"] != "Reject" {
            revision += 1;
            assert_eq!(report["revision"], revision.to_string());
            let retried = http(&mut server, "/command", &body, &secret);
            assert_eq!(retried.0, 200, "exact retry");
            let retried: Value = ok(serde_json::from_slice(&retried.1));
            assert_eq!(
                retried["receipt"], report["receipt"],
                "retry changed receipt"
            );
        }
    }
    let (app, _) = server.into_parts();
    assert_eq!(ok(app.store().version()), revision);
}
#[test]
fn malformed_keys_are_ingress_refusals() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let mut server = make_server(&authority);
    let good = command(&fixture()["steps"][0]);
    let key = good
        .iter()
        .position(|s| s.len() == 64)
        .unwrap_or_else(|| panic!("key field"));
    for invalid in [
        "",
        "0",
        &"aa".repeat(31),
        &"AA".repeat(32),
        &"gg".repeat(32),
        &"aa".repeat(33),
    ] {
        let mut values = good.clone();
        values[key] = invalid.into();
        let body = encode_command(
            identity(&authority),
            Hash32::new([1; 32]),
            0,
            &ok(serde_json::to_vec(&values)),
        );
        for route in ["/preview", "/command"] {
            let result = http(&mut server, route, &body, &"ab".repeat(32));
            assert_eq!(
                result,
                (400, b"Malformed".to_vec()),
                "malformed key escaped ingress: {invalid:?}"
            );
        }
    }
    let (app, _) = server.into_parts();
    assert_eq!(ok(app.store().version()), 0);
}
#[test]
fn shell_unnamed_write_is_refused() {
    let mut store = ok(CollectionStore::in_memory());
    let named = BTreeMap::from([(110, BTreeSet::from([[1; 32]]))]);
    let result = store.commit(
        &named,
        &[(110, [2; 32], Some(vec![1]))],
        b"decision",
        b"scalars",
    );
    assert!(
        matches!(
            result,
            Err(zeno_fcis_shell_sqlite::collections::Error::Framing { .. })
        ),
        "shell un-named write was accepted"
    );
    assert_eq!(ok(store.version()), 0);
    assert_eq!(ok(store.get(110, &[2; 32])), None);
}
#[test]
fn self_transfer_preserves_balance_and_supply() {
    let fixture = fixture();
    let Some(steps) = fixture["self_transfer"].as_array() else {
        return;
    };
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let mut server = make_server(&authority);
    for (i, step) in steps.iter().enumerate() {
        let body = encode_command(
            identity(&authority),
            Hash32::new([i as u8 + 1; 32]),
            i as u64,
            &ok(serde_json::to_vec(&command(step))),
        );
        let response = http(&mut server, "/command", &body, &"ab".repeat(32));
        assert_eq!(response.0, 200);
        let report: Value = ok(serde_json::from_slice(&response.1));
        assert_eq!(
            report["reply"]["class"], "Accept",
            "self transfer must remain an accepted no-op"
        );
    }
    let (app, _) = server.into_parts();
    let state: BTreeMap<u16, String> = ok(serde_json::from_slice(&ok(app.store().scalar_state())));
    assert_eq!(
        state.get(&120),
        Some(&"3".into()),
        "self transfer changed supply"
    );
    let key = zeno_fcis_contract::collections::runtime::decode_key(&"11".repeat(32))
        .unwrap_or_else(|| panic!("fixture key"));
    let record = ok(app.store().get(110, &key)).unwrap_or_else(|| panic!("balance entry"));
    assert_eq!(
        zeno_fcis_contract::collections::runtime::decode_record(&record),
        Some(vec![
            zeno_fcis_contract::collections::runtime::RecordValue::Number(3)
        ]),
        "self transfer changed balance"
    );
    assert_eq!(ok(app.store().len(110)), 1);
}

#[test]
fn request_loads_only_named_keys_at_both_sizes() {
    use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
    use zeno_fcis_contract::collections::runtime::{RecordValue, decode_key, encode_record};
    let fixture = fixture();
    let Some(records) = fixture["benchmark_records"].as_object() else {
        return;
    };
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    for size in [10u64, 10_000] {
        let context: Vec<String> = ok(serde_json::from_value(fixture["contexts"][0].clone()));
        let mut app = ok(Application::with_store(
            &authority,
            ok(CollectionStore::in_memory()),
            vec![(Hash32::new([7; 32]), context.clone())],
        ));
        let mut store = app.into_store();
        let mut named: BTreeMap<u32, BTreeSet<[u8; 32]>> = BTreeMap::new();
        let mut writes = Vec::new();
        for (map, values) in records {
            let map: u32 = ok(map.parse());
            let values: Vec<String> = ok(serde_json::from_value(values.clone()));
            let record = encode_record(
                &values
                    .iter()
                    .map(|v| match decode_key(v) {
                        Some(k) => RecordValue::Key(k),
                        None => RecordValue::Number(ok(v.parse())),
                    })
                    .collect::<Vec<_>>(),
            );
            for index in 0..size {
                let key = *RustCryptoSha256::hash(&index.to_be_bytes()).as_bytes();
                named.entry(map).or_default().insert(key);
                writes.push((map, key, Some(record.clone())));
            }
        }
        let scalar_state = ok(store.scalar_state());
        ok(store.commit(&named, &writes, b"benchmark setup", &scalar_state));
        app = ok(Application::with_store(
            &authority,
            store,
            vec![(Hash32::new([7; 32]), context)],
        ));
        let command: Vec<String> = ok(serde_json::from_value(fixture["benchmark_command"].clone()));
        let mut elapsed = Vec::new();
        for _ in 0..32 {
            let started = std::time::Instant::now();
            let report = ok(app.decide(Hash32::new([7; 32]), 1, &command, None));
            elapsed.push(started.elapsed().as_nanos());
            assert_eq!(
                report["loads"], 2,
                "frame loaded entries outside the two named keys"
            );
        }
        elapsed.sort_unstable();
        println!(
            "collection_request_cost={{\"kind\":\"preview\",\"entries_per_map\":{size},\"named_loads\":2,\"requests\":32,\"median_ns\":{},\"maximum_ns\":{}}}",
            elapsed[16], elapsed[31]
        );
        let first = hex_key(0);
        let second = hex_key(1);
        let binding = |n: u8| zeno_fcis_shell_sqlite::collections::RequestBinding {
            key: [n; 32],
            request: [n; 32],
        };
        let is_tokens = fixture.get("self_transfer").is_some();
        let mut revision = 1;
        if is_tokens {
            let report = ok(app.decide(
                Hash32::new([7; 32]),
                revision,
                &["150".into(), first.clone(), first.clone(), "1".into()],
                Some(binding(200)),
            ));
            assert_eq!(report["reply"]["class"], "Accept");
            revision += 1;
        }
        let mut elapsed = Vec::new();
        for index in 0..32u8 {
            let command = if is_tokens {
                let (from, to) = if index.is_multiple_of(2) {
                    (&first, &second)
                } else {
                    (&second, &first)
                };
                vec!["151".into(), from.clone(), to.clone(), "1".into()]
            } else {
                vec![
                    if index.is_multiple_of(2) {
                        "152"
                    } else {
                        "153"
                    }
                    .into(),
                    first.clone(),
                    second.clone(),
                ]
            };
            let started = std::time::Instant::now();
            let report = ok(app.decide(
                Hash32::new([7; 32]),
                revision,
                &command,
                Some(binding(index + 1)),
            ));
            elapsed.push(started.elapsed().as_nanos());
            assert_eq!(report["reply"]["class"], "Accept");
            assert_eq!(
                report["loads"], 2,
                "commit frame loaded entries outside the two named keys"
            );
            revision += 1;
        }
        elapsed.sort_unstable();
        println!(
            "collection_request_cost={{\"kind\":\"commit\",\"entries_per_map\":{size},\"named_loads\":2,\"requests\":32,\"median_ns\":{},\"maximum_ns\":{}}}",
            elapsed[16], elapsed[31]
        );
    }
}
fn hex_key(index: u64) -> String {
    use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
    application::app::hex(RustCryptoSha256::hash(&index.to_be_bytes()).as_bytes())
}

#[test]
fn collection_receipts_replay_and_restart() {
    let fixture = fixture();
    let contexts: Vec<Vec<String>> = ok(serde_json::from_value(fixture["contexts"].clone()));
    let parties = || {
        contexts
            .iter()
            .enumerate()
            .map(|(i, c)| (Hash32::new([7 + i as u8; 32]), c.clone()))
            .collect()
    };
    let nonce = ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)).as_nanos();
    let directory =
        std::env::temp_dir().join(format!("collection-restart-{}-{nonce}", std::process::id()));
    ok(std::fs::create_dir(&directory));
    let path = directory.join("store.sqlite");
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let app = ok(Application::open(&authority, &path, parties()));
    let initial = ok(app.store().scalar_state());
    let mut server = configured_server(&authority, app);
    let mut revision = 0;
    let mut receipts = Vec::new();
    for (index, step) in fixture["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let body = encode_command(
            identity(&authority),
            Hash32::new([index as u8 + 1; 32]),
            revision,
            &ok(serde_json::to_vec(&command(step))),
        );
        let party = step["party"].as_u64().unwrap_or(0) as u8;
        let (status, bytes) = http(
            &mut server,
            "/command",
            &body,
            &format!("{:02x}", 0xab + party).repeat(32),
        );
        assert_eq!(status, 200, "persistent command HTTP refusal");
        let report: Value = ok(serde_json::from_slice(&bytes));
        if step["class"] != "Reject" {
            revision += 1;
            receipts.push(report["receipt"].clone());
        }
    }
    let (app, _) = server.into_parts();
    let journal = ok(app.journal());
    assert_eq!(journal.len(), revision as usize);
    let mut previous = application::app::hex(&initial);
    for (row, receipt) in journal.iter().zip(&receipts) {
        assert_eq!(row["receipt"], *receipt);
        assert_eq!(
            receipt["pre_scalars"], previous,
            "receipt omitted scalar pre-state"
        );
        previous = receipt["post_scalars"]
            .as_str()
            .unwrap_or_else(|| panic!("scalar post-state"))
            .into();
    }
    let fresh = || {
        ok(Application::with_store(
            &authority,
            ok(CollectionStore::in_memory()),
            parties(),
        ))
    };
    let mut replay = fresh();
    let result = replay.replay(&journal);
    assert!(
        result.is_ok(),
        "genuine journal failed independent named-root replay: {result:?}"
    );
    assert_eq!(ok(replay.journal()), journal, "replay changed receipts");
    let maps: Vec<u32> = app.interface()["maps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| m["id"].as_u64().unwrap_or(0) as u32)
        .collect();
    for map in &maps {
        assert_eq!(ok(app.store().root(*map)), ok(replay.store().root(*map)));
        assert_eq!(
            ok(app.store().entries(*map)),
            ok(replay.store().entries(*map))
        );
    }
    let mut corrupt = journal.clone();
    let post = corrupt[0]["receipt"]["post_roots"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("post roots"));
    let map = post
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| panic!("named root"));
    post.remove(&map);
    let mut target = fresh();
    assert!(
        target.replay(&corrupt).is_err(),
        "replay accepted a receipt missing a named post root"
    );
    // Offline operations use configured contexts; credential files are deliberately absent.
    let party_path = party_file(&directory, false);
    let run = |arguments: &[&std::ffi::OsStr]| {
        ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
            .env("ZENO_APP_PARTIES", &party_path)
            .env_remove("ZENO_APP_SECRET_FILE")
            .env_remove("ZENO_APP_CONTEXT")
            .args(arguments)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output())
    };
    let legacy = ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
        .args([std::ffi::OsStr::new("--audit"), path.as_os_str()])
        .env_remove("ZENO_APP_PARTIES")
        .env_remove("ZENO_APP_CONTEXT")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output());
    assert!(
        legacy.status.success(),
        "legacy single-context recovery failed: {}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let exported = run(&[std::ffi::OsStr::new("--journal"), path.as_os_str()]);
    assert!(
        exported.status.success(),
        "journal export failed: {}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let document: Value = ok(serde_json::from_slice(&exported.stdout));
    let journal_path = directory.join("journal.json");
    ok(std::fs::write(&journal_path, &exported.stdout));
    let restored_path = directory.join("restored.sqlite");
    let restored = run(&[
        std::ffi::OsStr::new("--replay"),
        journal_path.as_os_str(),
        restored_path.as_os_str(),
    ]);
    assert!(
        restored.status.success(),
        "journal restore failed: {}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: Value = ok(serde_json::from_slice(&restored.stdout));
    assert_eq!(restored["head"], document["head"], "restore head differs");
    let audited = run(&[std::ffi::OsStr::new("--audit"), restored_path.as_os_str()]);
    assert!(
        audited.status.success(),
        "restored application failed audit"
    );
    let mut bad_document = document.clone();
    bad_document["entries"] = serde_json::to_value(&corrupt).unwrap_or(Value::Null);
    let bad_journal = directory.join("bad-journal.json");
    ok(std::fs::write(
        &bad_journal,
        ok(serde_json::to_vec(&bad_document)),
    ));
    let refused_path = directory.join("refused.sqlite");
    let refused = run(&[
        std::ffi::OsStr::new("--replay"),
        bad_journal.as_os_str(),
        refused_path.as_os_str(),
    ]);
    assert!(
        !refused.status.success(),
        "CLI restored a receipt missing a named map root"
    );
    assert!(
        !refused_path.exists(),
        "failed replay published a destination"
    );
    let before = ok(std::fs::read(&restored_path));
    let refused = run(&[
        std::ffi::OsStr::new("--replay"),
        journal_path.as_os_str(),
        restored_path.as_os_str(),
    ]);
    assert!(
        !refused.status.success(),
        "restore overwrote an existing destination"
    );
    assert_eq!(
        ok(std::fs::read(&restored_path)),
        before,
        "refused restore changed an existing store"
    );
    drop(app);
    let mut reopened = ok(Application::open(&authority, &path, parties()));
    assert_eq!(
        ok(reopened.journal()),
        journal,
        "restart changed roots or receipts"
    );
    for map in &maps {
        assert_eq!(
            ok(reopened.store().root(*map)),
            ok(replay.store().root(*map))
        );
    }
    let resume: Vec<String> = ok(serde_json::from_value(fixture["resume_command"].clone()));
    let resumed = ok(reopened.decide(
        Hash32::new([7; 32]),
        revision,
        &resume,
        Some(zeno_fcis_shell_sqlite::collections::RequestBinding {
            key: [200; 32],
            request: [201; 32],
        }),
    ));
    assert_eq!(
        resumed["reply"]["class"], "Accept",
        "restart did not resume decisions"
    );
    assert!(reopened.audit().is_ok(), "resumed store failed audit");
    drop(reopened);
    ok(std::fs::remove_dir_all(directory));
}
