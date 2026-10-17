//! Hash-consed term pool. `emit` writes a dead-code-free graph in one fixed
//! depth-first order, so equal term structure always gives equal bytes.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Domain;

use crate::ir::{Graph, Kind, Node};

pub(crate) struct Pool {
    inputs: Vec<Domain>,
    terms: Vec<Node>,
    index: BTreeMap<Node, usize>,
}

impl Pool {
    pub(crate) fn new(inputs: &[Domain]) -> Self {
        Self {
            inputs: inputs.to_vec(),
            terms: Vec::new(),
            index: BTreeMap::new(),
        }
    }

    /// Adds a term whose operands are pool ids; commutative operands are
    /// ordered so `a + b` and `b + a` share one term.
    pub(crate) fn add(&mut self, node: Node) -> usize {
        let node = match node {
            Node::Add(a, b) if a > b => Node::Add(b, a),
            Node::Eq(a, b) if a > b => Node::Eq(b, a),
            Node::And(a, b) if a > b => Node::And(b, a),
            other => other,
        };
        if let Some(id) = self.index.get(&node) {
            return *id;
        }
        let id = self.terms.len();
        self.terms.push(node);
        self.index.insert(node, id);
        id
    }

    pub(crate) fn constant(&mut self, kind: Kind, value: i64) -> usize {
        match kind {
            Kind::Bool => self.add(Node::Bool(value != 0)),
            Kind::Int => self.add(Node::Int(value)),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.terms.len()
    }

    pub(crate) fn term(&self, id: usize) -> Option<Node> {
        self.terms.get(id).copied()
    }

    /// Adds every node of `graph`; returns the pool id of each node.
    pub(crate) fn load(&mut self, graph: &Graph) -> Vec<usize> {
        let mut ids: Vec<usize> = Vec::with_capacity(graph.nodes.len());
        for node in &graph.nodes {
            let renamed = node.map(|operand| ids.get(operand).copied().unwrap_or(usize::MAX));
            ids.push(self.add(renamed));
        }
        ids
    }

    /// The graph reachable from `roots`, operands before users, in a fixed
    /// iterative depth-first post-order.
    pub(crate) fn emit(&self, outputs: &[Domain], roots: &[usize]) -> Graph {
        let mut order: Vec<usize> = Vec::new();
        let mut seen = vec![false; self.terms.len()];
        for &root in roots {
            let mut stack = vec![(root, false)];
            while let Some((term, done)) = stack.pop() {
                let Some(node) = self.terms.get(term) else {
                    continue;
                };
                if done {
                    if !seen[term] {
                        seen[term] = true;
                        order.push(term);
                    }
                    continue;
                }
                if seen[term] {
                    continue;
                }
                stack.push((term, true));
                let (operands, count) = node.operands();
                for &operand in operands[..count].iter().rev() {
                    if seen.get(operand) == Some(&false) {
                        stack.push((operand, false));
                    }
                }
            }
        }
        let mut position = vec![usize::MAX; self.terms.len()];
        for (index, term) in order.iter().enumerate() {
            position[*term] = index;
        }
        let at = |term: usize| position.get(term).copied().unwrap_or(usize::MAX);
        Graph {
            inputs: self.inputs.clone(),
            outputs: outputs.to_vec(),
            nodes: order.iter().map(|term| self.terms[*term].map(at)).collect(),
            roots: roots.iter().map(|root| at(*root)).collect(),
        }
    }
}

/// Hash-conses, drops dead nodes and orders deterministically. The result
/// never has more nodes. Under eager evaluation a dropped dead node that could
/// trap changes behaviour, so callers verify before trusting the result.
pub(crate) fn canonical(graph: &Graph) -> Graph {
    let mut pool = Pool::new(&graph.inputs);
    let ids = pool.load(graph);
    let roots: Vec<usize> = graph
        .roots
        .iter()
        .map(|root| ids.get(*root).copied().unwrap_or(usize::MAX))
        .collect();
    pool.emit(&graph.outputs, &roots)
}
