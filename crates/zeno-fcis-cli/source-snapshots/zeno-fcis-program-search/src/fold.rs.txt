//! Decision-list folding: `(a && b) ? x : (a ? y : z)` becomes
//! `a ? (b ? x : y) : z`, which drops the `And`.
//!
//! The rewrite is exact by case analysis on `a` and `b`, and it keeps every
//! operand, so it adds or removes no trap by itself. The final canonical form
//! drops dead nodes, which can remove a trap that only a dead node had; the
//! result is therefore checked like every other candidate.

use alloc::vec::Vec;

use crate::ir::{Graph, Node};
use crate::pool::{Pool, canonical};

pub(crate) fn fold_decision_lists(graph: &Graph) -> Graph {
    let mut pool = Pool::new(&graph.inputs);
    let mut ids: Vec<usize> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let renamed = node.map(|operand| ids.get(operand).copied().unwrap_or(usize::MAX));
        let folded = match renamed {
            Node::Select(condition, then, otherwise) => {
                match (pool.term(condition), pool.term(otherwise)) {
                    (Some(Node::And(p, q)), Some(Node::Select(inner, y, z))) => [(p, q), (q, p)]
                        .into_iter()
                        .find(|(a, _)| *a == inner)
                        .map(|(a, b)| {
                            let chosen = pool.add(Node::Select(b, then, y));
                            pool.add(Node::Select(a, chosen, z))
                        }),
                    _ => None,
                }
            }
            _ => None,
        };
        ids.push(folded.unwrap_or_else(|| pool.add(renamed)));
    }
    let roots: Vec<usize> = graph
        .roots
        .iter()
        .map(|root| ids.get(*root).copied().unwrap_or(usize::MAX))
        .collect();
    canonical(&pool.emit(&graph.outputs, &roots))
}
