//! Native adversarial checks for the shared certificate acceptance boundary.
#![forbid(unsafe_code)]
#![allow(dead_code, unexpected_cfgs)]

#[path = "../../crates/zeno-fcis-spec/src/syntax_certificate.rs"]
mod syntax_certificate;

use syntax_certificate::{
    ANY, Decision, Node, Production, Symbol, check_derivation, select_production,
};

fn terminal(tag: u32) -> Symbol {
    Symbol {
        terminal: true,
        tag,
    }
}
fn nonterminal(tag: u32) -> Symbol {
    Symbol {
        terminal: false,
        tag,
    }
}

struct Fixture {
    tags: Vec<u32>,
    productions: Vec<Production>,
    rhs: Vec<Symbol>,
    nodes: Vec<Node>,
    children: Vec<usize>,
    decisions: Vec<Decision>,
}

impl Fixture {
    fn accepted(&self) -> bool {
        check_derivation(
            &self.tags,
            &self.productions,
            &self.rhs,
            &self.nodes,
            &self.children,
            &self.decisions,
            0,
            10,
            64,
        )
    }
}

fn fixture() -> Fixture {
    Fixture {
        tags: vec![1, 2, 3],
        productions: vec![
            Production {
                lhs: 10,
                rhs_start: 0,
                rhs_len: 3,
            },
            Production {
                lhs: 20,
                rhs_start: 3,
                rhs_len: 1,
            },
            Production {
                lhs: 20,
                rhs_start: 4,
                rhs_len: 0,
            },
        ],
        rhs: vec![terminal(1), nonterminal(20), terminal(3), terminal(2)],
        nodes: vec![
            Node {
                symbol: terminal(1),
                production: 0,
                children_start: 0,
                children_len: 0,
                start: 0,
                end: 1,
                first_node: 0,
            },
            Node {
                symbol: terminal(2),
                production: 0,
                children_start: 0,
                children_len: 0,
                start: 1,
                end: 2,
                first_node: 1,
            },
            Node {
                symbol: nonterminal(20),
                production: 1,
                children_start: 0,
                children_len: 1,
                start: 1,
                end: 2,
                first_node: 1,
            },
            Node {
                symbol: terminal(3),
                production: 0,
                children_start: 1,
                children_len: 0,
                start: 2,
                end: 3,
                first_node: 3,
            },
            Node {
                symbol: nonterminal(10),
                production: 0,
                children_start: 1,
                children_len: 3,
                start: 0,
                end: 3,
                first_node: 0,
            },
        ],
        children: vec![1, 0, 2, 3],
        decisions: vec![
            Decision {
                lhs: 10,
                first: 1,
                second: ANY,
                production: 0,
            },
            Decision {
                lhs: 20,
                first: 2,
                second: ANY,
                production: 1,
            },
            Decision {
                lhs: 20,
                first: ANY,
                second: ANY,
                production: 2,
            },
        ],
    }
}

#[test]
fn complete_nested_derivation_is_accepted() {
    assert!(fixture().accepted());
}

#[test]
fn epsilon_and_complete_empty_root_are_accepted() {
    let mut f = fixture();
    f.tags = vec![1, 3];
    f.nodes = vec![
        Node {
            symbol: terminal(1),
            production: 0,
            children_start: 0,
            children_len: 0,
            start: 0,
            end: 1,
            first_node: 0,
        },
        Node {
            symbol: nonterminal(20),
            production: 2,
            children_start: 0,
            children_len: 0,
            start: 1,
            end: 1,
            first_node: 1,
        },
        Node {
            symbol: terminal(3),
            production: 0,
            children_start: 0,
            children_len: 0,
            start: 1,
            end: 2,
            first_node: 2,
        },
        Node {
            symbol: nonterminal(10),
            production: 0,
            children_start: 0,
            children_len: 3,
            start: 0,
            end: 2,
            first_node: 0,
        },
    ];
    f.children = vec![0, 1, 2];
    assert!(f.accepted());
    assert!(check_derivation(
        &[],
        &[Production {
            lhs: 10,
            rhs_start: 0,
            rhs_len: 0
        }],
        &[],
        &[Node {
            symbol: nonterminal(10),
            production: 0,
            children_start: 0,
            children_len: 0,
            start: 0,
            end: 0,
            first_node: 0
        }],
        &[],
        &[Decision {
            lhs: 10,
            first: 0,
            second: 0,
            production: 0
        }],
        0,
        10,
        1
    ));
}

