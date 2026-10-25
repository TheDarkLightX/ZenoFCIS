//! A first-order checker for complete grammar-derivation certificates.
//!
//! All inputs are immutable. The certificate is a postorder tree whose terminal
//! leaves refer to positions in the original token sequence. It contains no
//! copied token payload: consumers must read payloads from those original token
//! positions. This checker establishes syntax derivation, not AST lowering,
//! lexical correctness, grammar unambiguity, or agreement with human intent.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// A terminal token tag or a nonterminal grammar tag.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Symbol {
    /// Whether this symbol selects a source token rather than a production.
    pub terminal: bool,
    /// Token-kind identity for a terminal, or grammar identity for a nonterminal.
    pub tag: u32,
}

/// One production; its ordered right-hand side is a slice of the grammar RHS.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Production {
    /// Nonterminal introduced by this production.
    pub lhs: u32,
    /// First symbol in the shared right-hand-side array.
    pub rhs_start: usize,
    /// Number of ordered right-hand-side symbols.
    pub rhs_len: usize,
}

/// A first-match predictive decision using two token tags of lookahead.
/// `ANY` matches every tag; missing lookahead uses the caller's `eof_tag`.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decision {
    /// Nonterminal for which this decision is applicable.
    pub lhs: u32,
    /// First lookahead tag, or [`ANY`].
    pub first: u32,
    /// Second lookahead tag, or [`ANY`].
    pub second: u32,
    /// Index of the production selected when this is the first matching row.
    pub production: usize,
}

/// Wildcard token tag in a predictive decision pattern.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const ANY: u32 = u32::MAX;

/// A postorder derivation node, covering tokens in `[start, end)`.
///
/// `first_node..=self_index` is the node's complete subtree. Child subtrees must
/// be adjacent in postorder and token order. A terminal has no children and
/// uses production zero. An epsilon production has no children and an empty
/// token range. Child-index slices are adjacent across all node records.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node {
    /// Terminal or nonterminal derived by this node.
    pub symbol: Symbol,
    /// Grammar production index; zero for a terminal leaf.
    pub production: usize,
    /// First edge in the shared ordered child-index array.
    pub children_start: usize,
    /// Number of direct children.
    pub children_len: usize,
    /// First source-token index covered by this derivation.
    pub start: usize,
    /// First source-token index after this derivation.
    pub end: usize,
    /// First postorder node in this node's complete subtree.
    pub first_node: usize,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (left.terminal == right.terminal && left.tag == right.tag),
))]
fn same_symbol(left: Symbol, right: Symbol) -> bool {
    left.terminal == right.terminal && left.tag == right.tag
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == model::decision_matches(row, lhs, first, second),
))]
fn decision_matches(row: Decision, lhs: u32, first: u32, second: u32) -> bool {
    row.lhs == lhs
        && (row.first == ANY || row.first == first)
        && (row.second == ANY || row.second == second)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result.0 == model::lookahead(token_tags@, position as int, eof_tag),
        result.1 == model::lookahead(token_tags@, position as int + 1, eof_tag),
))]
fn lookahead_pair(token_tags: &[u32], position: usize, eof_tag: u32) -> (u32, u32) {
    if position >= token_tags.len() {
        return (eof_tag, eof_tag);
    }
    let first = token_tags[position];
    let second = if token_tags.len() - position > 1 {
        token_tags[position + 1]
    } else {
        eof_tag
    };
    (first, second)
}

/// Selects the first matching decision, with no backtracking or shared state.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(production) => model::selected(decisions@, lhs, first, second, production),
        None => forall|i: int| 0 <= i < decisions.len() ==>
            !model::decision_matches(decisions@[i], lhs, first, second),
    },
))]
pub fn select_production(
    decisions: &[Decision],
    lhs: u32,
    first: u32,
    second: u32,
) -> Option<usize> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= decisions.len(),
            forall|i: int| 0 <= i < index ==>
                !model::decision_matches(decisions@[i], lhs, first, second),
        decreases decisions.len() - index,
    ))]
    while index < decisions.len() {
        let row = decisions[index];
        if decision_matches(row, lhs, first, second) {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(model::selection_at(decisions@, lhs, first, second, row.production, index as int));
            }
            return Some(row.production);
        }
        index += 1;
    }
    None
}

