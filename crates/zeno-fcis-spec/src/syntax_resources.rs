//! Numeric resource preflight over a complete, independently checked CST.
//!
//! No recursive AST exists during this pass. Source recursion, left-folded
//! expression height, and conservative table-expansion height are accounted
//! separately. The semantic expansion pass still charges its actual node work;
//! this executable guard is not a proof of that pass's semantic mapping.

use crate::surface_lexer::{Token, TokenKind};
use crate::syntax::{SyntaxError, SyntaxTree};
use crate::{DiagnosticCode, MAX_FORMULA_DEPTH, MAX_PARSED_EXPRESSION_HEIGHT, MAX_TABLE_ENTRIES};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The legacy semantic parser has several Rust frames per source nesting level.
/// Deeper core expressions use the iterative fold, after both certificates.
pub(crate) const MAX_SEMANTIC_RECURSION: usize = 32;

#[derive(Clone, Copy, Default)]
struct Shape {
    height: usize,
    recursion: usize,
    source_nesting: usize,
}

pub(crate) struct ResourceUse {
    pub(crate) recursion: usize,
}

/// Height and extent of an imported indexed value. Derived formula heights
/// include their Circuit wrapper; bit reads add scalar packing at the use site.
/// Constant tables reserve the two layers of a negative signed literal.
#[derive(Clone, Copy)]
pub(crate) struct DerivedHeight {
    pub(crate) height: usize,
    pub(crate) bits: bool,
    pub(crate) size: usize,
}

fn refuse(tree: &SyntaxTree, node: usize, message: &'static str) -> SyntaxError {
    SyntaxError {
        token: tree.nodes.get(node).map_or(0, |node| node.start),
        resource_limit: true,
        code: DiagnosticCode::ResourceLimit,
        message,
    }
}

fn name(tree: &SyntaxTree, node: usize) -> &'static str {
    let node = &tree.nodes[node];
    if node.symbol.terminal {
        ""
    } else {
        tree.grammar.rule_name(node.symbol.tag).unwrap_or("")
    }
}

/// Flatten only generated EBNF helpers. Each helper belongs to one authored
/// parent, so this performs linear work over the certificate as a whole.
fn parts(tree: &SyntaxTree, node: usize, output: &mut Vec<usize>, pending: &mut Vec<usize>) {
    output.clear();
    pending.clear();
    let node = &tree.nodes[node];
    pending.extend(
        tree.children[node.children_start..node.children_start + node.children_len]
            .iter()
            .rev()
            .copied(),
    );
    while let Some(child) = pending.pop() {
        if name(tree, child).starts_with('_') {
            let child = &tree.nodes[child];
            pending.extend(
                tree.children[child.children_start..child.children_start + child.children_len]
                    .iter()
                    .rev()
                    .copied(),
            );
        } else {
            output.push(child);
        }
    }
}

