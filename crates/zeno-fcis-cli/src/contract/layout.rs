//! Line layout for generated contract source, matching `rustfmt --edition 2024`.
//!
//! Committed contracts are rustfmt output and `--check` compares bytes, so this
//! module makes rustfmt's decisions for the few expression forms a contract
//! uses: literals and paths, calls, struct literals, arrays, `vec!`, tuples,
//! references and the limits method chain. Widths are rustfmt's defaults:
//! `max_width` 100, `fn_call_width` 60, `array_width` 60, `struct_lit_width`
//! 18, `chain_width` 60 and `short_array_element_width_threshold` 10.
//!
//! A literal too long for any layout makes rustfmt keep the whole item as
//! written. Such an item is laid out again with literals allowed past the
//! margin, which rustfmt then leaves unchanged.

const MAX_WIDTH: usize = 100;
const INDENT: usize = 4;
const CALL_WIDTH: usize = 60;
const ARRAY_WIDTH: usize = 60;
const STRUCT_WIDTH: usize = 18;
const CHAIN_WIDTH: usize = 60;
const SHORT_ITEM_WIDTH: usize = 10;

/// Where an expression starts and how much room its first line has.
#[derive(Clone, Copy, Debug)]
struct Shape {
    /// Indentation of continuation lines.
    indent: usize,
    /// Columns already written on the first line after `indent`.
    offset: usize,
    /// Columns left on the first line.
    width: usize,
    /// Whether text may run past the margin.
    overflow: bool,
}

impl Shape {
    fn indented(indent: usize, overflow: bool) -> Self {
        Self {
            indent,
            offset: 0,
            width: MAX_WIDTH.saturating_sub(indent),
            overflow,
        }
    }

    fn offset_left(self, width: usize) -> Option<Self> {
        Some(Self {
            offset: self.offset + width,
            width: self.shrink(width)?,
            ..self
        })
    }

    fn sub_width(self, width: usize) -> Option<Self> {
        Some(Self {
            width: self.shrink(width)?,
            ..self
        })
    }

    fn shrink(self, width: usize) -> Option<usize> {
        if self.overflow {
            Some(self.width.saturating_sub(width))
        } else {
            self.width.checked_sub(width)
        }
    }

    fn fits(self, width: usize) -> bool {
        self.overflow || width <= self.width
    }

    /// The full-width shape one indentation level deeper.
    fn nested(self) -> Self {
        Self::indented(self.indent + INDENT, self.overflow)
    }
}