/// Checks monotone grouping once per certificate; malformed ordering is safe
/// because the caller retains the unrestricted first-match fallback.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result ==> model::sorted_decisions(decisions@),
))]
fn decisions_are_sorted(decisions: &[Decision]) -> bool {
    if decisions.len() < 2 {
        return true;
    }
    let mut index = 1usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            1 <= index <= decisions.len(),
            forall|i: int, j: int| 0 <= i <= j < index ==>
                decisions@[i].lhs <= decisions@[j].lhs,
        decreases decisions.len() - index,
    ))]
    while index < decisions.len() {
        if decisions[index - 1].lhs > decisions[index].lhs {
            return false;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|i: int, j: int| 0 <= i <= j <= index implies
                decisions@[i].lhs <= decisions@[j].lhs by {
                if j == index as int && i < index as int {
                    assert(decisions@[i].lhs <= decisions@[index as int - 1].lhs);
                }
            }
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires model::sorted_decisions(decisions@),
    ensures
        result <= decisions.len(),
        forall|i: int| 0 <= i < result ==> decisions@[i].lhs < lhs,
        forall|i: int| result <= i < decisions.len() ==> decisions@[i].lhs >= lhs,
))]
fn first_decision_for(decisions: &[Decision], lhs: u32) -> usize {
    let mut low = 0usize;
    let mut high = decisions.len();
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            low <= high <= decisions.len(),
            model::sorted_decisions(decisions@),
            forall|i: int| 0 <= i < low ==> decisions@[i].lhs < lhs,
            forall|i: int| high <= i < decisions.len() ==> decisions@[i].lhs >= lhs,
        decreases high - low,
    ))]
    while low < high {
        let middle = low + (high - low) / 2;
        if decisions[middle].lhs < lhs {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|i: int| 0 <= i <= middle implies decisions@[i].lhs < lhs by {
                    assert(decisions@[i].lhs <= decisions@[middle as int].lhs);
                }
            }
            low = middle + 1;
        } else {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|i: int| middle <= i < decisions.len() implies decisions@[i].lhs >= lhs by {
                    assert(decisions@[middle as int].lhs <= decisions@[i].lhs);
                }
            }
            high = middle;
        }
    }
    low
}

