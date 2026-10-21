//! Internal method tests. Only this cfg(test) descendant can assemble an Authority
//! directly. The fixture identity is explicitly non-authorizing; production
//! catalog/source constructor qualification remains a separate required test.
use super::super::super::{Resource, composition as c, laws};
use super::super::{
    Refusal,
    tests::{
        generous,
        sealing::{fixture, framing, originals, raw, transition_oracle},
    },
};
use super::Authority;

#[test]
fn methods_recompute_before_exact_replay_for_every_class_and_changed_byte() {
    fixture(generous(), true, |d| {
        let authority=Authority{core:c::bind(d).unwrap_or_else(|error| panic!("methods_recompute_before_exact_replay_for_every_class_and_changed_byte fixture failed: {error:?}")),framing:framing(),identity:b"internal-method-fixture".to_vec(),channel_roots:&[(3,200,201)]};
        assert!(core::ptr::eq(authority.descriptor(), d));
        for code in 0..=2 {
            let values = originals(code, true);
            let actual = authority.evaluate(raw(&values));
            let persisted=actual.subject().unwrap_or_else(|error| panic!("methods_recompute_before_exact_replay_for_every_class_and_changed_byte fixture failed: {error:?}")).to_vec();
            assert_eq!(
                persisted,
                transition_oracle(b"internal-method-fixture", &values, code as u128)
            );
            let replay = authority.replay(raw(&values), &persisted);
            assert_eq!(replay.result().unwrap_or_else(|error| panic!("methods_recompute_before_exact_replay_for_every_class_and_changed_byte fixture failed: {error:?}")).class(),actual.result().unwrap_or_else(|error| panic!("methods_recompute_before_exact_replay_for_every_class_and_changed_byte fixture failed: {error:?}")).class());
            assert_eq!(replay.subject().unwrap_or_else(|error| panic!("methods_recompute_before_exact_replay_for_every_class_and_changed_byte fixture failed: {error:?}")),persisted);
            assert_eq!(replay.usage(), actual.usage());
            assert_eq!(replay.reads(), actual.reads());
            assert_eq!(replay.decision_attempts(), actual.decision_attempts());
            assert_eq!(replay.diagnostics(), actual.diagnostics());
            assert_eq!(replay.law_reads(), actual.law_reads());
            for index in 0..persisted.len() {
                let mut changed = persisted.clone();
                changed[index] ^= 1;
                let refused = authority.replay(raw(&values), &changed);
                assert_eq!(refused.result().err().unwrap_or_else(|| panic!("expected refusal in methods_recompute_before_exact_replay_for_every_class_and_changed_byte")),Refusal::ReplayMismatch,"byte {index}");
                assert_eq!(refused.subject(), Err(Refusal::ReplayMismatch));
                assert_eq!(refused.usage(), actual.usage());
                assert_eq!(refused.diagnostics(), actual.diagnostics());
            }
            for changed in [&[][..], &persisted[..persisted.len() - 1]] {
                let refused = authority.replay(raw(&values), changed);
                assert_eq!(refused.result().err().unwrap_or_else(|| panic!("expected refusal in methods_recompute_before_exact_replay_for_every_class_and_changed_byte")),Refusal::ReplayMismatch);
                assert_eq!(refused.usage(), actual.usage());
            }
            let state = originals(code, false);
            let command = originals((code + 1) % 3, true);
            for changed in [raw(&state), raw(&command)] {
                assert_eq!(authority.replay(changed,&persisted).result().err().unwrap_or_else(|| panic!("expected refusal in methods_recompute_before_exact_replay_for_every_class_and_changed_byte")),Refusal::ReplayMismatch);
            }
            let mut malformed = originals(code, true);
            malformed.2[12] ^= 1;
            let refused = authority.replay(raw(&malformed), &persisted);
            assert!(matches!(
                refused.result(),
                Err(Refusal::Core(c::Failure::Frame(
                    2,
                    c::FrameFailure::Envelope(_)
                )))
            ));
            assert_eq!(refused.usage().used(Resource::Byte), 144);
            assert!(refused.reads().is_empty());
        }
    });
}

#[test]
fn genesis_methods_recompute_initial_laws_and_keep_phase_distinct() {
    fixture(generous(), true, |d| {
        let authority=Authority{core:c::bind(d).unwrap_or_else(|error| panic!("genesis_methods_recompute_initial_laws_and_keep_phase_distinct fixture failed: {error:?}")),framing:framing(),identity:b"internal-method-fixture".to_vec(),channel_roots:&[(3,200,201)]};
        let values = originals(0, true);
        let actual = authority.genesis(&values.0);
        let persisted=actual.subject().unwrap_or_else(|error| panic!("genesis_methods_recompute_initial_laws_and_keep_phase_distinct fixture failed: {error:?}")).to_vec();
        let replay = authority.replay_genesis(&values.0, &persisted);
        assert!(matches!(replay.result(),Ok(c) if c.class()==c::Class::Accept));
        assert_eq!(replay.subject().unwrap_or_else(|error| panic!("genesis_methods_recompute_initial_laws_and_keep_phase_distinct fixture failed: {error:?}")),persisted);
        assert_eq!(replay.raw().state, values.0);
        assert_eq!(replay.ingress_usage(), actual.ingress_usage());
        assert_eq!(replay.usage(), actual.usage());
        assert_eq!(replay.reads(), actual.reads());
        assert_eq!(replay.diagnostics(), actual.diagnostics());
        assert_eq!(replay.law_reads(), actual.law_reads());
        let transition = authority.evaluate(raw(&values));
        assert!(matches!(authority.replay_genesis(&values.0,transition.subject().unwrap_or_else(|error| panic!("genesis_methods_recompute_initial_laws_and_keep_phase_distinct fixture failed: {error:?}"))).result(),Err(Refusal::ReplayMismatch)));
        assert_eq!(authority.replay(raw(&values),&persisted).result().err().unwrap_or_else(|| panic!("expected refusal in genesis_methods_recompute_initial_laws_and_keep_phase_distinct")),Refusal::ReplayMismatch);
        for index in 0..persisted.len() {
            let mut changed = persisted.clone();
            changed[index] ^= 1;
            let refused = authority.replay_genesis(&values.0, &changed);
            assert!(matches!(refused.result(), Err(Refusal::ReplayMismatch)));
            assert_eq!(refused.usage(), actual.usage());
            assert_eq!(refused.diagnostics(), actual.diagnostics());
        }
        let bad = originals(0, false);
        let refused = authority.replay_genesis(&bad.0, &persisted);
        assert!(matches!(
            refused.result(),
            Err(Refusal::Core(c::Failure::Law(laws::Failure::Violated)))
        ));
        assert!(refused.subject().is_err());
        assert_eq!(refused.usage().used(Resource::Byte), 56);
        assert_eq!(refused.usage().used(Resource::Read), 2);
        assert_eq!(refused.usage().used(Resource::Step), 2);
        assert_eq!(refused.diagnostics().last().unwrap_or_else(|| panic!("missing fixture value in genesis_methods_recompute_initial_laws_and_keep_phase_distinct")).id,50);
    });
}
