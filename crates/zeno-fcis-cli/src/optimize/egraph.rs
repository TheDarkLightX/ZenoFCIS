//! The e-graph: a union-find over classes, hash-consed e-nodes, congruence
//! rebuilding and the per-class annotations of [`super::semantics`].
//!
//! Merges are guarded. With exact signatures two classes merge only when their
//! signatures are identical, poison included; otherwise only unpoisoned classes
//! merge by rule. Congruence and commutativity merges are structural: the two
//! sides are the same computation, so they always agree. A guard refusal is
//! counted, never silently accepted, and a structural merge that finds
//! disagreeing annotations stops the whole optimization as inconsistent.

use std::collections::BTreeMap;

use zeno_fcis_synthesis::finite::{Op, Program};

use super::semantics::{Analysis, ClassData, DomainInfo, Shape, TypeError, analyze};

pub(crate) type ClassId = u32;
pub(crate) type NodeId = u32;

/// One instruction over e-classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ENode {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(ClassId, ClassId),
    Sub(ClassId, ClassId),
    Eq(ClassId, ClassId),
    Lt(ClassId, ClassId),
    And(ClassId, ClassId),
    Not(ClassId),
    Select(ClassId, ClassId, ClassId),
}

impl ENode {
    /// Operand classes in instruction order.
    pub(crate) fn children(self) -> Vec<ClassId> {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => Vec::new(),
            Self::Not(a) => vec![a],
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::And(a, b) => {
                vec![a, b]
            }
            Self::Select(c, a, b) => vec![c, a, b],
        }
    }

    fn map(self, mut f: impl FnMut(ClassId) -> ClassId) -> Self {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => self,
            Self::Not(a) => Self::Not(f(a)),
            Self::Add(a, b) => Self::Add(f(a), f(b)),
            Self::Sub(a, b) => Self::Sub(f(a), f(b)),
            Self::Eq(a, b) => Self::Eq(f(a), f(b)),
            Self::Lt(a, b) => Self::Lt(f(a), f(b)),
            Self::And(a, b) => Self::And(f(a), f(b)),
            Self::Select(c, a, b) => Self::Select(f(c), f(a), f(b)),
        }
    }

    /// The library instruction, given each operand class's node index.
    pub(crate) fn op(self, mut index: impl FnMut(ClassId) -> u16) -> Op {
        match self {
            Self::Input(a) => Op::Input(a),
            Self::Int(a) => Op::Int(a),
            Self::Bool(a) => Op::Bool(a),
            Self::Add(a, b) => Op::Add(index(a), index(b)),
            Self::Sub(a, b) => Op::Sub(index(a), index(b)),
            Self::Eq(a, b) => Op::Eq(index(a), index(b)),
            Self::Lt(a, b) => Op::Lt(index(a), index(b)),
            Self::And(a, b) => Op::And(index(a), index(b)),
            Self::Not(a) => Op::Not(index(a)),
            Self::Select(c, a, b) => Op::Select(index(c), index(a), index(b)),
        }
    }

    /// Canonical byte length of this instruction once encoded.
    pub(crate) fn bytes(self) -> u64 {
        super::semantics::op_bytes(&self.op(|_| 0))
    }

    /// The library instruction as an e-node whose operands are the classes
    /// of the referenced nodes.
    pub(crate) fn from_op(op: &Op, class: impl Fn(u16) -> Option<ClassId>) -> Option<Self> {
        Some(match *op {
            Op::Input(a) => Self::Input(a),
            Op::Int(a) => Self::Int(a),
            Op::Bool(a) => Self::Bool(a),
            Op::Add(a, b) => Self::Add(class(a)?, class(b)?),
            Op::Sub(a, b) => Self::Sub(class(a)?, class(b)?),
            Op::Eq(a, b) => Self::Eq(class(a)?, class(b)?),
            Op::Lt(a, b) => Self::Lt(class(a)?, class(b)?),
            Op::And(a, b) => Self::And(class(a)?, class(b)?),
            Op::Not(a) => Self::Not(class(a)?),
            Op::Select(c, a, b) => Self::Select(class(c)?, class(a)?, class(b)?),
            _ => return None,
        })
    }
}

#[derive(Debug)]
struct NodeEntry {
    node: ENode,
    class: ClassId,
    alive: bool,
    /// Whether this `Add` or `Sub` may overflow on a tuple whose operands
    /// are defined. Such a node is pinned: every candidate keeps it.
    may_trap: bool,
    /// Index of the earliest original instruction this node came from.
    origin: Option<u16>,
}

