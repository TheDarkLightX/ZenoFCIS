//! Canonical tree roots, complete small-universe orders and owned path plans.
use std::collections::BTreeMap;
use zeno_fcis_codec::collection_merkle::{self as m, Digest, Key, Subtree};

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}
fn keys() -> [Key; 8] {
    std::array::from_fn(|i| {
        let mut key = [0x5a; 32];
        key[31] = 0x50 + u8::try_from(i).unwrap_or(0);
        key
    })
}
fn bit(key: &Key, depth: usize) -> u8 {
    (key[depth / 8] >> (7 - depth % 8)) & 1
}
fn prefix(key: &Key, depth: usize) -> Key {
    let mut out = [0; 32];
    for index in 0..depth.min(256) {
        if bit(key, index) == 1 {
            out[index / 8] |= 1 << (7 - index % 8);
        }
    }
    out
}
#[derive(Clone)]
struct Reference {
    root: Digest,
    nodes: BTreeMap<(usize, Key), Digest>,
}
fn reference(
    mask: u16,
    depth: usize,
    keys: &[Key; 8],
    nodes: &mut BTreeMap<(usize, Key), Digest>,
) -> Digest {
    if mask == 0 {
        return m::empty_hash();
    }
    if mask.count_ones() == 1 {
        let index = usize::try_from(mask.trailing_zeros()).unwrap_or(0);
        return m::leaf_hash(&keys[index], &[u8::try_from(index).unwrap_or(0)]);
    }
    let left = (0..8)
        .filter(|i| mask & (1 << i) != 0 && bit(&keys[*i], depth) == 0)
        .fold(0, |m, i| m | (1 << i));
    let digest = m::node_hash(
        &reference(left, depth + 1, keys, nodes),
        &reference(mask ^ left, depth + 1, keys, nodes),
    );
    let index = usize::try_from(mask.trailing_zeros()).unwrap_or(0);
    nodes.insert((depth, prefix(&keys[index], depth)), digest);
    digest
}
fn references(keys: &[Key; 8]) -> Vec<Reference> {
    (0..256u16)
        .map(|mask| {
            let mut nodes = BTreeMap::new();
            let root = reference(mask, 0, keys, &mut nodes);
            Reference { root, nodes }
        })
        .collect()
}
#[derive(Clone, Default)]
struct Tree {
    mask: u16,
    nodes: BTreeMap<(usize, Key), Digest>,
}
impl Tree {
    fn summary(
        &self,
        depth: usize,
        at: &Key,
        prefixes: &[[Key; 257]; 8],
        leaves: &[Digest; 8],
    ) -> Subtree {
        let members: Vec<_> = (0..8)
            .filter(|i| self.mask & (1 << i) != 0 && prefixes[*i][depth] == *at)
            .collect();
        match members.as_slice() {
            [] => Subtree::Empty,
            [index] => Subtree::Leaf(leaves[*index]),
            _ => Subtree::Branch(
                *self
                    .nodes
                    .get(&(depth, *at))
                    .unwrap_or_else(|| panic!("missing reference-cache node")),
            ),
        }
    }
    fn apply(
        &mut self,
        index: usize,
        present: bool,
        keys: &[Key; 8],
        leaves: &[Digest; 8],
        prefixes: &[[Key; 257]; 8],
        expected: &[Reference],
    ) {
        let before = (0..index).rev().find(|i| self.mask & (1 << i) != 0);
        let after = (index + 1..8).find(|i| self.mask & (1 << i) != 0);
        let neighbours: Vec<_> = [before, after]
            .into_iter()
            .flatten()
            .map(|i| keys[i])
            .collect();
        let length = m::shared_path_length(&keys[index], &neighbours);
        let independent = (0..8)
            .filter(|i| *i != index && self.mask & (1 << i) != 0)
            .map(|i| {
                (0..256)
                    .take_while(|d| bit(&keys[index], *d) == bit(&keys[i], *d))
                    .count()
                    + 1
            })
            .max()
            .unwrap_or(0);
        assert_eq!(
            length, independent,
            "indexed neighbours missed the deepest shared prefix"
        );
        let siblings: Vec<_> = (0..length)
            .map(|depth| {
                self.summary(
                    depth + 1,
                    &m::sibling_prefix(&keys[index], depth)
                        .unwrap_or_else(|| panic!("valid key depth")),
                    prefixes,
                    leaves,
                )
            })
            .collect();
        let record = [u8::try_from(index).unwrap_or(0)];
        let saved = siblings.clone();
        let plan = m::update_path(
            &keys[index],
            present.then_some(record.as_slice()),
            &siblings,
        )
        .unwrap_or_else(|| panic!("valid bounded path"));
        assert_eq!(siblings, saved, "path input was changed");
        self.mask = if present {
            self.mask | (1 << index)
        } else {
            self.mask & !(1 << index)
        };
        let oracle = &expected[usize::from(self.mask)];
        assert_eq!(
            plan.root, oracle.root,
            "incremental root differs from independent rebuild"
        );
        assert_eq!(plan.nodes.len(), length, "path work changed");
        for node in plan.nodes {
            assert_eq!(node.prefix, prefixes[index][node.depth]);
            if let Some(hash) = node.hash {
                self.nodes.insert((node.depth, node.prefix), hash);
            } else {
                self.nodes.remove(&(node.depth, node.prefix));
            }
        }
        assert_eq!(self.nodes, oracle.nodes, "cache depends on update history");
        let stored = self.summary(0, &[0; 32], prefixes, leaves).digest();
        assert_eq!(stored, oracle.root, "stored root differs from rebuilt root");
    }
}
fn permutations(items: &mut [usize], at: usize, visit: &mut impl FnMut(&[usize])) {
    if at == items.len() {
        visit(items);
        return;
    }
    for index in at..items.len() {
        items.swap(at, index);
        permutations(items, at + 1, visit);
        items.swap(at, index);
    }
}
#[test]
fn every_subset_root_matches_independent_sha256_vectors() {
    let keys = keys();
    let expected = references(&keys);
    let mut checked = 0;
    for line in include_str!("data/collection-roots.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let (mask, digest) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("reference vector"));
        let mask: usize = ok(mask.parse());
        let actual: String = expected[mask]
            .root
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(actual, digest, "legacy canonical hash vector {mask}");
        let entries: Vec<_> = (0..8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| (keys[i], vec![u8::try_from(i).unwrap_or(0)]))
            .collect();
        assert_eq!(m::rebuild_root(&entries), expected[mask].root);
        checked += 1;
    }
    assert_eq!(checked, 256);
}
#[test]
fn all_long_prefix_subsets_and_insert_remove_orders_are_canonical() {
    let keys = keys();
    let expected = references(&keys);
    let leaves = std::array::from_fn(|i| m::leaf_hash(&keys[i], &[u8::try_from(i).unwrap_or(0)]));
    let prefixes = std::array::from_fn(|i| std::array::from_fn(|depth| prefix(&keys[i], depth)));
    let mut inserted = 0;
    let mut removed = 0;
    for mask in 0..256u16 {
        let members: Vec<usize> = (0..8).filter(|i| mask & (1 << i) != 0).collect();
        let mut tree = Tree::default();
        for index in &members {
            tree.apply(*index, true, &keys, &leaves, &prefixes, &expected);
        }
        for index in members.iter().rev() {
            tree.apply(*index, false, &keys, &leaves, &prefixes, &expected);
        }
        if members.len() > 5 {
            continue;
        }
        permutations(&mut members.clone(), 0, &mut |order| {
            let mut tree = Tree::default();
            for index in order {
                tree.apply(*index, true, &keys, &leaves, &prefixes, &expected);
            }
            inserted += 1;
        });
        permutations(&mut members.clone(), 0, &mut |order| {
            let mut tree = Tree {
                mask,
                nodes: expected[usize::from(mask)].nodes.clone(),
            };
            for index in order {
                tree.apply(*index, false, &keys, &leaves, &prefixes, &expected);
            }
            removed += 1;
        });
    }
    assert_eq!((inserted, removed), (8801, 8801));
    println!(
        "collection_merkle_corpus subsets=256 insertion_orders=8801 removal_orders=8801 shared_prefix_bits=253"
    );
}
// Fixed small interpretation profile. Native CI retains both complete corpora.
#[test]
fn long_prefix_miri_profile_matches_reference_and_owned_paths() {
    let keys = keys();
    let orders = [[0usize, 1, 7], [7usize, 1, 0]];
    let mut visited = vec![0u16];
    for order in orders {
        let mut mask = 0;
        for index in order {
            mask |= 1 << index;
            visited.push(mask);
        }
        for index in order {
            mask &= !(1 << index);
            visited.push(mask);
        }
    }
    visited.sort_unstable();
    visited.dedup();
    let expected: Vec<_> = (0..256u16)
        .map(|mask| {
            let mut nodes = BTreeMap::new();
            let root = if visited.contains(&mask) {
                reference(mask, 0, &keys, &mut nodes)
            } else {
                m::empty_hash()
            };
            Reference { root, nodes }
        })
        .collect();
    let leaves = std::array::from_fn(|i| m::leaf_hash(&keys[i], &[i as u8]));
    let prefixes = std::array::from_fn(|i| std::array::from_fn(|depth| prefix(&keys[i], depth)));
    let mut updates = 0;
    for order in orders {
        let mut tree = Tree::default();
        for index in order {
            tree.apply(index, true, &keys, &leaves, &prefixes, &expected);
            updates += 1;
        }
        for index in order {
            tree.apply(index, false, &keys, &leaves, &prefixes, &expected);
            updates += 1;
        }
        assert_eq!(tree.mask, 0);
        assert!(tree.nodes.is_empty());
    }
    assert_eq!(updates, 12);
    assert!(visited.contains(&131));
}

#[test]
fn prefix_bounds_and_path_limit_are_exact() {
    for key in keys().into_iter().chain([[0; 32], [255; 32]]) {
        for depth in 0..=256 {
            assert_eq!(m::prefix(&key, depth), prefix(&key, depth));
            let (low, high) = m::bounds(&key, depth);
            assert_eq!(low, prefix(&key, depth));
            for bit_index in 0..256 {
                assert_eq!(
                    bit(&high, bit_index),
                    if bit_index < depth {
                        bit(&key, bit_index)
                    } else {
                        1
                    }
                );
            }
        }
    }
    let key = [7; 32];
    let input = vec![Subtree::Empty; 257];
    let saved = input.clone();
    assert!(m::update_path(&key, Some(&[1]), &input).is_none());
    assert_eq!(input, saved);
    assert_eq!(m::sibling_prefix(&key, 256), None);
    assert_eq!(m::prefix(&key, usize::MAX), key);
    assert_eq!(m::bounds(&key, usize::MAX), (key, key));
}