#[test]
fn wrong_terminal_tag_or_source_position_is_refused() {
    let mut f = fixture();
    f.tags[1] = 8;
    assert!(!f.accepted());
    let mut f = fixture();
    f.nodes[1].start = 0;
    assert!(!f.accepted());
}

#[test]
fn complete_input_not_just_valid_prefix_is_required() {
    let mut f = fixture();
    f.tags.push(7);
    assert!(!f.accepted());
    let mut f = fixture();
    f.tags.insert(0, 7);
    assert!(!f.accepted());
}

#[test]
fn declaration_order_and_rhs_symbols_are_semantic() {
    let mut f = fixture();
    f.rhs.swap(0, 2);
    assert!(!f.accepted());
    let mut f = fixture();
    f.productions[1].lhs = 21;
    assert!(!f.accepted());
    let mut f = fixture();
    f.nodes[2].production = 2;
    assert!(!f.accepted());
}

#[test]
fn malformed_grammar_ranges_are_refused_without_overflow() {
    for (start, len) in [(usize::MAX, 0), (1, usize::MAX), (4, 1)] {
        let mut f = fixture();
        f.productions[0].rhs_start = start;
        f.productions[0].rhs_len = len;
        assert!(!f.accepted());
    }
}

#[test]
fn malformed_child_ranges_are_refused_without_overflow() {
    let mut f = fixture();
    f.nodes[2].children_start = usize::MAX;
    assert!(!f.accepted());
    let mut f = fixture();
    f.nodes[2].children_len = usize::MAX;
    assert!(!f.accepted());
}

#[test]
fn cycles_forward_edges_and_out_of_range_edges_are_refused() {
    for target in [2, 4, usize::MAX] {
        let mut f = fixture();
        f.children[0] = target;
        assert!(!f.accepted());
    }
}

#[test]
fn token_gaps_overlaps_and_reordering_are_refused() {
    let mut f = fixture();
    f.nodes[2].start = 2;
    assert!(!f.accepted());
    let mut f = fixture();
    f.nodes[3].start = 1;
    assert!(!f.accepted());
    let mut f = fixture();
    f.children.swap(1, 3);
    assert!(!f.accepted());
}

#[test]
fn duplicate_children_and_subtree_holes_are_refused() {
    let mut f = fixture();
    f.children[2] = 0;
    assert!(!f.accepted());
    let mut f = fixture();
    f.nodes[2].first_node = 0;
    assert!(!f.accepted());
}

#[test]
fn unreachable_node_prefix_and_unused_edges_are_refused() {
    let mut f = fixture();
    // An extra independently valid terminal before the tree cannot be hidden.
    let prefix = f.nodes[0];
    for n in &mut f.nodes {
        n.first_node += 1;
    }
    for i in &mut f.children {
        *i += 1;
    }
    f.nodes.insert(0, prefix);
    assert!(!f.accepted());
    let mut f = fixture();
    f.children.push(0);
    assert!(!f.accepted());
}

#[test]
fn exact_root_tag_and_nonterminal_root_are_required() {
    let mut f = fixture();
    f.nodes[4].symbol.tag = 99;
    assert!(!f.accepted());
    let f = fixture();
    assert!(!check_derivation(
        &[1],
        &[],
        &[],
        &f.nodes[..1],
        &[],
        &[],
        0,
        1,
        1
    ));
}

#[test]
fn budget_is_checked_before_tree_validation() {
    let f = fixture();
    assert!(!check_derivation(
        &f.tags,
        &f.productions,
        &f.rhs,
        &f.nodes,
        &f.children,
        &f.decisions,
        0,
        10,
        4
    ));
    assert!(check_derivation(
        &f.tags,
        &f.productions,
        &f.rhs,
        &f.nodes,
        &f.children,
        &f.decisions,
        0,
        10,
        5
    ));
}