#[derive(Debug)]
struct ClassEntry {
    parent: ClassId,
    nodes: Vec<NodeId>,
    parents: Vec<NodeId>,
    data: ClassData,
}

/// Size caps that `add` enforces before creating anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Caps {
    pub(crate) max_enodes: u32,
    pub(crate) max_classes: u32,
}

/// Why an e-node was not added.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddError {
    Type(TypeError),
    /// A cap would be exceeded; names the cap.
    Limit(Limit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Limit {
    ENodes,
    Classes,
    Rewrites,
}

impl Limit {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ENodes => "max_enodes",
            Self::Classes => "max_classes",
            Self::Rewrites => "max_rewrites_per_round",
        }
    }
}

/// Which argument justifies a merge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MergeReason {
    /// An algebraic identity; subject to the compatibility guard.
    Rule,
    /// Congruence or commutativity: the same computation twice.
    Structural,
}

/// The e-graph found disagreeing annotations where the two sides are the same
/// computation; this is an internal defect and the optimization stops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Inconsistency;

/// What a merge did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Merge {
    Merged,
    AlreadyEqual,
    Refused,
}

#[derive(Debug)]
pub(crate) struct EGraph {
    domain: DomainInfo,
    nodes: Vec<NodeEntry>,
    classes: Vec<ClassEntry>,
    memo: BTreeMap<ENode, NodeId>,
    dirty: Vec<ClassId>,
    live_classes: u32,
    merges: u64,
    refused: u64,
}

impl EGraph {
    pub(crate) fn new(domain: DomainInfo) -> Self {
        Self {
            domain,
            nodes: Vec::new(),
            classes: Vec::new(),
            memo: BTreeMap::new(),
            dirty: Vec::new(),
            live_classes: 0,
            merges: 0,
            refused: 0,
        }
    }

    /// Builds the e-graph of an admitted program, hash-consing identical
    /// instructions, and returns the classes of its roots in order.
    pub(crate) fn from_program(
        program: &Program,
        domain: DomainInfo,
        caps: Caps,
    ) -> Result<(Self, Vec<ClassId>), AddError> {
        let mut egraph = Self::new(domain);
        let mut classes: Vec<ClassId> = Vec::with_capacity(program.nodes().len());
        for (index, op) in program.nodes().iter().enumerate() {
            let node = ENode::from_op(op, |id| classes.get(usize::from(id)).copied())
                .ok_or(AddError::Type(TypeError::Mismatch))?;
            let origin = u16::try_from(index).ok();
            let class = egraph.add_with_origin(node, caps, origin)?;
            classes.push(class);
        }
        let roots = program
            .roots()
            .iter()
            .map(|root| classes.get(usize::from(*root)).copied())
            .collect::<Option<Vec<_>>>()
            .ok_or(AddError::Type(TypeError::Mismatch))?;
        Ok((egraph, roots))
    }

    pub(crate) fn domain(&self) -> &DomainInfo {
        &self.domain
    }

    /// Representative of `class`.
    pub(crate) fn find(&self, mut class: ClassId) -> ClassId {
        loop {
            let parent = self.classes[class as usize].parent;
            if parent == class {
                return class;
            }
            class = parent;
        }
    }

    pub(crate) fn canonical(&self, node: ENode) -> ENode {
        node.map(|class| self.find(class))
    }

    /// The class holding `node`, if it exists.
    pub(crate) fn lookup(&self, node: ENode) -> Option<ClassId> {
        let id = *self.memo.get(&self.canonical(node))?;
        Some(self.find(self.nodes[id as usize].class))
    }

    /// Adds an e-node, returning its class. An existing e-node is shared.
    pub(crate) fn add(&mut self, node: ENode, caps: Caps) -> Result<ClassId, AddError> {
        self.add_with_origin(node, caps, None)
    }