pub(crate) fn check(
    tree: &SyntaxTree,
    tokens: &[Token<'_>],
    entries: &BTreeMap<u32, usize>,
    ambient_derived: &BTreeMap<String, DerivedHeight>,
    tables: &BTreeMap<u32, u32>,
) -> Result<ResourceUse, SyntaxError> {
    let mut derived = ambient_derived.clone();
    let mut own_scopes = BTreeMap::new();
    for node in &tree.nodes {
        if !node.symbol.terminal
            && tree.grammar.rule_name(node.symbol.tag) == Some("DerivedTable")
            && let Some(label) = tokens.get(node.start + 3)
        {
            own_scopes
                .entry(label.spelling)
                .or_insert((node.start, node.end));
        }
    }
    let table_size = |at: usize| -> usize {
        tokens
            .get(at)
            .and_then(|token| match token.kind {
                TokenKind::Number(value) => u32::try_from(value).ok(),
                _ => None,
            })
            .and_then(|id| tables.get(&id).copied())
            .filter(|size| (1..=MAX_TABLE_ENTRIES).contains(size))
            .map_or(MAX_TABLE_ENTRIES as usize, |size| size as usize)
    };
    let mut shapes: Vec<Shape> = Vec::with_capacity(tree.nodes.len());
    let mut children = Vec::new();
    let mut pending = Vec::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let mut shape = Shape::default();
        for &child in &tree.children[node.children_start..node.children_start + node.children_len] {
            let child = shapes[child];
            shape.height = shape.height.max(child.height);
            shape.recursion = shape.recursion.max(child.recursion);
            shape.source_nesting = shape.source_nesting.max(child.source_nesting);
        }
        let recursive_children = shape.recursion;
        let rule = name(tree, index);
        let first = tokens.get(node.start);
        let word = first.map_or("", |token| token.spelling);
        match rule {
            "ExprNamed" => {
                parts(tree, index, &mut children, &mut pending);
                let imported = derived.get(word).copied();
                let own = own_scopes
                    .get(word)
                    .is_some_and(|&(start, end)| start <= node.start && node.end <= end)
                    && !children.iter().any(|&child| {
                        name(tree, child) == "ProjectionSuffix"
                            && tokens[tree.nodes[child].start].kind == TokenKind::Dot
                    });
                let mut indexes = 0;
                let mut selection = 0usize;
                let mut size = imported.map_or(MAX_TABLE_ENTRIES as usize, |value| value.size);
                for &child in &children {
                    let suffix = &tree.nodes[child];
                    if name(tree, child) != "ProjectionSuffix" {
                        continue;
                    }
                    if tokens[suffix.start].kind == TokenKind::Dot {
                        size = table_size(suffix.start + 1);
                    } else if tokens[suffix.start].kind == TokenKind::LBracket {
                        indexes += 1;
                        if !own
                            && (suffix.end - suffix.start != 3
                                || !matches!(tokens[suffix.start + 1].kind, TokenKind::Number(_)))
                        {
                            selection = selection.saturating_add(size.saturating_add(2));
                        }
                    }
                }
                // Active self reads are temporary two-level predicate leaves;
                // valid recursive indices are literal after bound substitution.
                let base = if own {
                    2
                } else {
                    imported.map_or(if indexes == 0 { 1 } else { 2 }, |value| value.height)
                };
                shape.height = shape.height.max(base).saturating_add(selection);
                if indexes > 0 && !own && imported.is_some_and(|value| value.bits) {
                    shape.height = shape.height.saturating_add(8);
                }
            }
            "ExprAtom" => {
                if first.is_some_and(|token| token.kind == TokenKind::LParen) {
                    // This is exactly the semantic parser's value-group
                    // lookahead. Scalar groups are now consumed iteratively;
                    // other groups conservatively retain native recursion.
                    let scalar = tokens.get(node.end).is_some_and(|token| {
                        matches!(
                            token.kind,
                            TokenKind::Plus
                                | TokenKind::Minus
                                | TokenKind::Star
                                | TokenKind::EqEq
                                | TokenKind::NotEq
                                | TokenKind::Less
                                | TokenKind::LessEq
                                | TokenKind::Greater
                                | TokenKind::GreaterEq
                        )
                    });
                    if scalar {
                        shape.source_nesting += 1;
                    } else {
                        shape.recursion += 1;
                    }
                } else if first.is_some_and(|token| matches!(token.kind, TokenKind::Number(_))) {
                    shape.height = 1;
                }
            }
            "ExprUnary" => {
                if first
                    .is_some_and(|token| matches!(token.kind, TokenKind::Bang | TokenKind::Minus))
                    || word == "not"
                {
                    let minimum = node.end - node.start == 2
                        && first.is_some_and(|token| token.kind == TokenKind::Minus)
                        && tokens[node.start + 1].kind == TokenKind::Number(1u128 << 127);
                    if minimum {
                        shape.height = 1;
                    } else {
                        shape.height = shape.height.max(1) + 1;
                        // Relational !/not and scalar minus prefixes are
                        // consumed iteratively, but still spend source nesting.
                        shape.source_nesting += 1;
                    }
                }
            }
            "ExprImplies" => {
                parts(tree, index, &mut children, &mut pending);
                if children
                    .iter()
                    .any(|&child| name(tree, child) == "ExprImplies")
                {
                    shape.height += 1;
                    shape.recursion += 1;
                }
            }
            "ExprComparison" => {
                parts(tree, index, &mut children, &mut pending);
                if children
                    .iter()
                    .any(|&child| name(tree, child) == "CompareOp")
                {
                    shape.height += 1;
                }
            }
            "ExprOr" | "ExprAnd" | "ExprAdd" | "ExprTerm" | "TemporalUntil" | "TemporalOr"
            | "TemporalAnd" => {
                let operand = match rule {
                    "ExprOr" => "ExprAnd",
                    "ExprAnd" => "ExprComparison",
                    "ExprAdd" => "ExprTerm",
                    "ExprTerm" => "ExprUnary",
                    "TemporalUntil" => "TemporalOr",
                    "TemporalOr" => "TemporalAnd",
                    _ => "TemporalAtom",
                };
                parts(tree, index, &mut children, &mut pending);
                let mut height = None;
                let mut count = 0;
                for &child in &children {
                    if name(tree, child) != operand {
                        continue;
                    }
                    count += 1;
                    height = Some(match height {
                        None => shapes[child].height,
                        Some(left) => crate::value_expr::binary_height(
                            left,
                            shapes[child].height,
                            MAX_PARSED_EXPRESSION_HEIGHT,
                        )
                        .ok_or_else(|| {
                            refuse(
                                tree,
                                index,
                                "constructed expression height exceeds the parser limit",
                            )
                        })?,
                    });
                }
                if count > MAX_FORMULA_DEPTH {
                    return Err(refuse(
                        tree,
                        index,
                        "operator chain exceeds the parser limit",
                    ));
                }
                shape.height = height.unwrap_or(shape.height);
            }
            "ExprCall" => {
                shape.height += match word {
                    "sum" | "count" => MAX_TABLE_ENTRIES as usize,
                    // A lower bit extraction adds Div/Mul/Sub/Compare; the
                    // operation adds one Boolean layer, then a Select and up
                    // to seven balanced Add levels pack at most 128 bits.
                    "complement" | "join" | "meet" => 13,
                    // The temporary subset conjunction is built before it is
                    // shared: up to 128 entries plus extraction and wrappers.
                    "subset" => tokens
                        .get(node.start + 2)
                        .and_then(|token| match token.kind {
                            TokenKind::Number(width) if (1..=128).contains(&width) => {
                                usize::try_from(width).ok()
                            }
                            _ => None,
                        })
                        .unwrap_or(128)
                        .saturating_add(6),
                    _ => 1,
                };
                shape.recursion += 1;
            }
            "QuantifiedDomain" | "SumDomain" => {
                shape.height += if word == "table" {
                    table_size(node.start + 1).saturating_add(2)
                } else if word == "entries" {
                    let map = tokens
                        .get(node.start + 1)
                        .and_then(|token| match token.kind {
                            TokenKind::Number(value) => u32::try_from(value).ok(),
                            _ => None,
                        });
                    map.and_then(|map| entries.get(&map))
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(2)
                } else {
                    1
                };
                shape.recursion += 1;
            }
            "TableCount" => {
                // A charged count can have at most MAX_FORMULA_NODES clauses:
                // at most 20 balanced levels, 64 conjunction entries, and four
                // comparison/negation wrappers. The expansion checks node work.
                shape.height = shape.height.max(1) + table_size(node.start + 4) + 24;
                shape.recursion += 1;
            }
            "BitFold" => {
                shape.height = shape.height.max(1) + table_size(node.start + 4) + 13;
                shape.recursion += 1;
            }
            "ProjectionSuffix" if first.is_some_and(|token| token.kind == TokenKind::LBracket) => {
                shape.recursion += 1;
            }
            "TableRead" => {
                parts(tree, index, &mut children, &mut pending);
                let imported = derived.get(word).copied();
                let mut size = imported.map_or(MAX_TABLE_ENTRIES as usize, |value| value.size);
                let mut selection = 0usize;
                for &child in &children {
                    let value = &tree.nodes[child];
                    if name(tree, child) == "ProjectionSegment" {
                        size = table_size(value.start);
                    }
                    if name(tree, child) == "Rel"
                        && (value.end - value.start != 1
                            || !matches!(tokens[value.start].kind, TokenKind::Number(_)))
                    {
                        selection = selection.saturating_add(size.saturating_add(2));
                    }
                }
                shape.height = shape
                    .height
                    .max(imported.map_or(2, |value| value.height))
                    .saturating_add(selection)
                    .saturating_add(if imported.is_some_and(|value| value.bits) {
                        8
                    } else {
                        0
                    })
                    .saturating_add(1);
                shape.recursion += 1;
            }
            "Circuit" => {
                shape.height = shape.height.max(1).saturating_add(1);
            }
            "DerivedTable" => {
                if let Some(label) = tokens.get(node.start + 3) {
                    let header = node.start
                        + if tokens
                            .get(node.start + 7)
                            .is_some_and(|token| token.kind == TokenKind::LBracket)
                        {
                            13
                        } else {
                            10
                        };
                    let bits = tokens
                        .get(header)
                        .is_some_and(|token| token.spelling == "bits");
                    // Definitions are visited in certified source order. The
                    // semantic pass refuses forward/mutual reads; a later read
                    // composes its full imported formula height here.
                    derived
                        .entry(String::from(label.spelling))
                        .or_insert(DerivedHeight {
                            height: shape.height.max(1).saturating_add(1),
                            bits,
                            size: table_size(header - 1),
                        });
                }
            }
            "ConstantTable" => {
                if let Some(label) = tokens.get(node.start + 3) {
                    derived
                        .entry(String::from(label.spelling))
                        .or_insert(DerivedHeight {
                            height: 2,
                            bits: false,
                            size: table_size(node.start + 2),
                        });
                }
            }
            "CircuitNode" if word == "formula" => {
                shape.recursion += 1;
            }
            "TemporalAtom" => {
                if first.is_some_and(|token| token.kind == TokenKind::LParen) {
                    shape.recursion += 1;
                } else {
                    shape.height = if shape.height == 0 {
                        2
                    } else {
                        shape.height + 1
                    };
                    if !matches!(word, "true" | "false") {
                        shape.recursion += 1;
                    }
                }
            }
            _ => {}
        }
        shape.source_nesting += shape.recursion - recursive_children;
        if shape.height > MAX_PARSED_EXPRESSION_HEIGHT {
            return Err(refuse(
                tree,
                index,
                "constructed expression height exceeds the parser limit",
            ));
        }
        if shape.source_nesting > MAX_FORMULA_DEPTH {
            return Err(refuse(
                tree,
                index,
                "source expression nesting exceeds the parser limit",
            ));
        }
        shapes.push(shape);
    }
    Ok(ResourceUse {
        recursion: shapes[tree.root].recursion,
    })
}