/// The checked fast path preserves first-match order inside the selected group.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires model::sorted_decisions(decisions@),
    ensures match result {
        Some(production) => model::selected(decisions@, lhs, first, second, production),
        None => forall|i: int| 0 <= i < decisions.len() ==>
            !model::decision_matches(decisions@[i], lhs, first, second),
    },
))]
fn select_production_sorted(
    decisions: &[Decision],
    lhs: u32,
    first: u32,
    second: u32,
) -> Option<usize> {
    let mut index = first_decision_for(decisions, lhs);
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= decisions.len(),
            model::sorted_decisions(decisions@),
            forall|i: int| 0 <= i < index ==>
                !model::decision_matches(decisions@[i], lhs, first, second),
            forall|i: int| index <= i < decisions.len() ==> decisions@[i].lhs >= lhs,
        decreases decisions.len() - index,
    ))]
    while index < decisions.len() {
        let row = decisions[index];
        if row.lhs != lhs {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|i: int| 0 <= i < decisions.len() implies
                    !model::decision_matches(decisions@[i], lhs, first, second) by {
                    if i >= index {
                        assert(decisions@[index as int].lhs <= decisions@[i].lhs);
                    }
                }
            }
            return None;
        }
        if decision_matches(row, lhs, first, second) {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(model::selection_at(decisions@, lhs, first, second, row.production, index as int));
            }
            return Some(row.production);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        index < nodes.len(),
        sorted ==> model::sorted_decisions(decisions@),
    ensures result ==> model::local_derivation(
        token_tags@, productions@, rhs@, nodes@, children@, decisions@, eof_tag, index as int),
))]
#[allow(clippy::too_many_arguments)]
fn check_node(
    token_tags: &[u32],
    productions: &[Production],
    rhs: &[Symbol],
    nodes: &[Node],
    children: &[usize],
    decisions: &[Decision],
    eof_tag: u32,
    index: usize,
    sorted: bool,
) -> bool {
    let node = nodes[index];
    if node.start > node.end || node.end > token_tags.len() || node.first_node > index {
        return false;
    }
    if node.children_start > children.len()
        || node.children_len > children.len() - node.children_start
    {
        return false;
    }
    if node.symbol.terminal {
        return node.production == 0
            && node.children_len == 0
            && node.first_node == index
            && node.start < token_tags.len()
            && node.end - node.start == 1
            && token_tags[node.start] == node.symbol.tag;
    }
    if node.production >= productions.len() {
        return false;
    }
    let (first, second) = lookahead_pair(token_tags, node.start, eof_tag);
    let selected = if sorted {
        select_production_sorted(decisions, node.symbol.tag, first, second)
    } else {
        select_production(decisions, node.symbol.tag, first, second)
    };
    match selected {
        Some(selected) if selected == node.production => {}
        _ => return false,
    }
    let production = productions[node.production];
    if production.lhs != node.symbol.tag
        || production.rhs_start > rhs.len()
        || production.rhs_len > rhs.len() - production.rhs_start
        || production.rhs_len != node.children_len
    {
        return false;
    }
    let mut child_offset = 0usize;
    let mut token_cursor = node.start;
    let mut subtree_cursor = node.first_node;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index < nodes.len(),
            node == nodes@[index as int],
            node.production < productions.len(),
            production == productions@[node.production as int],
            !node.symbol.terminal,
            child_offset <= node.children_len,
            node.children_start + node.children_len <= children.len(),
            production.rhs_start + node.children_len <= rhs.len(),
            token_cursor <= token_tags.len(),
            subtree_cursor <= index,
            child_offset == 0 ==> token_cursor == node.start && subtree_cursor == node.first_node,
            child_offset > 0 ==> token_cursor == nodes@[children@[node.children_start as int + child_offset as int - 1] as int].end,
            child_offset > 0 ==> subtree_cursor as int == children@[node.children_start as int + child_offset as int - 1] as int + 1,
            forall|j: int| 0 <= j < child_offset ==> model::child_link(
                rhs@, nodes@, children@, index as int, j)
                && model::symbol_equal(nodes@[children@[node.children_start as int + j] as int].symbol,
                    rhs@[production.rhs_start as int + j]),
        decreases node.children_len - child_offset,
    ))]
    while child_offset < node.children_len {
        let child_index = children[node.children_start + child_offset];
        if child_index >= index {
            return false;
        }
        let child = nodes[child_index];
        if !same_symbol(child.symbol, rhs[production.rhs_start + child_offset])
            || child.start != token_cursor
            || child.first_node != subtree_cursor
            || child.end > token_tags.len()
        {
            return false;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(model::child_link(rhs@, nodes@, children@, index as int, child_offset as int));
            assert(model::symbol_equal(child.symbol, rhs@[production.rhs_start as int + child_offset as int]));
            assert forall|j: int| 0 <= j <= child_offset implies
                model::child_link(rhs@, nodes@, children@, index as int, j)
                && model::symbol_equal(nodes@[children@[node.children_start as int + j] as int].symbol,
                    rhs@[production.rhs_start as int + j]) by {
                if j < child_offset as int {
                    assert(model::child_link(rhs@, nodes@, children@, index as int, j));
                    assert(model::symbol_equal(nodes@[children@[node.children_start as int + j] as int].symbol,
                        rhs@[production.rhs_start as int + j]));
                } else {
                    assert(j == child_offset as int);
                }
            }
        }
        token_cursor = child.end;
        subtree_cursor = child_index + 1;
        child_offset += 1;
    }
    token_cursor == node.end && subtree_cursor == index
}

