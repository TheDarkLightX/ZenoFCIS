//! Functional, first-order production of a lossless syntax derivation.
//!
//! Immutable tokens enter this invocation; all stacks are exclusively owned
//! scratch. The completed candidate is accepted only by the independently
//! verified certificate checker. Semantic lowering consumes the original token
//! payloads selected by the certificate's leaves.

use crate::DiagnosticCode;
use crate::lexer::{Token, TokenKind};
use crate::syntax_certificate::{self, Node, Symbol};
use crate::syntax_grammar::{
    self as grammar, DECISION_RANGES, DECISIONS, EOF_TAG, PRODUCTIONS, RHS, ROOT_TAG,
};
use alloc::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SyntaxTree {
    pub(crate) nodes: Vec<Node>,
    pub(crate) children: Vec<usize>,
    pub(crate) root: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SyntaxError {
    pub(crate) token: usize,
    pub(crate) resource_limit: bool,
    pub(crate) code: DiagnosticCode,
    pub(crate) message: &'static str,
}

struct Frame {
    production: usize,
    next: usize,
    start: usize,
    first_node: usize,
    pending_start: usize,
}

fn selected(tags: &[u32], position: usize, lhs: u32) -> Option<usize> {
    let first = tags.get(position).copied().unwrap_or(EOF_TAG);
    let second = position
        .checked_add(1)
        .and_then(|i| tags.get(i))
        .copied()
        .unwrap_or(EOF_TAG);
    let (start, count) = *DECISION_RANGES.get(lhs as usize)?;
    let rows = DECISIONS.get(start..start.checked_add(count)?)?;
    syntax_certificate::select_production(rows, lhs, first, second)
}

/// Produces then independently checks a complete derivation. EOF is lookahead,
/// never a leaf; source payloads are never copied into a candidate certificate.
pub(crate) fn parse_tokens(tokens: &[Token]) -> Result<SyntaxTree, SyntaxError> {
    let bad = |token, resource_limit, message| SyntaxError {
        token,
        resource_limit,
        code: if resource_limit {
            DiagnosticCode::ResourceLimit
        } else {
            DiagnosticCode::InvalidDeclaration
        },
        message,
    };
    if tokens.last().map(|t| &t.kind) != Some(&TokenKind::End) {
        return Err(bad(0, false, "token sequence must end with EOF"));
    }
    let count = tokens.len() - 1;
    if tokens[..count]
        .iter()
        .any(|t| matches!(t.kind, TokenKind::End))
    {
        return Err(bad(0, false, "EOF cannot occur inside the token sequence"));
    }
    let tags: Vec<u32> = tokens[..count].iter().map(grammar::token_tag).collect();
    // Explicit operational bounds are separate from grammar membership. The
    // fixed grammar has no left recursion (checked by generation); exhausting
    // either envelope is a refusal, never a successful prefix parse.
    let max_nodes = count.saturating_mul(16).saturating_add(1024);
    let max_work = max_nodes.saturating_mul(4);
    let initial = selected(&tags, 0, ROOT_TAG).ok_or(SyntaxError {
        code: DiagnosticCode::MissingHeader,
        ..bad(0, false, "expected a zeno project header")
    })?;
    let mut frames = Vec::new();
    let mut pending = Vec::new();
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    frames.push(Frame {
        production: initial,
        next: 0,
        start: 0,
        first_node: 0,
        pending_start: 0,
    });
    let mut position = 0usize;
    let mut work = 0usize;
    while !frames.is_empty() {
        if work >= max_work || nodes.len().saturating_add(frames.len()) >= max_nodes {
            return Err(bad(
                position,
                true,
                "syntax derivation work/node limit exceeded",
            ));
        }
        work += 1;
        let index = frames.len() - 1;
        let production = *PRODUCTIONS.get(frames[index].production).ok_or(bad(
            position,
            false,
            "invalid generated production",
        ))?;
        if frames[index].next < production.rhs_len {
            let symbol = *RHS
                .get(production.rhs_start + frames[index].next)
                .ok_or(bad(position, false, "invalid generated right-hand side"))?;
            frames[index].next += 1;
            if symbol.terminal {
                if tags.get(position).copied() != Some(symbol.tag) {
                    return Err(SyntaxError {
                        code: DiagnosticCode::ExpectedToken,
                        ..bad(
                            position,
                            false,
                            grammar::terminal_name(symbol.tag)
                                .unwrap_or("terminal from selected grammar production"),
                        )
                    });
                }
                let node_index = nodes.len();
                nodes.push(Node {
                    symbol,
                    production: 0,
                    children_start: children.len(),
                    children_len: 0,
                    start: position,
                    end: position + 1,
                    first_node: node_index,
                });
                position += 1;
                pending.push(node_index);
            } else {
                let choice = selected(&tags, position, symbol.tag).ok_or(bad(
                    position,
                    false,
                    "no grammar production matches the current tokens",
                ))?;
                frames.push(Frame {
                    production: choice,
                    next: 0,
                    start: position,
                    first_node: nodes.len(),
                    pending_start: pending.len(),
                });
            }
        } else {
            let frame = match frames.pop() {
                Some(frame) => frame,
                None => return Err(bad(position, false, "empty parser stack")),
            };
            let child_count = pending.len().checked_sub(frame.pending_start).ok_or(bad(
                position,
                false,
                "invalid parser child stack",
            ))?;
            if child_count != production.rhs_len {
                return Err(bad(position, false, "incomplete production children"));
            }
            let child_start = children.len();
            children.extend_from_slice(&pending[frame.pending_start..]);
            pending.truncate(frame.pending_start);
            let node_index = nodes.len();
            nodes.push(Node {
                symbol: Symbol {
                    terminal: false,
                    tag: production.lhs,
                },
                production: frame.production,
                children_start: child_start,
                children_len: child_count,
                start: frame.start,
                end: position,
                first_node: frame.first_node,
            });
            pending.push(node_index);
        }
    }
    if position != count || pending.len() != 1 {
        return Err(bad(
            position,
            false,
            "complete source consumption is required",
        ));
    }
    let root = pending[0];
    if nodes.len().checked_sub(1) != Some(root) {
        return Err(bad(
            position,
            false,
            "lowering root must be the certified final node",
        ));
    }
    if !syntax_certificate::check_derivation(
        &tags,
        PRODUCTIONS,
        RHS,
        &nodes,
        &children,
        DECISIONS,
        EOF_TAG,
        ROOT_TAG,
        max_nodes,
    ) {
        return Err(bad(
            position,
            false,
            "syntax derivation failed independent validation",
        ));
    }
    Ok(SyntaxTree {
        nodes,
        children,
        root,
    })
}
