//! Opt-in long-history recovery with streaming I/O and bounded comparisons.
use application::app::{Application, hex, identity};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_shell_sqlite::collections::RequestBinding;

fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|error| panic!("{error:?}"))
}

#[test]
fn collection_long_history_restores_and_continues() {
    let Some(count) = std::env::var("ZENO_COLLECTION_HISTORY_LENGTH").ok() else {
        return;
    };
    let count: u64 = ok(count.parse());
    let seed = std::env::var("ZENO_COLLECTION_HISTORY_SEED").ok();
    let fixture: Value = ok(serde_json::from_str(include_str!(
        "collection-journey.json"
    )));
    let contexts: Vec<Vec<String>> = ok(serde_json::from_value(fixture["contexts"].clone()));
    let parties = || {
        contexts
            .iter()
            .enumerate()
            .map(|(i, c)| (Hash32::new([7 + i as u8; 32]), c.clone()))
            .collect()
    };
    let stamp = ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)).as_nanos();
    let directory =
        std::env::temp_dir().join(format!("collection-history-{}-{stamp}", std::process::id()));
    ok(std::fs::create_dir(&directory));
    let source = directory.join("source.sqlite");
    if let Some(seed) = seed {
        ok(std::fs::copy(seed, &source));
    }
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let started = Instant::now();
    let mut app = ok(Application::open(&authority, &source, parties()));
    let resume: Vec<String> = ok(serde_json::from_value(fixture["resume_command"].clone()));
    let initial = ok(app.store().version());
    assert!(initial <= count);
    for index in initial..count {
        let key =
            RustCryptoSha256::hash_parts(&[b"collection-history-test-key", &index.to_be_bytes()]);
        let result = ok(app.decide(
            Hash32::new([7; 32]),
            index,
            &resume,
            Some(RequestBinding {
                key: *key.as_bytes(),
                request: *key.as_bytes(),
            }),
        ));
        assert_eq!(
            result["reply"]["class"], "Accept",
            "long-history command refused at {index}"
        );
    }
    let build_seconds = started.elapsed().as_secs_f64();
    let party_path = directory.join("parties.json");
    let configured: Vec<Value> = contexts
        .iter()
        .enumerate()
        .map(|(i, c)| json!({"principal":hex(&[7+i as u8;32]),"context":c,"secret_file":"unused"}))
        .collect();
    ok(std::fs::write(
        &party_path,
        ok(serde_json::to_vec(&configured)),
    ));
    let command = |args: &[&Path], flag: &str, output: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_app-server"));
        command
            .arg(flag)
            .args(args)
            .env("ZENO_APP_PARTIES", &party_path)
            .env_remove("ZENO_APP_CONTEXT")
            .env_remove("ZENO_APP_SECRET_FILE");
        if let Some(path) = output {
            command.stdout(Stdio::from(ok(std::fs::File::create(path))));
        }
        let result = ok(command.output());
        assert!(
            result.status.success(),
            "history {flag} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    let journal = directory.join("journal.json");
    let started = Instant::now();
    command(&[&source], "--journal", Some(&journal));
    let export_seconds = started.elapsed().as_secs_f64();
    let restored = directory.join("restored.sqlite");
    let started = Instant::now();
    command(&[&journal, &restored], "--replay", None);
    let restore_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let mut replay = ok(Application::open(&authority, &restored, parties()));
    let reopen_seconds = started.elapsed().as_secs_f64();
    assert_eq!(ok(replay.store().version()), count);
    assert_eq!(
        ok(replay.store().scalar_state()),
        ok(app.store().scalar_state())
    );
    let maps = app.interface()["maps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for map in maps {
        let map = map["id"].as_u64().unwrap_or(0) as u32;
        assert_eq!(
            ok(replay.store().entries(map)),
            ok(app.store().entries(map))
        );
        assert_eq!(ok(replay.store().root(map)), ok(app.store().root(map)));
    }
    for sequence in 1..=count {
        assert_eq!(
            ok(replay.journal_entry(sequence)),
            ok(app.journal_entry(sequence)),
            "history row {sequence} changed"
        );
    }
    let result = ok(replay.decide(
        Hash32::new([7; 32]),
        count,
        &resume,
        Some(RequestBinding {
            key: [254; 32],
            request: [253; 32],
        }),
    ));
    assert_eq!(
        result["reply"]["class"], "Accept",
        "restored history did not continue"
    );
    assert_eq!(ok(replay.store().version()), count + 1);
    assert!(replay.audit().is_ok(), "continued history failed audit");
    println!(
        "history_measurement={}",
        json!({"commands":count,"seed_commands":initial,
        "journal_bytes":ok(std::fs::metadata(&journal)).len(),"build_seconds":build_seconds,
        "export_seconds":export_seconds,"restore_seconds":restore_seconds,"reopen_seconds":reopen_seconds,
        "contract":hex(identity(&authority).as_bytes()),"head_receipt":ok(app.journal_entry(count))["receipt"],
        "continued":true,"scope":"full row, entry, scalar and root comparison; no live network"})
    );
    drop(replay);
    drop(app);
    ok(std::fs::remove_dir_all(directory));
}