    fn add_with_origin(
        &mut self,
        node: ENode,
        caps: Caps,
        origin: Option<u16>,
    ) -> Result<ClassId, AddError> {
        let node = self.canonical(node);
        if let Some(&id) = self.memo.get(&node) {
            let entry = &mut self.nodes[id as usize];
            if entry.origin.is_none() {
                entry.origin = origin;
            }
            let class = entry.class;
            return Ok(self.find(class));
        }
        let analysis = self.analyze(node)?;
        if self.nodes.len() >= caps.max_enodes as usize {
            return Err(AddError::Limit(Limit::ENodes));
        }
        if self.live_classes >= caps.max_classes {
            return Err(AddError::Limit(Limit::Classes));
        }
        let id = u32::try_from(self.nodes.len()).map_err(|_| AddError::Limit(Limit::ENodes))?;
        let class =
            u32::try_from(self.classes.len()).map_err(|_| AddError::Limit(Limit::Classes))?;
        self.nodes.push(NodeEntry {
            node,
            class,
            alive: true,
            may_trap: analysis.may_trap,
            origin,
        });
        self.classes.push(ClassEntry {
            parent: class,
            nodes: vec![id],
            parents: Vec::new(),
            data: analysis.data,
        });
        self.live_classes += 1;
        for child in node.children() {
            self.classes[child as usize].parents.push(id);
        }
        self.memo.insert(node, id);
        Ok(class)
    }

    fn analyze(&self, node: ENode) -> Result<Analysis, AddError> {
        let data = |class: ClassId| &self.classes[self.find(class) as usize].data;
        let shape = match node {
            ENode::Input(a) => Shape::Input(a),
            ENode::Int(a) => Shape::Int(a),
            ENode::Bool(a) => Shape::Bool(a),
            ENode::Add(a, b) => Shape::Add(data(a), data(b)),
            ENode::Sub(a, b) => Shape::Sub(data(a), data(b)),
            ENode::Eq(a, b) => Shape::Eq(data(a), data(b)),
            ENode::Lt(a, b) => Shape::Lt(data(a), data(b)),
            ENode::And(a, b) => Shape::And(data(a), data(b)),
            ENode::Not(a) => Shape::Not(data(a)),
            ENode::Select(c, a, b) => Shape::Select(data(c), data(a), data(b)),
        };
        analyze(&self.domain, shape).map_err(AddError::Type)
    }