/// One expression of the subset contracts are written in.
#[derive(Clone, Debug)]
pub(super) enum Syntax {
    /// A literal or path, never broken. rustfmt treats literals and
    /// single-segment paths as simple.
    Atom {
        text: String,
        simple: bool,
    },
    Call {
        callee: String,
        args: Vec<Syntax>,
    },
    Struct {
        path: String,
        fields: Vec<(&'static str, Syntax)>,
    },
    List {
        kind: ListKind,
        items: Vec<Syntax>,
    },
    Ref(Box<Syntax>),
    /// A method chain; each call's callee starts with `.`.
    Chain {
        head: Box<Syntax>,
        calls: Vec<Syntax>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ListKind {
    Array,
    Vec,
    Tuple,
}

impl Syntax {
    pub(super) fn literal(text: impl ToString) -> Self {
        Self::Atom {
            text: text.to_string(),
            simple: true,
        }
    }

    pub(super) fn path(text: impl Into<String>) -> Self {
        let text = text.into();
        let simple = !text.contains("::");
        Self::Atom { text, simple }
    }

    /// A byte string literal; bytes outside printable ASCII are escaped.
    pub(super) fn bytes(value: &[u8]) -> Self {
        Self::literal(byte_string(value))
    }

    pub(super) fn call(callee: impl Into<String>, args: Vec<Self>) -> Self {
        Self::Call {
            callee: callee.into(),
            args,
        }
    }

    pub(super) fn record(path: impl Into<String>, fields: Vec<(&'static str, Self)>) -> Self {
        Self::Struct {
            path: path.into(),
            fields,
        }
    }

    pub(super) fn array(items: Vec<Self>) -> Self {
        Self::List {
            kind: ListKind::Array,
            items,
        }
    }

    /// `&[items]`.
    pub(super) fn slice(items: Vec<Self>) -> Self {
        Self::Ref(Box::new(Self::array(items)))
    }

    pub(super) fn vec(items: Vec<Self>) -> Self {
        Self::List {
            kind: ListKind::Vec,
            items,
        }
    }

    pub(super) fn tuple(items: Vec<Self>) -> Self {
        Self::List {
            kind: ListKind::Tuple,
            items,
        }
    }

    pub(super) fn reference(inner: Self) -> Self {
        Self::Ref(Box::new(inner))
    }

    pub(super) fn chain(head: Self, calls: Vec<Self>) -> Self {
        Self::Chain {
            head: Box::new(head),
            calls,
        }
    }

    fn rewrite(&self, shape: Shape) -> Option<String> {
        match self {
            Self::Atom { text, .. } => shape.fits(text.len()).then(|| text.clone()),
            Self::Ref(inner) => Some(format!("&{}", inner.rewrite(shape.offset_left(1)?)?)),
            Self::Call { callee, args } => {
                if !shape.fits(callee.len()) {
                    return None;
                }
                list(callee, ("(", ")"), args, shape, CALL_WIDTH)
            }
            Self::List { kind, items } => match kind {
                ListKind::Array => list("", ("[", "]"), items, shape, ARRAY_WIDTH),
                ListKind::Vec => list("vec!", ("[", "]"), items, shape, ARRAY_WIDTH),
                ListKind::Tuple => list("", ("(", ")"), items, shape, CALL_WIDTH),
            },
            Self::Struct { path, fields } => struct_literal(path, fields, shape),
            Self::Chain { head, calls } => chain(head, calls, shape),
        }
    }

    fn is_simple(&self) -> bool {
        match self {
            Self::Atom { simple, .. } => *simple,
            Self::Ref(inner) => inner.is_simple(),
            _ => false,
        }
    }

    /// Whether the last of `count` list items may continue past the opening line.
    fn can_overflow(&self, count: usize) -> bool {
        match self {
            Self::Atom { .. } => false,
            Self::Ref(inner) => inner.can_overflow(count),
            _ => count == 1,
        }
    }

    fn is_nested_call(&self) -> bool {
        match self {
            Self::Call { .. }
            | Self::List {
                kind: ListKind::Vec,
                ..
            } => true,
            Self::Ref(inner) => inner.is_nested_call(),
            _ => false,
        }
    }
}

/// `{lhs} {rhs};` for an item at column zero, breaking after `=` only when
/// rustfmt would.
pub(super) fn assignment(lhs: &str, rhs: &Syntax) -> Option<String> {
    assign(lhs, rhs, false).or_else(|| assign(lhs, rhs, true))
}

/// The tail expression of a function body at `indent`.
pub(super) fn statement(indent: usize, expression: &Syntax) -> Option<String> {
    expression
        .rewrite(Shape::indented(indent, false))
        .or_else(|| expression.rewrite(Shape::indented(indent, true)))
}

fn assign(lhs: &str, rhs: &Syntax, overflow: bool) -> Option<String> {
    // One column is reserved for the `;`.
    let shape = Shape {
        width: MAX_WIDTH - 1,
        ..Shape::indented(0, overflow)
    };
    let same_line = shape.offset_left(lhs.len() + 1).unwrap_or(Shape {
        width: 0,
        offset: shape.offset + lhs.len() + 1,
        ..shape
    });
    let original = rhs.rewrite(same_line);
    let chosen = match &original {
        Some(text) if !text.contains('\n') && text.len() <= same_line.width => format!(" {text}"),
        _ => {
            let overhead =
                MAX_WIDTH.saturating_sub(same_line.indent + same_line.offset + same_line.width);
            let next_shape = shape.nested().sub_width(overhead)?;
            let next = rhs.rewrite(next_shape);
            let indent = format!("\n{}", spaces(next_shape.indent));
            match (original, next) {
                (Some(original), Some(next)) if !fits(&next, next_shape) => format!(" {original}"),
                (Some(original), Some(next)) if prefer_next_line(&original, &next) => {
                    format!("{indent}{next}")
                }
                (None, Some(next)) => format!("{indent}{next}"),
                (Some(original), _) => format!(" {original}"),
                (None, None) => return None,
            }
        }
    };
    Some(format!("{lhs}{chosen};"))
}

fn fits(text: &str, shape: Shape) -> bool {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    if first.len() > shape.width {
        return false;
    }
    if !text.contains('\n') {
        return true;
    }
    if text.lines().skip(1).any(|line| line.len() > MAX_WIDTH) {
        return false;
    }
    last_line_width(text) <= shape.indent + shape.offset + shape.width
}

fn prefer_next_line(original: &str, next: &str) -> bool {
    let ends = |text: &str, delimiter: char| {
        text.lines()
            .next()
            .is_some_and(|line| line.ends_with(delimiter))
    };
    !next.contains('\n')
        || newlines(original) > newlines(next) + 1
        || ['(', '{', '[']
            .iter()
            .any(|delimiter| ends(original, *delimiter) && !ends(next, *delimiter))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tactic {
    Horizontal,
    Vertical,
    Mixed,
}

/// rustfmt's delimited-list layout (`overflow::Context`): items on one line,
/// the last item continuing past the opening line, one item per line, or
/// short simple items filling lines.
fn list(
    ident: &str,
    (open, close): (&str, &str),
    items: &[Syntax],
    shape: Shape,
    item_max_width: usize,
) -> Option<String> {
    let one_line_width = shape.width.saturating_sub(ident.len() + 2);
    let one_line_shape = shape
        .offset_left(ident.len() + 1)
        .and_then(|shape| shape.sub_width(1))
        .unwrap_or(Shape { width: 0, ..shape });
    let nested = shape.nested();
    // One column is reserved for each item's comma.
    let item_shape = Shape {
        width: nested.width.saturating_sub(1),
        ..nested
    };
    let mut rendered: Vec<Option<String>> =
        items.iter().map(|item| item.rewrite(item_shape)).collect();
    let last = items.len().checked_sub(1);
    let combine = items.len() == 1 && ident.len() < INDENT;
    let overflow = combine
        || items
            .last()
            .is_some_and(|item| item.can_overflow(items.len()));
    let mut overflowed = None;
    if overflow && let (Some(last), Some(item)) = (last, items.last()) {
        let shape = if items.len() == 1 && !item.is_nested_call() {
            Some(one_line_shape)
        } else {
            let before: usize = rendered[..last]
                .iter()
                .map(|text| 2 + text.as_deref().map_or(0, str::len))
                .sum();
            Shape {
                width: item_max_width.min(one_line_shape.width),
                ..one_line_shape
            }
            .offset_left(before)
        };
        if let Some(full) = shape.and_then(|shape| item.rewrite(shape)) {
            rendered[last] = Some(first_line(&full).to_owned());
            overflowed = Some(full);
        }
    }
    let mut tactic = definitive_tactic(&rendered, one_line_width.min(item_max_width));
    match (overflow, tactic, overflowed, last) {
        (true, Tactic::Horizontal, Some(full), Some(last)) if items.len() == 1 => {
            rendered[last] = if newlines(&full) == 1 {
                match items[last].rewrite(item_shape) {
                    Some(text) if !text.contains('\n') => Some(text),
                    _ => Some(full),
                }
            } else {
                Some(full)
            };
        }
        (true, Tactic::Horizontal, Some(full), Some(last)) => rendered[last] = Some(full),
        (_, _, _, Some(last)) => {
            rendered[last] = items[last].rewrite(item_shape);
            if items.len() == 1
                && one_line_width != 0
                && rendered[0]
                    .as_deref()
                    .is_some_and(|text| !text.contains('\n') && text.len() <= one_line_width)
            {
                tactic = Tactic::Horizontal;
            } else {
                tactic = definitive_tactic(&rendered, one_line_width.min(item_max_width));
                if tactic == Tactic::Vertical
                    && items.iter().all(Syntax::is_simple)
                    && rendered
                        .iter()
                        .all(|text| text.as_deref().map_or(0, str::len) <= SHORT_ITEM_WIDTH)
                {
                    tactic = Tactic::Mixed;
                }
            }
        }
        _ => {}
    }
    let ends_with_newline = tactic != Tactic::Horizontal;
    let body = write_list(&rendered, tactic, item_shape, ends_with_newline)?;
    let extend_width = if body.is_empty() {
        2
    } else {
        first_line(&body).len() + 1
    };
    let mut result = format!("{ident}{open}");
    if tactic == Tactic::Horizontal && extend_width <= shape.width.saturating_sub(ident.len()) {
        result.push_str(&body);
    } else {
        if !body.is_empty() {
            result.push('\n');
            result.push_str(&spaces(nested.indent));
            result.push_str(&body);
        }
        result.push('\n');
        result.push_str(&spaces(shape.indent));
    }
    result.push_str(close);
    Some(result)
}

/// Horizontal when every item is one line and the items with `, ` between
/// them fit in `width`.
fn definitive_tactic(items: &[Option<String>], width: usize) -> Tactic {
    let total: usize = items
        .iter()
        .map(|text| text.as_deref().map_or(0, str::len))
        .sum::<usize>()
        + 2 * items.len().saturating_sub(1);
    let multiline = items
        .iter()
        .any(|text| text.as_deref().is_some_and(|text| text.contains('\n')));
    if total <= width && !multiline {
        Tactic::Horizontal
    } else {
        Tactic::Vertical
    }
}

/// rustfmt's `write_list` with trailing commas on vertical layouts. A missing
/// item fails the whole list.
fn write_list(
    items: &[Option<String>],
    tactic: Tactic,
    shape: Shape,
    ends_with_newline: bool,
) -> Option<String> {
    let indent = spaces(shape.indent);
    let mut trailing = tactic == Tactic::Vertical;
    let mut result = String::new();
    let mut line_width = 0;
    for (index, item) in items.iter().enumerate() {
        let item = item.as_deref()?;
        let first = index == 0;
        let last = index + 1 == items.len();
        let mut separate = !last || trailing;
        match tactic {
            Tactic::Horizontal if !first => result.push(' '),
            Tactic::Vertical if !first && !item.is_empty() && !result.is_empty() => {
                result.push('\n');
                result.push_str(&indent);
            }
            Tactic::Mixed => {
                let width = item.len() + usize::from(separate);
                if line_width > 0 && line_width + 1 + width > shape.width {
                    result.push('\n');
                    result.push_str(&indent);
                    line_width = 0;
                    if ends_with_newline {
                        trailing = true;
                    }
                } else if line_width > 0 {
                    result.push(' ');
                    line_width += 1;
                }
                if last && ends_with_newline {
                    separate = true;
                }
                line_width += width;
            }
            _ => {}
        }
        result.push_str(item);
        if separate {
            result.push(',');
        }
    }
    Some(result)
}

/// rustfmt's struct literal: `Path { a: 1 }` when the fields fit
/// `struct_lit_width`, otherwise one field per line.
fn struct_literal(path: &str, fields: &[(&'static str, Syntax)], shape: Shape) -> Option<String> {
    if !shape.sub_width(2)?.fits(path.len()) {
        return None;
    }
    if fields.is_empty() {
        return Some(format!("{path} {{}}"));
    }
    let vertical = shape.nested();
    // `Path { ` and ` }`.
    let horizontal = shape
        .width
        .checked_sub(path.len() + 5)
        .map(|width| width.min(STRUCT_WIDTH));
    let field_shape = vertical.sub_width(1)?;
    let items: Vec<Option<String>> = fields
        .iter()
        .map(|(name, value)| field(name, value, field_shape))
        .collect();
    let tactic = match horizontal {
        Some(width) => definitive_tactic(&items, width),
        None => Tactic::Vertical,
    };
    let list_shape = match (tactic, horizontal) {
        (Tactic::Horizontal, Some(width)) => Shape { width, ..shape },
        _ => vertical,
    };
    let body = write_list(&items, tactic, list_shape, tactic == Tactic::Vertical)?;
    if body.contains('\n') || body.len() > horizontal.unwrap_or(0) {
        Some(format!(
            "{path} {{\n{}{body}\n{}}}",
            spaces(vertical.indent),
            spaces(shape.indent)
        ))
    } else {
        Some(format!("{path} {{ {body} }}"))
    }
}

fn field(name: &str, value: &Syntax, shape: Shape) -> Option<String> {
    let value_shape = shape.offset_left(name.len() + 2)?;
    if let Some(value) = value.rewrite(value_shape) {
        return Some(format!("{name}: {value}"));
    }
    let next = shape.nested();
    let value = value.rewrite(next)?;
    Some(format!("{name}:\n{}{value}", spaces(next.indent)))
}

/// A method chain: on one line within `chain_width`, otherwise one call per
/// line one level deeper than the head.
fn chain(head: &Syntax, calls: &[Syntax], shape: Shape) -> Option<String> {
    let head = head.rewrite(shape)?;
    let nested = shape.nested();
    let calls: Vec<String> = calls
        .iter()
        .map(|call| call.rewrite(nested))
        .collect::<Option<_>>()?;
    let one_line = format!("{head}{}", calls.concat());
    if !one_line.contains('\n') && one_line.len() <= shape.width.min(CHAIN_WIDTH) {
        return Some(one_line);
    }
    let mut result = head;
    for call in calls {
        result.push('\n');
        result.push_str(&spaces(nested.indent));
        result.push_str(&call);
    }
    Some(result)
}

/// A Rust byte string literal. Printable ASCII is written as itself apart from
/// `"` and `\`; other bytes use escapes, so any byte sequence round-trips.
pub(super) fn byte_string(value: &[u8]) -> String {
    let mut text = String::from("b\"");
    for byte in value {
        match byte {
            b'"' => text.push_str("\\\""),
            b'\\' => text.push_str("\\\\"),
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\t' => text.push_str("\\t"),
            0x20..=0x7e => text.push(char::from(*byte)),
            _ => text.push_str(&format!("\\x{byte:02x}")),
        }
    }
    text.push('"');
    text
}

fn spaces(count: usize) -> String {
    " ".repeat(count)
}

fn first_line(text: &str) -> &str {
    text.split('\n').next().unwrap_or("")
}

fn last_line_width(text: &str) -> usize {
    text.rsplit('\n').next().map_or(0, str::len)
}

fn newlines(text: &str) -> usize {
    text.matches('\n').count()
}

#[cfg(test)]
mod tests {
    //! Each expected layout is `rustfmt --edition 2024 --check` clean.

    use super::{Syntax, assignment, statement};

    fn rec(path: &str, fields: Vec<(&'static str, Syntax)>) -> Syntax {
        Syntax::record(path, fields)
    }

    fn lit(value: impl ToString) -> Syntax {
        Syntax::literal(value)
    }

    #[test]
    fn layouts_match_rustfmt() {
        let plan = rec(
            "c::DeliveryPlan",
            vec![("ordinal", lit(0)), ("channel", lit(300))],
        );
        let observe = |text: &str| {
            Syntax::call(
                "l::Op::ObserveWhen",
                vec![
                    lit(58),
                    Syntax::call("l::Observation::OutboxDestination", vec![lit(0)]),
                    Syntax::call("l::Atom::Text", vec![Syntax::bytes(text.as_bytes())]),
                ],
            )
        };
        let tuples = |n: usize, base: u32| {
            Syntax::slice(
                (0..n as u32)
                    .map(|i| {
                        Syntax::tuple(vec![
                            lit(base + i),
                            lit(base + 1000 + i),
                            lit(base + 2000 + i),
                        ])
                    })
                    .collect(),
            )
        };
        let cases: [(&str, Option<String>, &str); 11] = [
            (
                "single_struct_overflows",
                assignment(
                    "pub const OUTBOX: &[c::DeliveryPlan<'static>] =",
                    &Syntax::slice(vec![plan]),
                ),
                r#"pub const OUTBOX: &[c::DeliveryPlan<'static>] = &[c::DeliveryPlan {
    ordinal: 0,
    channel: 300,
}];"#,
            ),
            (
                "call_args_over_60_go_vertical",
                assignment(
                    "const LAW: &[l::Op<'static>] =",
                    &Syntax::slice(vec![observe("local-observer"), observe("local")]),
                ),
                r#"const LAW: &[l::Op<'static>] = &[
    l::Op::ObserveWhen(
        58,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"local-observer"),
    ),
    l::Op::ObserveWhen(
        58,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"local"),
    ),
];"#,
            ),
            (
                "struct_body_18_stays_on_one_line",
                assignment(
                    "const V: &[InputVariant] =",
                    &Syntax::slice(vec![
                        rec("InputVariant", vec![("id", lit(120)), ("code", lit(120))]),
                        rec("InputVariant", vec![("id", lit(1200)), ("code", lit(120))]),
                    ]),
                ),
                r#"const V: &[InputVariant] = &[
    InputVariant { id: 120, code: 120 },
    InputVariant {
        id: 1200,
        code: 120,
    },
];"#,
            ),
            (
                "short_numbers_fill_lines",
                assignment(
                    "pub const REQUIRED: &[u32] =",
                    &Syntax::slice((500..540).map(lit).collect()),
                ),
                r#"pub const REQUIRED: &[u32] = &[
    500, 501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511, 512, 513, 514, 515, 516, 517, 518,
    519, 520, 521, 522, 523, 524, 525, 526, 527, 528, 529, 530, 531, 532, 533, 534, 535, 536, 537,
    538, 539,
];"#,
            ),
            (
                "list_that_fits_the_next_line_moves_there",
                assignment(
                    "pub const CHANNEL_ROOTS: &[(u32, u32, u32)] =",
                    &tuples(3, 3000),
                ),
                r#"pub const CHANNEL_ROOTS: &[(u32, u32, u32)] =
    &[(3000, 4000, 5000), (3001, 4001, 5001), (3002, 4002, 5002)];"#,
            ),
            (
                "wide_tuples_go_vertical",
                assignment(
                    "pub const CHANNEL_ROOTS: &[(u32, u32, u32)] =",
                    &tuples(4, 3000),
                ),
                r#"pub const CHANNEL_ROOTS: &[(u32, u32, u32)] = &[
    (3000, 4000, 5000),
    (3001, 4001, 5001),
    (3002, 4002, 5002),
    (3003, 4003, 5003),
];"#,
            ),
            (
                "field_value_moves_below_its_name",
                assignment(
                    "const D: s::Definition<'static> =",
                    &rec(
                        "s::Definition",
                        vec![
                            ("id", lit(100)),
                            (
                                "kind",
                                Syntax::call(
                                    "s::Kind::Record",
                                    vec![Syntax::slice(vec![rec(
                                        "s::Field",
                                        vec![
                                            ("id", lit(110)),
                                            ("name", Syntax::bytes("n".repeat(84).as_bytes())),
                                            ("type_id", lit(105)),
                                        ],
                                    )])],
                                ),
                            ),
                        ],
                    ),
                ),
                r#"const D: s::Definition<'static> = s::Definition {
    id: 100,
    kind: s::Kind::Record(&[s::Field {
        id: 110,
        name:
            b"nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
        type_id: 105,
    }]),
};"#,
            ),
            (
                "nested_text_call_breaks_inside",
                assignment(
                    "const P: c::DeliveryPlan<'static> =",
                    &rec(
                        "c::DeliveryPlan",
                        vec![
                            ("ordinal", lit(0)),
                            (
                                "destination",
                                Syntax::call(
                                    "c::Expr::Constant",
                                    vec![Syntax::call(
                                        "c::Atom::Text",
                                        vec![Syntax::bytes("d".repeat(70).as_bytes())],
                                    )],
                                ),
                            ),
                        ],
                    ),
                ),
                r#"const P: c::DeliveryPlan<'static> = c::DeliveryPlan {
    ordinal: 0,
    destination: c::Expr::Constant(c::Atom::Text(
        b"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    )),
};"#,
            ),
            (
                "overlong_literal_runs_past_the_margin",
                assignment(
                    "const P: c::DeliveryPlan<'static> =",
                    &rec(
                        "c::DeliveryPlan",
                        vec![
                            ("ordinal", lit(0)),
                            (
                                "destination",
                                Syntax::call(
                                    "c::Expr::Constant",
                                    vec![Syntax::call(
                                        "c::Atom::Text",
                                        vec![Syntax::bytes("e".repeat(120).as_bytes())],
                                    )],
                                ),
                            ),
                        ],
                    ),
                ),
                r#"const P: c::DeliveryPlan<'static> = c::DeliveryPlan {
    ordinal: 0,
    destination: c::Expr::Constant(c::Atom::Text(
        b"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    )),
};"#,
            ),
            (
                "chain_goes_one_call_per_line",
                statement(
                    4,
                    &Syntax::chain(
                        Syntax::call("v2_zero_limits", Vec::new()),
                        vec![
                            Syntax::call(
                                ".with_limit",
                                vec![Syntax::path("Resource::Read"), lit(124)],
                            ),
                            Syntax::call(
                                ".with_limit",
                                vec![Syntax::path("Resource::Write"), lit(2)],
                            ),
                        ],
                    ),
                ),
                r#"v2_zero_limits()
        .with_limit(Resource::Read, 124)
        .with_limit(Resource::Write, 2)"#,
            ),
            (
                "empty_slice_and_paths",
                assignment(
                    "const B: c::Branch<'static> =",
                    &rec(
                        "c::Branch",
                        vec![
                            ("reason", Syntax::path("None")),
                            ("effects", Syntax::slice(Vec::new())),
                        ],
                    ),
                ),
                r#"const B: c::Branch<'static> = c::Branch {
    reason: None,
    effects: &[],
};"#,
            ),
        ];
        for (name, actual, expected) in cases {
            assert_eq!(actual.as_deref(), Some(expected), "{name}");
        }
    }
}
