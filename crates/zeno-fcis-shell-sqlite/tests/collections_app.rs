//! Atomic application revision, retry and capacity guards on the collection store.
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_shell_sqlite::collections::{CollectionStore, Error, RequestBinding};
fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}
fn named(keys: &[[u8; 32]]) -> BTreeMap<u32, BTreeSet<[u8; 32]>> {
    BTreeMap::from([(110, keys.iter().copied().collect())])
}
#[test]
fn application_guards_leave_all_publication_parts_unchanged_on_refusal() {
    let mut store = ok(CollectionStore::in_memory());
    ok(store.bind(&[1; 32], b"genesis", &BTreeMap::from([(110, 1)])));
    let binding = RequestBinding {
        key: [2; 32],
        request: [3; 32],
    };
    let receipt = ok(store.commit_request(
        0,
        &named(&[[4; 32]]),
        &[(110, [4; 32], Some(vec![5]))],
        b"checked decision",
        b"post",
        binding,
    ));
    let root = ok(store.root(110));
    assert!(matches!(
        store.commit_request(
            0,
            &named(&[[6; 32]]),
            &[],
            b"stale",
            b"bad",
            RequestBinding {
                key: [7; 32],
                request: [8; 32]
            }
        ),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        store.commit_request(1, &named(&[[4; 32]]), &[], b"retry", b"bad", binding),
        Err(Error::Replay)
    ));
    assert!(matches!(
        store.commit_request(
            1,
            &named(&[[6; 32]]),
            &[(110, [6; 32], Some(vec![9]))],
            b"over capacity",
            b"bad",
            RequestBinding {
                key: [7; 32],
                request: [8; 32]
            }
        ),
        Err(Error::Capacity { map: 110 })
    ));
    assert!(matches!(
        store.commit_request(
            1,
            &named(&[[4; 32]]),
            &[(110, [6; 32], Some(vec![9]))],
            b"unnamed",
            b"bad",
            RequestBinding {
                key: [7; 32],
                request: [8; 32]
            }
        ),
        Err(Error::Framing { .. })
    ));
    assert_eq!(ok(store.version()), 1);
    assert_eq!(ok(store.scalar_state()), b"post");
    assert_eq!(ok(store.root(110)), root);
    assert_eq!(ok(store.get(110, &[6; 32])), None);
    let (request, mut replay) =
        ok(store.request(&binding.key)).unwrap_or_else(|| panic!("request"));
    replay.rehashed = receipt.rehashed;
    assert_eq!(request, binding.request);
    assert_eq!(replay, receipt);
    assert_eq!(ok(store.commits()).len(), 1);
    assert!(ok(store.request(&[7; 32])).is_none());
    // A resource refusal must not retain a changed cardinality counter.
    ok(store.commit_request(
        1,
        &named(&[[4; 32], [6; 32]]),
        &[(110, [4; 32], None), (110, [6; 32], Some(vec![9]))],
        b"replace",
        b"next",
        RequestBinding {
            key: [7; 32],
            request: [8; 32],
        },
    ));
    assert_eq!(ok(store.len(110)), 1);
    // Capacity bounds the atomic result, including insert-before-delete order.
    ok(store.commit_request(
        2,
        &named(&[[4; 32], [6; 32]]),
        &[(110, [4; 32], Some(vec![5])), (110, [6; 32], None)],
        b"replace in reverse order",
        b"last",
        RequestBinding {
            key: [9; 32],
            request: [10; 32],
        },
    ));
    assert_eq!(ok(store.len(110)), 1);
    assert_eq!(ok(store.get(110, &[4; 32])), Some(vec![5]));
    assert_eq!(ok(store.get(110, &[6; 32])), None);
}
#[test]
fn application_binding_cannot_be_replaced_or_adopt_unbound_state() {
    let mut store = ok(CollectionStore::in_memory());
    let limits = BTreeMap::from([(110, 10)]);
    ok(store.bind(&[1; 32], b"genesis", &limits));
    assert!(store.bind(&[2; 32], b"genesis", &limits).is_err());
    assert!(store.bind(&[1; 32], b"different", &limits).is_err());
    assert!(
        store
            .bind(&[1; 32], b"genesis", &BTreeMap::from([(110, 11)]))
            .is_err()
    );
    ok(store.bind(&[1; 32], b"genesis", &limits));
    let mut unbound = ok(CollectionStore::in_memory());
    ok(unbound.commit(
        &named(&[[1; 32]]),
        &[(110, [1; 32], Some(vec![1]))],
        b"data",
        b"state",
    ));
    assert!(unbound.bind(&[1; 32], b"genesis", &limits).is_err());
}
