//! Full-width original zUSD decisions through the real durable V2 store.
#[path = "../../zeno-fcis-synthesis/tests/fixtures/g3-original-zusd/fixture.rs"]
mod fixture;
use fixture::*;
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::v2::V2SqliteShell;
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition as c};
use zeno_fcis_value::Value;

#[test]
fn full_width_deposits_reopen_and_replay_without_synthesizing_a_smaller_app() {
    with_authority(|authority, framing| {
        let path =
            std::env::temp_dir().join(format!("zenofcis-original-zusd-{}.db", std::process::id()));
        assert!(!path.exists());
        let mut pre = numbers(framing.state, 110, GENESIS);
        let context = envelope(framing.context, Value::boolean(false));
        let mut store = ok(V2SqliteShell::create(
            &path,
            authority,
            commit(authority.publish_genesis(&pre)),
        ));
        let mut total = 0u128;
        for (index, amount) in [1u128 << 63, 1u128 << 64, 1_000_000_000_000_000_000_000u128]
            .into_iter()
            .enumerate()
        {
            total += amount;
            let command = numbers(framing.command, 150, &[5, amount, 0]);
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let mut state = GENESIS.to_vec();
            state[6] = total;
            let post = numbers(framing.state, 110, &state);
            let publication = commit(authority.publish(raw));
            assert_eq!(publication.poststate(), post);
            assert_eq!(
                publication.effects()[0].payload(),
                effect_payload("collateral_deposited", &[amount])
            );
            let key = Hash32::new([ok(u8::try_from(index + 1)); 32]);
            assert_eq!(
                ok(store.commit(key, publication)).status(),
                CommitStatus::Committed
            );
            assert_eq!(
                ok(store.commit(key, commit(authority.publish(raw)))).status(),
                CommitStatus::IdempotentReplay
            );
            ok(store.audit());
            drop(store);
            store = ok(V2SqliteShell::open(&path, authority));
            assert_eq!(ok(store.snapshot()).state(), post);
            assert_eq!(ok(store.snapshot()).version(), index as u64 + 1);
            pre = post;
        }
        let command = numbers(framing.command, 150, &[5, u128::MAX, 0]);
        assert!(matches!(
            authority.publish(c::Raw {
                state: &pre,
                command: &command,
                context: &context
            }),
            PublicationOutcome::Reject(_)
        ));
        assert_eq!(ok(store.snapshot()).state(), pre);
        ok(store.audit());
        drop(store);
        ok(std::fs::remove_file(&path));
    });
}