#[test]
fn committed_first_match_cannot_be_replaced_by_later_match() {
    let mut f = fixture();
    f.decisions.insert(
        1,
        Decision {
            lhs: 20,
            first: ANY,
            second: ANY,
            production: 2,
        },
    );
    assert!(!f.accepted());
    assert_eq!(select_production(&f.decisions, 20, 2, 3), Some(2));
}

#[test]
fn two_token_lookahead_and_lhs_are_both_checked() {
    let mut f = fixture();
    f.decisions[0].second = 2;
    assert!(f.accepted());
    f.decisions[0].second = 3;
    assert!(!f.accepted());
    f.decisions[0].second = 2;
    f.decisions[0].lhs = 11;
    assert!(!f.accepted());
}

#[test]
fn no_decision_or_invalid_selected_production_is_refused() {
    let mut f = fixture();
    f.decisions.clear();
    assert!(!f.accepted());
    assert_eq!(select_production(&[], 10, 1, 2), None);
    let mut f = fixture();
    f.decisions[0].production = usize::MAX;
    assert!(!f.accepted());
}

#[test]
fn acceptance_does_not_mutate_any_input() {
    let f = fixture();
    let before = (
        f.tags.clone(),
        f.productions.clone(),
        f.rhs.clone(),
        f.nodes.clone(),
        f.children.clone(),
        f.decisions.clone(),
    );
    assert!(f.accepted());
    assert_eq!(
        before,
        (
            f.tags,
            f.productions,
            f.rhs,
            f.nodes,
            f.children,
            f.decisions
        )
    );
}

#[test]
fn sorted_groups_allow_sparse_and_extreme_nonterminal_tags() {
    let mut f = fixture();
    f.decisions.insert(
        0,
        Decision {
            lhs: 0,
            first: ANY,
            second: ANY,
            production: usize::MAX,
        },
    );
    f.decisions.insert(
        2,
        Decision {
            lhs: 11,
            first: ANY,
            second: ANY,
            production: usize::MAX,
        },
    );
    f.decisions.push(Decision {
        lhs: u32::MAX,
        first: ANY,
        second: ANY,
        production: usize::MAX,
    });
    assert!(f.accepted());
}

#[test]
fn missing_nonterminal_group_cannot_use_a_neighboring_group() {
    let mut f = fixture();
    f.decisions.retain(|row| row.lhs != 20);
    f.decisions.push(Decision {
        lhs: 21,
        first: ANY,
        second: ANY,
        production: 1,
    });
    assert!(!f.accepted());
}

#[test]
fn unsorted_interleaved_groups_preserve_original_first_match_order() {
    let mut f = fixture();
    f.decisions.swap(0, 1);
    assert!(f.accepted());
    f.decisions.rotate_left(1);
    // The wildcard for 20 now precedes its specific row: original order matters.
    assert!(!f.accepted());
}

#[test]
fn unsorted_early_wildcard_cannot_be_skipped_by_binary_search() {
    let mut f = fixture();
    let root = f.decisions[0];
    let specific = f.decisions[1];
    let wildcard = f.decisions[2];
    f.decisions = vec![wildcard, root, specific];
    assert_eq!(select_production(&f.decisions, 20, 2, 3), Some(2));
    assert!(!f.accepted());
}

#[test]
fn ordered_groups_scan_all_prior_patterns_before_selecting() {
    let mut f = fixture();
    let root = f.decisions[0];
    let specific = f.decisions[1];
    let wildcard = f.decisions[2];
    f.decisions = (100..140)
        .map(|tag| Decision {
            lhs: 10,
            first: tag,
            second: ANY,
            production: usize::MAX,
        })
        .collect();
    f.decisions.push(root);
    f.decisions.extend((100..140).map(|tag| Decision {
        lhs: 20,
        first: tag,
        second: ANY,
        production: usize::MAX,
    }));
    f.decisions.push(specific);
    f.decisions.push(wildcard);
    assert!(f.accepted());
    f.decisions[50].first = ANY;
    assert!(!f.accepted());
}