/// Checks one complete syntax tree against immutable grammar and source tags.
///
/// The certificate must have at least one node, at most `max_nodes` nodes,
/// fewer child edges than nodes, and no unused node/edge records. Every
/// successful certificate is an acyclic derivation rooted at `root_tag` whose
/// ordered terminal references are exactly the complete input token sequence.
/// Runtime checks decision ordering once. Ordered tables use binary search and
/// scan only one nonterminal's rows; unordered tables retain the full scan.
/// Structural work is linear, with constant invocation-local scratch and no allocation. A `false` result includes a
/// budget refusal; callers that distinguish diagnostics must check the bounds
/// before invoking this Boolean acceptance predicate.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result ==> model::complete_derivation(
        token_tags@, productions@, rhs@, nodes@, children@, decisions@, eof_tag, root_tag, max_nodes),
))]
#[allow(clippy::too_many_arguments)]
pub fn check_derivation(
    token_tags: &[u32],
    productions: &[Production],
    rhs: &[Symbol],
    nodes: &[Node],
    children: &[usize],
    decisions: &[Decision],
    eof_tag: u32,
    root_tag: u32,
    max_nodes: usize,
) -> bool {
    if nodes.is_empty() || nodes.len() > max_nodes || children.len() >= nodes.len() {
        return false;
    }
    let sorted = decisions_are_sorted(decisions);
    let mut index = 0usize;
    let mut child_cursor = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= nodes.len(),
            sorted ==> model::sorted_decisions(decisions@),
            child_cursor <= children.len(),
            index == 0 ==> child_cursor == 0,
            index > 0 ==> child_cursor as int == nodes@[index as int - 1].children_start as int + nodes@[index as int - 1].children_len as int,
            forall|k: int| 0 <= k < index ==> model::local_derivation(
                token_tags@, productions@, rhs@, nodes@, children@, decisions@, eof_tag, k),
            forall|k: int| 0 <= k < index ==> model::edge_slice_follows(nodes@, k),
        decreases nodes.len() - index,
    ))]
    while index < nodes.len() {
        let node = nodes[index];
        if node.children_start != child_cursor
            || !check_node(
                token_tags,
                productions,
                rhs,
                nodes,
                children,
                decisions,
                eof_tag,
                index,
                sorted,
            )
        {
            return false;
        }
        child_cursor = node.children_start + node.children_len;
        index += 1;
    }
    if child_cursor != children.len() {
        return false;
    }
    let root = nodes[nodes.len() - 1];
    if root.symbol.terminal
        || root.symbol.tag != root_tag
        || root.start != 0
        || root.end != token_tags.len()
        || root.first_node != 0
    {
        return false;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        model::prefix_sound(token_tags@, productions@, rhs@, nodes@, children@, decisions@, eof_tag, nodes@.len() as int);
    }
    true
}

#[cfg(verus_keep_ghost)]
pub mod model {
    use super::{ANY, Decision, Node, Production, Symbol};
    use vstd::prelude::*;