    /// Merges two classes if the guard admits it. The lower class id becomes
    /// the representative.
    pub(crate) fn union(
        &mut self,
        a: ClassId,
        b: ClassId,
        reason: MergeReason,
    ) -> Result<Merge, Inconsistency> {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return Ok(Merge::AlreadyEqual);
        }
        let (root, child) = if a < b { (a, b) } else { (b, a) };
        let compatible = self.classes[root as usize]
            .data
            .compatible(&self.classes[child as usize].data);
        let merged = match reason {
            MergeReason::Rule if !compatible => {
                self.refused += 1;
                return Ok(Merge::Refused);
            }
            MergeReason::Structural
                if self.classes[root as usize].data.kind
                    != self.classes[child as usize].data.kind
                    || (self.domain.exact() && !compatible) =>
            {
                return Err(Inconsistency);
            }
            _ => self.classes[root as usize]
                .data
                .merged(&self.classes[child as usize].data)
                .ok_or(Inconsistency)?,
        };
        self.classes[child as usize].parent = root;
        let nodes = std::mem::take(&mut self.classes[child as usize].nodes);
        for &id in &nodes {
            self.nodes[id as usize].class = root;
        }
        let parents = std::mem::take(&mut self.classes[child as usize].parents);
        let entry = &mut self.classes[root as usize];
        entry.nodes.extend(nodes);
        entry.nodes.sort_unstable();
        entry.nodes.dedup();
        entry.parents.extend(parents);
        entry.data = merged;
        self.live_classes -= 1;
        self.merges += 1;
        self.dirty.push(root);
        Ok(Merge::Merged)
    }

    /// Restores the hash-consing and congruence invariants after unions:
    /// every parent of a merged class is re-canonicalized, and two parents
    /// that become the same e-node merge their classes in turn.
    pub(crate) fn rebuild(&mut self) -> Result<(), Inconsistency> {
        while let Some(class) = self.dirty.pop() {
            let class = self.find(class);
            let mut parents = std::mem::take(&mut self.classes[class as usize].parents);
            parents.sort_unstable();
            parents.dedup();
            for &parent in &parents {
                self.repair(parent)?;
            }
            let root = self.find(class);
            let entry = &mut self.classes[root as usize];
            entry.parents.extend(
                parents
                    .into_iter()
                    .filter(|id| self.nodes[*id as usize].alive),
            );
            entry.parents.sort_unstable();
            entry.parents.dedup();
        }
        Ok(())
    }

    fn repair(&mut self, id: NodeId) -> Result<(), Inconsistency> {
        if !self.nodes[id as usize].alive {
            return Ok(());
        }
        let old = self.nodes[id as usize].node;
        let new = self.canonical(old);
        if self.memo.get(&old) == Some(&id) {
            self.memo.remove(&old);
        }
        match self.memo.get(&new).copied() {
            Some(other) if other != id && self.nodes[other as usize].alive => {
                let (a, b) = (
                    self.nodes[other as usize].class,
                    self.nodes[id as usize].class,
                );
                self.union(a, b, MergeReason::Structural)?;
                let (keep, kill) = if other < id { (other, id) } else { (id, other) };
                self.kill(kill, keep);
                self.nodes[keep as usize].node = new;
                self.memo.insert(new, keep);
            }
            _ => {
                self.nodes[id as usize].node = new;
                self.memo.insert(new, id);
            }
        }
        Ok(())
    }

    /// Retires a duplicate e-node, transferring its pin and origin.
    fn kill(&mut self, kill: NodeId, keep: NodeId) {
        let may_trap = self.nodes[kill as usize].may_trap;
        let origin = self.nodes[kill as usize].origin;
        self.nodes[kill as usize].alive = false;
        let keeper = &mut self.nodes[keep as usize];
        keeper.may_trap |= may_trap;
        keeper.origin = match (keeper.origin, origin) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let class = self.find(self.nodes[kill as usize].class);
        self.classes[class as usize].nodes.retain(|id| *id != kill);
    }

    /// Representatives of every live class, ascending.
    pub(crate) fn classes(&self) -> Vec<ClassId> {
        (0..self.classes.len())
            .map(|class| class as ClassId)
            .filter(|class| self.classes[*class as usize].parent == *class)
            .collect()
    }

    /// Live e-nodes of a class, ascending.
    pub(crate) fn class_nodes(&self, class: ClassId) -> Vec<NodeId> {
        let class = self.find(class);
        self.classes[class as usize]
            .nodes
            .iter()
            .copied()
            .filter(|id| self.nodes[*id as usize].alive)
            .collect()
    }

    /// The canonical form of a live e-node.
    pub(crate) fn node(&self, id: NodeId) -> ENode {
        self.canonical(self.nodes[id as usize].node)
    }

    pub(crate) fn node_class(&self, id: NodeId) -> ClassId {
        self.find(self.nodes[id as usize].class)
    }

    #[cfg(test)]
    pub(crate) fn node_may_trap(&self, id: NodeId) -> bool {
        self.nodes[id as usize].may_trap
    }

    pub(crate) fn node_origin(&self, id: NodeId) -> Option<u16> {
        self.nodes[id as usize].origin
    }

    #[cfg(test)]
    pub(crate) fn node_alive(&self, id: NodeId) -> bool {
        self.nodes[id as usize].alive
    }

    pub(crate) fn data(&self, class: ClassId) -> &ClassData {
        &self.classes[self.find(class) as usize].data
    }

    /// Live e-nodes that may trap, ascending. Every candidate keeps them.
    pub(crate) fn pinned(&self) -> Vec<NodeId> {
        (0..self.nodes.len())
            .map(|id| id as NodeId)
            .filter(|id| {
                let entry = &self.nodes[*id as usize];
                entry.alive && entry.may_trap
            })
            .collect()
    }

    /// E-nodes ever created, including retired duplicates; this is the
    /// quantity `max_enodes` bounds.
    pub(crate) fn enode_count(&self) -> u32 {
        u32::try_from(self.nodes.len()).unwrap_or(u32::MAX)
    }

    #[cfg(test)]
    pub(crate) fn live_node_count(&self) -> u32 {
        u32::try_from(self.nodes.iter().filter(|entry| entry.alive).count()).unwrap_or(u32::MAX)
    }

    pub(crate) fn class_count(&self) -> u32 {
        self.live_classes
    }

    pub(crate) fn merges(&self) -> u64 {
        self.merges
    }

    pub(crate) fn refused_merges(&self) -> u64 {
        self.refused
    }

    #[cfg(test)]
    /// Whether `rebuild` has nothing to do.
    pub(crate) fn clean(&self) -> bool {
        self.dirty.is_empty()
    }
}