    verus! {
    pub open spec fn decision_matches(row: Decision, lhs: u32, first: u32, second: u32) -> bool {
        row.lhs == lhs && (row.first == ANY || row.first == first)
            && (row.second == ANY || row.second == second)
    }

    pub open spec fn sorted_decisions(decisions: Seq<Decision>) -> bool {
        forall|i: int, j: int| 0 <= i <= j < decisions.len() ==>
            decisions[i].lhs <= decisions[j].lhs
    }

    pub open spec fn lookahead(tags: Seq<u32>, position: int, eof_tag: u32) -> u32 {
        if 0 <= position < tags.len() { tags[position] } else { eof_tag }
    }

    pub open spec fn selection_at(decisions: Seq<Decision>, lhs: u32, first: u32,
        second: u32, production: usize, index: int) -> bool
    {
        &&& 0 <= index < decisions.len()
        &&& decisions[index].production == production
        &&& decision_matches(decisions[index], lhs, first, second)
        &&& forall|j: int| 0 <= j < index ==>
            !decision_matches(decisions[j], lhs, first, second)
    }

    pub open spec fn selected(decisions: Seq<Decision>, lhs: u32, first: u32,
        second: u32, production: usize) -> bool
    {
        exists|index: int| selection_at(decisions, lhs, first, second, production, index)
    }

    pub open spec fn symbol_equal(a: Symbol, b: Symbol) -> bool {
        a.terminal == b.terminal && a.tag == b.tag
    }

    pub open spec fn child_link(rhs: Seq<Symbol>, nodes: Seq<Node>, children: Seq<usize>,
        index: int, offset: int) -> bool
    {
        let node = nodes[index];
        let child_index = children[node.children_start as int + offset] as int;
        let child = nodes[child_index];
        &&& 0 <= child_index < index
        &&& child.start == if offset == 0 { node.start }
            else { nodes[children[node.children_start as int + offset - 1] as int].end }
        &&& child.first_node as int == if offset == 0 { node.first_node as int }
            else { children[node.children_start as int + offset - 1] as int + 1 }
    }

    pub open spec fn local_derivation(tags: Seq<u32>, productions: Seq<Production>,
        rhs: Seq<Symbol>, nodes: Seq<Node>, children: Seq<usize>, decisions: Seq<Decision>,
        eof_tag: u32, index: int) -> bool
    {
        &&& 0 <= index < nodes.len()
        &&& {
            let node = nodes[index];
            &&& node.start <= node.end <= tags.len()
            &&& node.first_node <= index
            &&& node.children_start + node.children_len <= children.len()
            &&& if node.symbol.terminal {
                &&& node.production == 0
                &&& node.children_len == 0
                &&& node.first_node == index
                &&& node.start < tags.len()
                &&& node.end == node.start + 1
                &&& tags[node.start as int] == node.symbol.tag
            } else {
                &&& node.production < productions.len()
                &&& selected(decisions, node.symbol.tag,
                    lookahead(tags, node.start as int, eof_tag),
                    lookahead(tags, node.start as int + 1, eof_tag), node.production)
                &&& {
                    let production = productions[node.production as int];
                    &&& production.lhs == node.symbol.tag
                    &&& production.rhs_start + production.rhs_len <= rhs.len()
                    &&& production.rhs_len == node.children_len
                    &&& forall|j: int| 0 <= j < node.children_len ==>
                        child_link(rhs, nodes, children, index, j)
                        && symbol_equal(nodes[children[node.children_start as int + j] as int].symbol,
                            rhs[production.rhs_start as int + j])
                    &&& if node.children_len == 0 {
                        node.start == node.end && node.first_node == index
                    } else {
                        let last = children[node.children_start as int + node.children_len as int - 1] as int;
                        nodes[last].end == node.end && last + 1 == index
                    }
                }
            }
        }
    }

    /// Inductive grammatical meaning: a terminal references exactly its input
    /// token, and a production derives the concatenation of its ordered child
    /// ranges. The decreasing postorder index excludes cycles.
    #[verifier::opaque]
    pub open spec fn derives(tags: Seq<u32>, productions: Seq<Production>,
        rhs: Seq<Symbol>, nodes: Seq<Node>, children: Seq<usize>, decisions: Seq<Decision>,
        eof_tag: u32, index: int) -> bool
        decreases index,
    {
        0 <= index < nodes.len()
        && local_derivation(tags, productions, rhs, nodes, children, decisions, eof_tag, index)
        && (nodes[index].symbol.terminal || forall|j: int| 0 <= j < nodes[index].children_len ==>
            #[trigger] children[nodes[index].children_start as int + j] < index
            &&
            derives(tags, productions, rhs, nodes, children, decisions, eof_tag,
                children[nodes[index].children_start as int + j] as int))
    }

    pub open spec fn edge_slice_follows(nodes: Seq<Node>, index: int) -> bool {
        nodes[index].children_start as int == if index == 0 { 0 }
            else { nodes[index - 1].children_start as int + nodes[index - 1].children_len as int }
    }

    pub open spec fn complete_derivation(tags: Seq<u32>, productions: Seq<Production>,
        rhs: Seq<Symbol>, nodes: Seq<Node>, children: Seq<usize>, decisions: Seq<Decision>,
        eof_tag: u32, root_tag: u32,
        max_nodes: usize) -> bool
    {
        &&& 0 < nodes.len() <= max_nodes
        &&& children.len() < nodes.len()
        &&& !nodes.last().symbol.terminal
        &&& nodes.last().symbol.tag == root_tag
        &&& nodes.last().start == 0
        &&& nodes.last().end == tags.len()
        &&& nodes.last().first_node == 0
        &&& nodes.last().children_start + nodes.last().children_len == children.len()
        &&& forall|k: int| 0 <= k < nodes.len() ==> edge_slice_follows(nodes, k)
        &&& derives(tags, productions, rhs, nodes, children, decisions, eof_tag, nodes.len() as int - 1)
    }

    pub proof fn prefix_sound(tags: Seq<u32>, productions: Seq<Production>, rhs: Seq<Symbol>,
        nodes: Seq<Node>, children: Seq<usize>, decisions: Seq<Decision>, eof_tag: u32, count: int)
        requires
            0 <= count <= nodes.len(),
            forall|k: int| 0 <= k < count ==>
                local_derivation(tags, productions, rhs, nodes, children, decisions, eof_tag, k),
        ensures
            forall|k: int| 0 <= k < count ==>
                derives(tags, productions, rhs, nodes, children, decisions, eof_tag, k),
        decreases count,
    {
        if count > 0 {
            prefix_sound(tags, productions, rhs, nodes, children, decisions, eof_tag, count - 1);
            let index = count - 1;
            assert(local_derivation(tags, productions, rhs, nodes, children, decisions, eof_tag, index));
            reveal_with_fuel(derives, 2);
            if nodes[index].symbol.terminal {
                assert(derives(tags, productions, rhs, nodes, children, decisions, eof_tag, index));
            } else {
                assert forall|j: int| 0 <= j < nodes[index].children_len implies
                    #[trigger] children[nodes[index].children_start as int + j] < index
                    && derives(tags, productions, rhs, nodes, children, decisions, eof_tag,
                        children[nodes[index].children_start as int + j] as int) by {
                    assert(child_link(rhs, nodes, children, index, j));
                    let child = children[nodes[index].children_start as int + j] as int;
                    assert(0 <= child < index);
                    assert(derives(tags, productions, rhs, nodes, children, decisions, eof_tag, child));
                }
                assert(0 <= index < nodes.len());
                assert(local_derivation(tags, productions, rhs, nodes, children, decisions, eof_tag, index));
                assert(forall|j: int| 0 <= j < nodes[index].children_len ==>
                    #[trigger] children[nodes[index].children_start as int + j] < index
                    && derives(tags, productions, rhs, nodes, children, decisions, eof_tag,
                        children[nodes[index].children_start as int + j] as int));
                assert(derives(tags, productions, rhs, nodes, children, decisions, eof_tag, index)
                    == (0 <= index < nodes.len()
                        && local_derivation(tags, productions, rhs, nodes, children, decisions, eof_tag, index)
                        && (nodes[index].symbol.terminal || forall|j: int| 0 <= j < nodes[index].children_len ==>
                            #[trigger] children[nodes[index].children_start as int + j] < index
                            && derives(tags, productions, rhs, nodes, children, decisions, eof_tag,
                                children[nodes[index].children_start as int + j] as int))));
                assert(derives(tags, productions, rhs, nodes, children, decisions, eof_tag, index));
            }
            assert(derives(tags, productions, rhs, nodes, children, decisions, eof_tag, count - 1));
        }
    }
    }
}
