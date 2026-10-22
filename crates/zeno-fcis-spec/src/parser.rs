//! Recovering parser for language version 1.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::ast::*;
use crate::collections::{
    CollectionsSpec, EntryDecl, KeyPaths, KeyTypeDecl, MapDecl, Scan, SummaryDecl, SummaryKind,
};
use crate::diagnostic::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, SourceSpan, text,
};
use crate::lexer::{Token, TokenKind, lex};
use crate::tables::{self, CountInfo, MAX_TABLE_ENTRIES, Rewrite, TableExpansion, TableInfo};
use crate::{MAX_FORMULA_DEPTH, MAX_FORMULA_NODES, SourceLimits, ZENO_DSL_VERSION};

/// Parses one bounded `.zeno` source file and accumulates deterministic diagnostics.
pub fn parse_project(source: &str, limits: SourceLimits) -> Result<ParsedProject, DiagnosticSet> {
    parse_with_tables(source, limits).map(|(parsed, _)| parsed)
}

/// Parses exactly one relational formula using the project's table and
/// collection declarations. Trailing tokens and undeclared projection paths
/// are refused. The returned tree owns its data and cannot add declarations.
pub fn parse_formula(
    project_source: &str,
    formula_source: &str,
    limits: SourceLimits,
) -> Result<RelExpr, DiagnosticSet> {
    let (parsed, mut parser) = parse_with_tables(project_source, limits)?;
    let project = crate::elaborate_project(parsed, crate::ProjectLimits::default())?;
    let (tokens, diagnostics) = lex(formula_source, limits);
    parser.tokens = tokens;
    parser.diagnostics = diagnostics;
    parser.index = 0;
    parser.nesting = 0;
    parser.expanded = 0;
    parser.table_values = 0;
    parser.rewrites.clear();
    parser.counts.clear();
    let formula = parser.parse_rel();
    if !matches!(parser.current().kind, TokenKind::Eof) {
        parser.error(
            DiagnosticCode::ExpectedToken,
            "formula",
            "end of one formula",
            parser.describe_current(),
            "supply one formula without a declaration or terminator",
        );
    }
    let (nodes, depth) = formula_shape_rel(&formula);
    if nodes > MAX_FORMULA_NODES || depth > MAX_FORMULA_DEPTH {
        parser.error(
            DiagnosticCode::ResourceLimit,
            "formula",
            "a bounded formula",
            format!("{nodes} nodes and depth {depth}"),
            "use a smaller formula",
        );
    }
    let law = LawDecl::new(project.project_id(), fallback_identifier(), formula);
    for path in crate::law_paths(&law) {
        if crate::resolve_path(&project, path).is_unresolved() {
            parser.error(
                DiagnosticCode::UnknownReference,
                "formula.path",
                "a declared projection",
                format!("{path:?}"),
                "use the project's declared fields",
            );
        }
    }
    if let Some(reason) = parser.key_paths.misuse(law.formula()) {
        parser.error(
            DiagnosticCode::InvalidDeclaration,
            "formula",
            "valid collection use",
            reason,
            "compare keys only for equality and read maps through entries",
        );
    }
    if parser.diagnostics.is_empty() {
        Ok(law.formula)
    } else {
        Err(DiagnosticSet::from_vec(
            parser.diagnostics,
            limits.max_diagnostics(),
        ))
    }
}

/// Parses one source file and returns the exact expansion of its table forms:
/// the source with each table declaration, index type and table formula
/// replaced by what it expands to, as [`TableExpansion`] describes.
pub fn expand_tables(source: &str, limits: SourceLimits) -> Result<TableExpansion, DiagnosticSet> {
    let (_, parser) = parse_with_tables(source, limits)?;
    let mut tables = parser.table_infos;
    tables.sort_by_key(|table| table.id);
    let counts = parser.counts.into_values().collect();
    match tables::apply(source, parser.rewrites) {
        Some(text) => Ok(TableExpansion {
            text,
            tables,
            counts,
        }),
        None => Err(DiagnosticSet::from_vec(
            vec![Diagnostic::new(
                DiagnosticCode::InvalidDeclaration,
                DiagnosticStage::Parse,
                AstPath::new("table.expansion"),
                SourceSpan::new(0, 0, 1, 1),
                "disjoint table expansions",
                "overlapping table expansions",
                "report this expansion failure",
            )],
            limits.max_diagnostics(),
        )),
    }
}

fn parse_with_tables(
    source: &str,
    limits: SourceLimits,
) -> Result<(ParsedProject, Parser), DiagnosticSet> {
    let (tokens, diagnostics) = lex(source, limits);
    let mut parser = Parser {
        tables: tables::prescan(&tokens),
        scan: Scan::read(&tokens),
        entry_bound: Vec::new(),
        drafts: Drafts::default(),
        key_paths: KeyPaths::default(),
        tokens,
        index: 0,
        diagnostics,
        declarations: Vec::new(),
        merge_order: Vec::new(),
        nesting: 0,
        declaration_start: SourceSpan::new(0, 0, 1, 1),
        bound: Vec::new(),
        table_depth: 0,
        table_values: 0,
        expanded: 0,
        rewrites: Vec::new(),
        table_infos: Vec::new(),
        table_spans: Vec::new(),
        counts: BTreeMap::new(),
    };
    let parsed = parser.parse(limits.max_diagnostics());
    if parser.diagnostics.is_empty() {
        Ok((parsed, parser))
    } else {
        Err(DiagnosticSet::from_vec(
            parser.diagnostics,
            limits.max_diagnostics(),
        ))
    }
}

/// Collection declarations as written, with their source extents, until the
/// whole file is parsed and their view can be emitted.
#[derive(Default)]
struct Drafts {
    keys: Vec<(StableId, Identifier, SourceSpan, u32)>,
    maps: Vec<(MapDecl, SourceSpan, u32)>,
    entries: Vec<(EntryDecl, SourceSpan, u32)>,
    summaries: Vec<(SummaryDecl, SourceSpan)>,
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
    declarations: Vec<SpannedDeclaration>,
    merge_order: Vec<StableId>,
    nesting: usize,
    /// The first token of the declaration being parsed.
    declaration_start: SourceSpan,
    /// Table sizes by ID, from every table declaration in the file.
    tables: BTreeMap<u32, u32>,
    /// Bound formula variables, innermost last: a table index's value, or
    /// `None` for a quantifier or sum variable, which shadows table indices.
    bound: Vec<(Box<str>, Option<i128>)>,
    /// Table forms being expanded; rewrites are recorded only outside them.
    table_depth: usize,
    /// Table value forms parsed so far, to find the atoms that contain one.
    table_values: usize,
    /// Expanded formula nodes, bounded by `MAX_FORMULA_NODES`.
    expanded: usize,
    rewrites: Vec<Rewrite>,
    table_infos: Vec<TableInfo>,
    /// Each table's ID, size and declaration span.
    table_spans: Vec<(u32, u32, SourceSpan)>,
    /// Count forms by source offset.
    counts: BTreeMap<u32, CountInfo>,
    /// Maps, entries and record fields declared anywhere in the file.
    scan: Scan,
    /// Bound entry variables, innermost last, with the entry each names.
    entry_bound: Vec<(Box<str>, u32)>,
    drafts: Drafts,
    /// Collection restrictions retained only for standalone formula parsing.
    key_paths: KeyPaths,
}

impl Parser {
    fn parse(&mut self, diagnostic_limit: usize) -> ParsedProject {
        let version = if self.take_keyword("zeno") {
            self.take_u16("header.version").unwrap_or(0)
        } else {
            self.error(
                DiagnosticCode::MissingHeader,
                "header",
                "zeno <version>;",
                "missing zeno header",
                "start the file with `zeno 1;`",
            );
            0
        };
        self.expect_symbol(TokenKind::Semicolon, "header.version");
        if version != ZENO_DSL_VERSION {
            self.error(
                DiagnosticCode::UnsupportedVersion,
                "header.version",
                text(ZENO_DSL_VERSION),
                text(version),
                "select the supported language version",
            );
        }
        let (project_id, name) = if self.take_keyword("project") {
            let id = self.take_id("project.id").unwrap_or_else(one_id);
            let name = self
                .take_identifier("project.name")
                .unwrap_or_else(fallback_identifier);
            self.expect_symbol(TokenKind::Semicolon, "project");
            (id, name)
        } else {
            self.error(
                DiagnosticCode::MissingHeader,
                "project",
                "project <id> <name>;",
                "missing project declaration",
                "add one explicit project declaration after the version header",
            );
            (one_id(), fallback_identifier())
        };
        while !matches!(self.current().kind, TokenKind::Eof) {
            let start = self.current().span;
            self.declaration_start = start;
            let result = if self.take_keyword("namespace") {
                self.parse_namespace().map(Declaration::Namespace)
            } else if self.take_keyword("type") {
                self.parse_type().map(Declaration::Type)
            } else if self.take_keyword("field") {
                self.parse_field().map(Declaration::Field)
            } else if self.take_keyword("table") {
                self.parse_table();
                None
            } else if self.take_keyword("map") {
                self.parse_map();
                None
            } else if self.take_keyword("entry") {
                self.parse_entry();
                None
            } else if self.take_keyword("summary") {
                self.parse_summary().map(Declaration::Field)
            } else if self.take_keyword("variant") {
                self.parse_variant().map(Declaration::Variant)
            } else if self.take_keyword("reason") {
                self.parse_reason().map(Declaration::Reason)
            } else if self.take_keyword("effect") {
                self.parse_effect().map(Declaration::Effect)
            } else if self.take_keyword("channel") {
                self.parse_channel().map(Declaration::Channel)
            } else if self.take_keyword("component") {
                self.parse_component().map(Declaration::Component)
            } else if self.take_keyword("wire") {
                self.parse_wiring().map(Declaration::Wiring)
            } else if self.take_keyword("merge") {
                self.parse_merge();
                None
            } else if self.take_keyword("random") {
                self.parse_random().map(Declaration::Random)
            } else if self.take_keyword("distribution") {
                self.parse_distribution().map(Declaration::Distribution)
            } else if self.take_keyword("reward") {
                self.parse_reward().map(Declaration::Reward)
            } else if self.take_keyword("law") {
                self.parse_law().map(Declaration::Law)
            } else if self.take_keyword("claim") {
                self.parse_claim().map(Declaration::Claim)
            } else {
                self.error(DiagnosticCode::UnexpectedToken, "project.declarations", "top-level declaration", self.describe_current(), "use a version-1 declaration keyword and terminate declarations with semicolons");
                self.recover_top();
                None
            };
            if let Some(declaration) = result {
                self.declarations.push(SpannedDeclaration {
                    declaration,
                    span: start,
                });
            }
        }
        self.refuse_table_collisions(project_id);
        let collections = self.finish_collections();
        ParsedProject {
            version,
            project_id,
            name,
            collections,
            declarations: core::mem::take(&mut self.declarations),
            merge_order: core::mem::take(&mut self.merge_order),
            diagnostic_limit,
        }
    }

    fn parse_namespace(&mut self) -> Option<NamespaceDecl> {
        let id = self.take_id("namespace.id")?;
        let name = self.take_identifier("namespace.name")?;
        self.expect_symbol(TokenKind::Semicolon, "namespace");
        Some(NamespaceDecl::new(id, name))
    }
    fn parse_type(&mut self) -> Option<TypeDecl> {
        let id = self.take_id("type.id")?;
        let kind_name = self.take_identifier("type.kind")?;
        if kind_name.as_str() == "index" {
            return self.parse_index_type(id);
        }
        if kind_name.as_str() == "key" {
            let name = self.take_identifier("type.name")?;
            self.expect_symbol(TokenKind::Semicolon, "type");
            let end = self.last_end();
            self.drafts
                .keys
                .push((id, name, self.declaration_start, end));
            return None;
        }
        let kind = match kind_name.as_str() {
            "state" => TypeKind::State,
            "command" => TypeKind::Command,
            "context" => TypeKind::Context,
            "effect" => TypeKind::Effect,
            "destination" => TypeKind::Destination,
            "payload" => TypeKind::Payload,
            "data" => TypeKind::Data,
            "bool" => TypeKind::Bool,
            "int" => TypeKind::Int,
            "u128" => TypeKind::U128,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "type.kind",
                    "state|command|context|effect|destination|payload|data|bool|int",
                    kind_name.as_str(),
                    "select one closed type kind",
                );
                TypeKind::Data
            }
        };
        let name = self.take_identifier("type.name")?;
        let declared = TypeDecl::new(id, kind, name);
        let declared = if self.take_keyword("in") {
            if kind == TypeKind::U128 {
                let min = self.take_unsigned("u128.min")?;
                self.expect_symbol(TokenKind::Range, "u128 range");
                self.expect_symbol(TokenKind::Equal, "u128 inclusive range");
                let max = self.take_unsigned("u128.max")?;
                declared.with_unsigned_range(min, max).or_else(|| {
                    self.error(
                        DiagnosticCode::InvalidDeclaration,
                        "u128.range",
                        "min <= max",
                        "reversed",
                        "use inclusive unsigned bounds",
                    );
                    None
                })?
            } else {
                self.parse_int_range(declared)
            }
        } else {
            declared
        };
        let declared = if self.take_keyword("unit") {
            self.parse_quantity(declared)?
        } else {
            declared
        };
        if kind == TypeKind::U128 && declared.unsigned_range().is_none() {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "u128.range",
                "in MIN..=MAX",
                "missing",
                "declare unsigned bounds",
            );
        }
        self.expect_symbol(TokenKind::Semicolon, "type");
        Some(declared)
    }
    fn take_unsigned(&mut self, context: &str) -> Option<u128> {
        if let TokenKind::Number(n) = self.current().kind {
            let span = self.current().span;
            self.advance();
            if span.end() - span.start() != n.to_string().len() as u32 {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    context,
                    "canonical unsigned decimal",
                    "leading zero",
                    "use canonical unsigned decimal",
                );
                return None;
            }
            Some(n)
        } else {
            self.error(
                DiagnosticCode::InvalidNumber,
                context,
                "unsigned decimal",
                "invalid",
                "use canonical unsigned decimal",
            );
            None
        }
    }
    fn parse_scale(&mut self) -> Option<u8> {
        let text = self.take_identifier("quantity.scale")?;
        let scale = text
            .as_str()
            .strip_prefix('e')
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|s| *s <= 38);
        if scale.is_none() {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "quantity.scale",
                "e0..e38",
                text.as_str(),
                "declare a literal decimal exponent",
            );
        }
        scale
    }
    fn parse_quantity(&mut self, declared: TypeDecl) -> Option<TypeDecl> {
        let mut factors = Vec::new();
        let mut sign = 1;
        loop {
            if self.current().kind == TokenKind::Number(1) {
                self.advance();
            } else {
                factors.push((self.take_identifier("quantity.unit")?, sign));
            }
            if self.take_symbol(&TokenKind::Star) {
                sign = 1;
            } else if self.take_symbol(&TokenKind::Slash) {
                sign = -1;
            } else {
                break;
            }
        }
        if !self.take_keyword("scale") {
            self.error(
                DiagnosticCode::ExpectedToken,
                "quantity.scale",
                "scale eP",
                self.describe_current(),
                "both unit and scale are required",
            );
            return None;
        }
        let quantity = Quantity::try_new(factors, self.parse_scale()?)?;
        let result = declared.with_quantity(quantity);
        if result.is_none() {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "quantity",
                "int",
                "non-integer",
                "only integer declarations carry units",
            );
        }
        result
    }
    /// Parses `MIN..=MAX` after `in` in a type declaration. The bounds are
    /// inclusive, as in the schema; `..` alone is refused, because quantifiers
    /// read it as half-open.
    fn parse_int_range(&mut self, declared: TypeDecl) -> TypeDecl {
        let Some(min) = self.take_i128("type.range.min") else {
            return declared;
        };
        self.expect_symbol(TokenKind::Range, "type.range");
        let inclusive = self.take_symbol(&TokenKind::Equal);
        if !inclusive {
            self.error(
                DiagnosticCode::ExpectedToken,
                "type.range",
                "`..=`",
                self.describe_current(),
                "type ranges are inclusive: write MIN..=MAX",
            );
        }
        let Some(max) = self.take_i128("type.range.max") else {
            return declared;
        };
        if !inclusive {
            return declared;
        }
        let Some(range) = IntRange::try_new(min, max) else {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "type.range",
                "MIN <= MAX",
                format!("{min}..={max}"),
                "write the least value first",
            );
            return declared;
        };
        match declared.clone().with_range(range) {
            Some(ranged) => ranged,
            None => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "type.range",
                    "int",
                    format!("{:?}", declared.kind()).to_lowercase(),
                    "declare a range only on an int type",
                );
                declared
            }
        }
    }
    /// Parses `type ID index Name of TABLE;`, the integer type `0..=N-1` of
    /// a table of N entries.
    fn parse_index_type(&mut self, id: StableId) -> Option<TypeDecl> {
        let name = self.take_identifier("type.name")?;
        self.expect_keyword("of", "type.index");
        let table = self.take_id("type.index.table")?;
        self.expect_symbol(TokenKind::Semicolon, "type");
        let Some(size) = self.tables.get(&table.get()).copied() else {
            self.error_at(
                self.declaration_start,
                DiagnosticCode::UnknownReference,
                "type.index.table",
                "a declared table",
                text(table.get()),
                "declare `table ID OWNER name TYPE size N;` or index a declared table",
            );
            return None;
        };
        let max = i128::from(size.max(1) - 1);
        let declared =
            TypeDecl::new(id, TypeKind::Int, name).with_range(IntRange::try_new(0, max)?)?;
        self.rewrite_declaration(format!(
            "type {} int {} in 0..={max};",
            id.get(),
            declared.name().as_str()
        ));
        Some(declared)
    }
    /// Parses `table ID OWNER name TYPE size N;` into the N fields
    /// `ID + k` named `name_k`, each declared at the table's span.
    fn parse_table(&mut self) {
        let start = self.declaration_start;
        let Some((id, owner, name, field_type, size)) = self.parse_table_header() else {
            self.recover_top();
            return;
        };
        if size == 0 || size > MAX_TABLE_ENTRIES {
            self.error_at(
                start,
                if size == 0 {
                    DiagnosticCode::InvalidDeclaration
                } else {
                    DiagnosticCode::ResourceLimit
                },
                "table.size",
                format!("1..={MAX_TABLE_ENTRIES} entries"),
                text(size),
                "declare between one and 64 entries",
            );
            return;
        }
        let mut lines = Vec::new();
        for k in 0..size {
            let entry = id.get().checked_add(k).and_then(StableId::new);
            let entry_name = Identifier::try_new(format!("{}_{k}", name.as_str()));
            let (Some(entry), Some(entry_name)) = (entry, entry_name) else {
                self.error_at(
                    start,
                    DiagnosticCode::InvalidDeclaration,
                    "table.entry",
                    "entry IDs within u32 and entry names of at most 128 bytes",
                    format!("entry {k}"),
                    "choose a smaller table ID or a shorter name",
                );
                return;
            };
            lines.push(format!(
                "field {} {} {} {};",
                entry.get(),
                owner.get(),
                entry_name.as_str(),
                field_type.get()
            ));
            self.declarations.push(SpannedDeclaration {
                declaration: Declaration::Field(FieldDecl::new(
                    entry, owner, entry_name, field_type,
                )),
                span: start,
            });
        }
        self.rewrite_declaration(lines.join("\n"));
        self.table_spans.push((id.get(), size, start));
        self.table_infos.push(TableInfo {
            id: id.get(),
            owner: owner.get(),
            name: name.as_str().into(),
            field_type: field_type.get(),
            size,
        });
    }
    fn parse_table_header(&mut self) -> Option<(StableId, StableId, Identifier, StableId, u32)> {
        let id = self.take_id("table.id")?;
        let owner = self.take_id("table.owner")?;
        let name = self.take_identifier("table.name")?;
        let field_type = self.take_id("table.type")?;
        self.expect_keyword("size", "table.size");
        let size = self.take_u32("table.size")?;
        self.expect_symbol(TokenKind::Semicolon, "table");
        Some((id, owner, name, field_type, size))
    }
    /// Validates the collection declarations, emits their view declarations
    /// and rewrites, checks how formulas use keys and maps, and returns the
    /// collections metadata. See [`crate::CollectionsSpec`].
    fn finish_collections(&mut self) -> CollectionsSpec {
        let drafts = core::mem::take(&mut self.drafts);
        if drafts.keys.is_empty()
            && drafts.maps.is_empty()
            && drafts.entries.is_empty()
            && drafts.summaries.is_empty()
        {
            return CollectionsSpec::default();
        }
        let mut kinds: BTreeMap<u32, (TypeKind, Option<IntRange>)> = BTreeMap::new();
        let mut fields: BTreeMap<u32, Vec<(StableId, Identifier, StableId, SourceSpan)>> =
            BTreeMap::new();
        let mut variants: BTreeMap<u32, Vec<bool>> = BTreeMap::new();
        for spanned in &self.declarations {
            match &spanned.declaration {
                Declaration::Type(t) => {
                    kinds.insert(t.id().get(), (t.kind(), t.range()));
                }
                Declaration::Field(f) => fields.entry(f.owner().get()).or_default().push((
                    f.id(),
                    f.name().clone(),
                    f.field_type(),
                    spanned.span,
                )),
                Declaration::Variant(v) => variants
                    .entry(v.owner().get())
                    .or_default()
                    .push(v.payload_type().is_some()),
                _ => {}
            }
        }
        for list in fields.values_mut() {
            list.sort_by_key(|(id, ..)| *id);
        }
        let key_types: BTreeSet<u32> = drafts.keys.iter().map(|(id, ..)| id.get()).collect();
        let root = |kind: TypeKind| {
            kinds
                .iter()
                .find(|(_, (k, _))| *k == kind)
                .map(|(id, _)| *id)
        };
        let (state, command, context) = (
            root(TypeKind::State),
            root(TypeKind::Command),
            root(TypeKind::Context),
        );
        let scalar = |type_id: u32| {
            key_types.contains(&type_id)
                || match kinds.get(&type_id) {
                    Some((TypeKind::Int, Some(_)) | (TypeKind::Bool, _)) => true,
                    Some((TypeKind::Data, _)) => variants
                        .get(&type_id)
                        .is_some_and(|v| v.iter().all(|payload| !payload)),
                    _ => false,
                }
        };
        let mut problems: Vec<(SourceSpan, &'static str, &'static str, String)> = Vec::new();
        let mut emitted: Vec<(Declaration, SourceSpan)> = Vec::new();
        let mut key_paths = KeyPaths::default();

        // Maps: their presence type is the map's ID.
        let mut maps = Vec::new();
        for (mut map, span, end) in drafts.maps {
            let record = map.value_type.get();
            let value = fields.get(&record).cloned().unwrap_or_default();
            let refusal = if Some(map.owner.get()) != state {
                Some(("map.owner", "the state record"))
            } else if !key_types.contains(&map.key_type.get()) {
                Some(("map.key", "a key type"))
            } else if !matches!(kinds.get(&record), Some((TypeKind::Data, _)))
                || variants.contains_key(&record)
            {
                Some(("map.value", "a record type"))
            } else if value.iter().any(|(_, _, t, _)| !scalar(t.get())) {
                Some((
                    "map.value",
                    "record fields of ranged int, bool, enumerated or key types",
                ))
            } else if map.capacity == 0 {
                Some(("map.capacity", "at least one entry"))
            } else {
                None
            };
            let present = Identifier::try_new(format!("{}_present", map.name.as_str()));
            let presence = present.clone().and_then(|name| {
                TypeDecl::new(map.id, TypeKind::Int, name).with_range(IntRange::try_new(0, 1)?)
            });
            match (refusal, present, presence) {
                (None, Some(present), Some(presence)) => {
                    map.value_fields = value.iter().map(|(id, _, t, _)| (*id, *t)).collect();
                    emitted.push((Declaration::Type(presence), span));
                    self.rewrites.push(Rewrite {
                        start: span.start(),
                        end,
                        text: format!("type {} int {} in 0..=1;", map.id.get(), present.as_str()),
                    });
                    key_paths.maps.insert((map.owner.get(), map.id.get()));
                    maps.push(map);
                }
                (refusal, ..) => {
                    let (path, expected) =
                        refusal.unwrap_or(("map.name", "a name of at most 120 bytes"));
                    problems.push((span, path, expected, text(map.id.get()).into()));
                }
            }
        }

        // Entries: presence field, then one field per value field.
        let mut entries = Vec::new();
        let mut stored: BTreeMap<u32, u32> = BTreeMap::new();
        for (entry, span, end) in drafts.entries {
            let Some(map) = maps.iter().find(|m| m.id == entry.map) else {
                problems.push((
                    span,
                    "entry.map",
                    "a declared map",
                    text(entry.map.get()).into(),
                ));
                continue;
            };
            let segments = entry.key.segments();
            let owner = match entry.key.root() {
                ProjectionRoot::Command => command,
                ProjectionRoot::Context => context,
                _ => None,
            };
            let key_type = (segments.len() == 2 && Some(segments[0].get()) == owner)
                .then(|| fields.get(&segments[0].get()))
                .flatten()
                .and_then(|list| list.iter().find(|(id, ..)| *id == segments[1]))
                .map(|(_, _, t, _)| *t);
            if key_type != Some(map.key_type) {
                problems.push((
                    span,
                    "entry.key",
                    "a command or context field of the map's key type",
                    text(entry.id.get()).into(),
                ));
                continue;
            }
            let record = fields
                .get(&map.value_type.get())
                .cloned()
                .unwrap_or_default();
            let mut lines = vec![format!(
                "field {} {} {} {};",
                entry.id.get(),
                map.owner.get(),
                entry.name.as_str(),
                map.id.get()
            )];
            let mut views = vec![(entry.id, entry.name.clone(), map.id)];
            for (j, (_, field_name, field_type, _)) in record.iter().enumerate() {
                let view = u32::try_from(j + 1)
                    .ok()
                    .and_then(|j| entry.id.get().checked_add(j))
                    .and_then(StableId::new);
                let name =
                    Identifier::try_new(format!("{}_{}", entry.name.as_str(), field_name.as_str()));
                let (Some(view), Some(name)) = (view, name) else {
                    problems.push((
                        span,
                        "entry.id",
                        "view field IDs within u32 and names of at most 128 bytes",
                        text(entry.id.get()).into(),
                    ));
                    continue;
                };
                if key_types.contains(&field_type.get()) {
                    key_paths.keys.insert(
                        (ProjectionRoot::Pre, vec![map.owner.get(), view.get()]),
                        field_type.get(),
                    );
                    *stored.entry(field_type.get()).or_default() += 1;
                }
                lines.push(format!(
                    "field {} {} {} {};",
                    view.get(),
                    map.owner.get(),
                    name.as_str(),
                    field_type.get()
                ));
                views.push((view, name, *field_type));
            }
            for (id, name, field_type) in views {
                emitted.push((
                    Declaration::Field(FieldDecl::new(id, map.owner, name, field_type)),
                    span,
                ));
            }
            self.rewrites.push(Rewrite {
                start: span.start(),
                end,
                text: lines.join("\n"),
            });
            entries.push(entry);
        }

        // Key fields live only in the command, the context and map records.
        let value_records: BTreeSet<u32> = maps.iter().map(|m| m.value_type.get()).collect();
        let mut inputs: BTreeMap<u32, u32> = BTreeMap::new();
        for (owner, list) in &fields {
            for (id, _, field_type, span) in list {
                if !key_types.contains(&field_type.get()) {
                    continue;
                }
                let input = if Some(*owner) == command {
                    Some(ProjectionRoot::Command)
                } else if Some(*owner) == context {
                    Some(ProjectionRoot::Context)
                } else {
                    None
                };
                match input {
                    Some(root) => {
                        key_paths
                            .keys
                            .insert((root, vec![*owner, id.get()]), field_type.get());
                        *inputs.entry(field_type.get()).or_default() += 1;
                    }
                    None if value_records.contains(owner) => {}
                    None => problems.push((
                        *span,
                        "field.type",
                        "a key held by the command, the context or a map's record",
                        text(id.get()).into(),
                    )),
                }
            }
        }

        // Key types: one class per key position of the type.
        let mut keys = Vec::new();
        for (id, name, span, end) in drafts.keys {
            let positions = inputs.get(&id.get()).copied().unwrap_or(0)
                + stored.get(&id.get()).copied().unwrap_or(0);
            let max = i128::from(positions.max(1) - 1);
            if let Some(view) = IntRange::try_new(0, max)
                .and_then(|range| TypeDecl::new(id, TypeKind::Int, name.clone()).with_range(range))
            {
                emitted.push((Declaration::Type(view), span));
                self.rewrites.push(Rewrite {
                    start: span.start(),
                    end,
                    text: format!("type {} int {} in 0..={max};", id.get(), name.as_str()),
                });
            }
            keys.push(KeyTypeDecl {
                id,
                name,
                positions,
            });
        }

        // Summaries: a ranged integer field over a declared map.
        let mut summaries = Vec::new();
        for (summary, span) in drafts.summaries {
            let map = maps.iter().find(|m| m.id == summary.map);
            let ranged = |type_id: StableId| {
                matches!(kinds.get(&type_id.get()), Some((TypeKind::Int, Some(_))))
            };
            let refusal = match (map, summary.kind) {
                (None, _) => Some(("summary.map", "a declared map")),
                (Some(map), _) if map.owner != summary.owner => {
                    Some(("summary.owner", "the record that holds the map"))
                }
                (Some(map), SummaryKind::Sum(field))
                    if !map.value_fields.iter().any(|(id, t)| {
                        *id == field && ranged(*t) && !key_types.contains(&t.get())
                    }) =>
                {
                    Some(("summary.field", "a ranged int field of the map's record"))
                }
                _ if !ranged(summary.value_type) => Some(("summary.type", "a ranged int type")),
                _ => None,
            };
            match refusal {
                Some((path, expected)) => {
                    problems.push((span, path, expected, text(summary.id.get()).into()))
                }
                None => summaries.push(summary),
            }
        }

        // A state law, which the checker assumes on every pre-view, must stay
        // true while unnamed entries keep their values: each conjunct reads
        // one named entry, or only scalar state.
        let entry_of: BTreeMap<u32, u32> = entries
            .iter()
            .flat_map(|entry| {
                let width = maps
                    .iter()
                    .find(|m| m.id == entry.map)
                    .map_or(0, |m| m.value_fields.len() as u32);
                (0..=width).map(move |j| (entry.id.get() + j, entry.id.get()))
            })
            .collect();
        let state_owner = state.unwrap_or(0);
        for spanned in &self.declarations {
            let Declaration::Law(law) = &spanned.declaration else {
                continue;
            };
            if !law.applicability().is_some_and(|a| a.genesis()) {
                continue;
            }
            let mut conjuncts = Vec::new();
            crate::collections::conjuncts(law.formula(), &mut conjuncts);
            let mixed = conjuncts.iter().any(|conjunct| {
                let mut reads = BTreeSet::new();
                crate::collections::state_reads(conjunct, state_owner, &mut reads);
                let named: BTreeSet<u32> = reads
                    .iter()
                    .filter_map(|f| entry_of.get(f))
                    .copied()
                    .collect();
                named.len() > 1
                    || (named.len() == 1 && reads.iter().any(|f| !entry_of.contains_key(f)))
            });
            if !entry_of.is_empty() && mixed {
                problems.push((
                    spanned.span,
                    "law.entries",
                    "a state law whose conjuncts each read one named entry or only scalar state",
                    text(law.id().get()).into(),
                ));
            }
        }
        // Formulas compare keys only with keys, by equality, and read maps
        // only through named entries.
        for spanned in &self.declarations {
            let mut formulas = Vec::new();
            match &spanned.declaration {
                Declaration::Law(law) => formulas.push(law.formula()),
                Declaration::Claim(claim) => match claim.formula() {
                    ClaimFormula::Relational(formula) => formulas.push(formula),
                    ClaimFormula::Temporal(formula) => temporal_atoms(formula, &mut formulas),
                },
                _ => {}
            }
            if let Some(message) = formulas.iter().find_map(|f| key_paths.misuse(f)) {
                problems.push((spanned.span, "formula.key", message, "refused".into()));
            }
        }
        for (span, path, expected, actual) in problems {
            self.error_at(
                span,
                DiagnosticCode::InvalidDeclaration,
                path,
                expected,
                actual,
                "see the contract collections reference for the allowed forms",
            );
        }
        for (declaration, span) in emitted {
            self.declarations
                .push(SpannedDeclaration { declaration, span });
        }
        self.key_paths = key_paths;
        keys.sort_by_key(|key| key.id);
        maps.sort_by_key(|map| map.id);
        entries.sort_by_key(|entry| entry.id);
        summaries.sort_by_key(|summary| summary.id);
        CollectionsSpec {
            keys: keys.into_boxed_slice(),
            maps: maps.into_boxed_slice(),
            entries: entries.into_boxed_slice(),
            summaries: summaries.into_boxed_slice(),
        }
    }
    /// Refuses a table whose entry IDs, `ID` to `ID + N - 1`, meet the ID of
    /// any other declaration, of any kind, or of the project.
    fn refuse_table_collisions(&mut self, project: StableId) {
        let mut owners = vec![(project.get(), "project", None)];
        for spanned in &self.declarations {
            for (id, kind) in declaration_ids(&spanned.declaration) {
                owners.push((id, kind, Some(spanned.span)));
            }
        }
        for (table, size, span) in self.table_spans.clone() {
            let entries = u64::from(table)..u64::from(table) + u64::from(size);
            let clash = owners.iter().find(|(id, kind, at)| {
                entries.contains(&u64::from(*id)) && !(*kind == "field" && *at == Some(span))
            });
            if let Some((id, kind, _)) = clash {
                self.error_at(
                    span,
                    DiagnosticCode::DuplicateId,
                    "table.id",
                    format!(
                        "entry IDs {table}..={} that no other declaration uses",
                        entries.end - 1
                    ),
                    format!("{kind} {id}"),
                    "choose a table ID whose entry IDs are all free",
                );
            }
        }
    }
    /// The end offset of the last token consumed.
    fn last_end(&self) -> u32 {
        self.tokens[self.index.saturating_sub(1)].span.end()
    }
    /// Parses `map ID OWNER name KEY VALUE capacity N;`. The map has no view
    /// field; its presence type is emitted once the file is parsed.
    fn parse_map(&mut self) {
        let start = self.declaration_start;
        let Some(map) = self.parse_map_header() else {
            self.recover_top();
            return;
        };
        let end = self.last_end();
        self.drafts.maps.push((map, start, end));
    }
    fn parse_map_header(&mut self) -> Option<MapDecl> {
        let id = self.take_id("map.id")?;
        let owner = self.take_id("map.owner")?;
        let name = self.take_identifier("map.name")?;
        let key_type = self.take_id("map.key")?;
        let value_type = self.take_id("map.value")?;
        self.expect_keyword("capacity", "map.capacity");
        let capacity = self.take_number("map.capacity")?;
        self.expect_symbol(TokenKind::Semicolon, "map");
        Some(MapDecl {
            id,
            owner,
            name,
            key_type,
            value_type,
            capacity,
            value_fields: Box::new([]),
        })
    }
    /// Parses `entry ID name of MAP at ROOT.TYPE.FIELD;`.
    fn parse_entry(&mut self) {
        let start = self.declaration_start;
        let Some(entry) = self.parse_entry_header() else {
            self.recover_top();
            return;
        };
        let end = self.last_end();
        self.drafts.entries.push((entry, start, end));
    }
    fn parse_entry_header(&mut self) -> Option<EntryDecl> {
        let id = self.take_id("entry.id")?;
        let name = self.take_identifier("entry.name")?;
        self.expect_keyword("of", "entry.map");
        let map = self.take_id("entry.map")?;
        self.expect_keyword("at", "entry.key");
        let ValueExpr::Projection(key) = self.parse_value() else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "entry.key",
                "a command or context key field",
                self.describe_current(),
                "write ROOT.TYPE.FIELD",
            );
            return None;
        };
        self.expect_symbol(TokenKind::Semicolon, "entry");
        Some(EntryDecl { id, name, map, key })
    }
    /// Parses `summary ID OWNER name TYPE sum MAP.FIELD;` or
    /// `summary ID OWNER name TYPE count MAP;`: an ordinary state field whose
    /// meaning the collections checker binds.
    fn parse_summary(&mut self) -> Option<FieldDecl> {
        let start = self.declaration_start;
        let id = self.take_id("summary.id")?;
        let owner = self.take_id("summary.owner")?;
        let name = self.take_identifier("summary.name")?;
        let value_type = self.take_id("summary.type")?;
        let (map, kind) = if self.take_keyword("count") {
            (self.take_id("summary.map")?, SummaryKind::Count)
        } else {
            self.expect_keyword("sum", "summary.kind");
            let map = self.take_id("summary.map")?;
            self.expect_symbol(TokenKind::Dot, "summary.field");
            (map, SummaryKind::Sum(self.take_id("summary.field")?))
        };
        self.expect_symbol(TokenKind::Semicolon, "summary");
        self.rewrite_declaration(format!(
            "field {} {} {} {};",
            id.get(),
            owner.get(),
            name.as_str(),
            value_type.get()
        ));
        self.drafts.summaries.push((
            SummaryDecl {
                id,
                owner,
                name: name.clone(),
                value_type,
                map,
                kind,
            },
            start,
        ));
        Some(FieldDecl::new(id, owner, name, value_type))
    }
    /// The entry a bound entry variable names.
    fn entry_variable(&self, name: &str) -> Option<u32> {
        self.entry_bound
            .iter()
            .rev()
            .find(|(bound, _)| bound.as_ref() == name)
            .map(|(_, entry)| *entry)
    }
    /// `ROOT.OWNER.ENTRY.FIELD` is the entry's view copy of a value field.
    fn entry_path(&mut self, start: SourceSpan, segments: &mut Vec<StableId>) {
        if segments.len() != 3 || !self.scan.entries.contains_key(&segments[1].get()) {
            return;
        }
        match self
            .scan
            .entry_field(segments[1].get(), segments[2].get())
            .and_then(StableId::new)
        {
            Some(view) => {
                segments.pop();
                segments[1] = view;
                self.table_values += 1;
            }
            None => self.error_at(
                start,
                DiagnosticCode::InvalidDeclaration,
                "entry.field",
                "a field of the map's record",
                text(segments[2].get()),
                "name a field of the record type the map declares",
            ),
        }
    }
    /// True when `IDENT in entries` starts `offset` tokens ahead.
    fn entries_form_ahead(&self, offset: usize) -> bool {
        let word = |at: usize, expected: Option<&str>| {
            matches!(self.tokens.get(self.index + offset + at).map(|t| &t.kind),
                Some(TokenKind::Ident(value)) if expected.is_none_or(|e| value.as_ref() == e))
        };
        word(0, None) && word(1, Some("in")) && word(2, Some("entries"))
    }
    /// Skips to the end of the current declaration, after a refused form.
    fn skip_to_declaration_end(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.current().kind {
                TokenKind::Eof => return,
                TokenKind::Semicolon if depth == 0 => return,
                TokenKind::LBrace | TokenKind::LParen => depth += 1,
                TokenKind::RBrace | TokenKind::RParen => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.advance();
        }
    }
    /// A `count` or `sum` over a map's entries: a map's aggregate is a
    /// declared summary, whose every change the checker binds.
    fn refuse_entries_aggregate(&mut self, start: SourceSpan) {
        self.error_at(
            start,
            DiagnosticCode::InvalidDeclaration,
            "entries.aggregate",
            "a declared summary",
            "an aggregate over a map's entries",
            "declare `summary ID OWNER name TYPE sum MAP.FIELD;` or `... count MAP;` and read that field",
        );
        self.skip_to_declaration_end();
    }
    /// `forall|exists VAR in entries MAP [where C] { F }`: F at each named
    /// entry of MAP, joined as the table forms are.
    fn parse_entries_formula(&mut self) -> RelExpr {
        let start = self.current().span;
        let keyword = self
            .take_identifier("entries.form")
            .unwrap_or_else(fallback_identifier);
        if keyword.as_str() == "count" {
            self.refuse_entries_aggregate(start);
            return RelExpr::Bool(false);
        }
        let Some(variable) = self.take_identifier("entries.variable") else {
            return RelExpr::Bool(false);
        };
        self.expect_keyword("in", "entries.variable");
        self.expect_keyword("entries", "entries.variable");
        let Some(map) = self.take_id("entries.map") else {
            return RelExpr::Bool(false);
        };
        let entries = self.scan.entries_of(map.get());
        if entries.is_empty() {
            self.error_at(
                start,
                DiagnosticCode::UnknownReference,
                "entries.map",
                "a map with named entries",
                text(map.get()),
                "declare `entry ID name of MAP at ROOT.TYPE.FIELD;` for this map",
            );
            self.skip_to_declaration_end();
            return RelExpr::Bool(false);
        }
        let universal = keyword.as_str() == "forall";
        self.table_depth += 1;
        let body_start = self.index;
        let mut parts = Vec::new();
        for entry in entries {
            self.index = body_start;
            self.entry_bound.push((variable.as_str().into(), entry));
            let condition = self
                .take_keyword("where")
                .then(|| self.nested("entries.where", RelExpr::Bool(false), |p| p.parse_rel()));
            self.expect_symbol(TokenKind::LBrace, "entries.body");
            let body = self.nested("entries.body", RelExpr::Bool(false), |p| p.parse_rel());
            self.expect_symbol(TokenKind::RBrace, "entries.body");
            self.entry_bound.pop();
            parts.push(match (condition, universal) {
                (Some(c), true) => RelExpr::Implies(Box::new(c), Box::new(body)),
                (Some(c), false) => RelExpr::And(Box::new(c), Box::new(body)),
                (None, _) => body,
            });
        }
        let nodes = parts.iter().map(|part| formula_shape_rel(part).0).sum();
        let formula = if self.charge(start, nodes) {
            tables::chain(
                parts,
                if universal { RelExpr::And } else { RelExpr::Or },
                universal,
            )
        } else {
            RelExpr::Bool(false)
        };
        self.table_depth -= 1;
        if self.table_depth == 0 {
            self.rewrite_from(start, format!("({})", tables::rel_text(&formula)));
        }
        formula
    }
    /// Records that the declaration just parsed expands to `text`.
    fn rewrite_declaration(&mut self, text: String) {
        let start = self.declaration_start;
        self.rewrite_from(start, text);
    }
    /// Records a rewrite from `start` to the last token consumed.
    fn rewrite_from(&mut self, start: SourceSpan, text: String) {
        let end = self.tokens[self.index.saturating_sub(1)].span.end();
        self.rewrites.push(Rewrite {
            start: start.start(),
            end,
            text,
        });
    }
    fn parse_field(&mut self) -> Option<FieldDecl> {
        let id = self.take_id("field.id")?;
        let owner = self.take_id("field.owner")?;
        let name = self.take_identifier("field.name")?;
        let field_type = self.take_id("field.type")?;
        self.expect_symbol(TokenKind::Semicolon, "field");
        Some(FieldDecl::new(id, owner, name, field_type))
    }
    fn parse_variant(&mut self) -> Option<VariantDecl> {
        let id = self.take_id("variant.id")?;
        let owner = self.take_id("variant.owner")?;
        let name = self.take_identifier("variant.name")?;
        let payload_type = if self.take_keyword("none") {
            None
        } else {
            self.take_id("variant.payload")
        };
        self.expect_symbol(TokenKind::Semicolon, "variant");
        Some(VariantDecl::new(id, owner, name, payload_type))
    }
    fn parse_reason(&mut self) -> Option<ReasonDecl> {
        let id = self.take_id("reason.id")?;
        let name = self.take_identifier("reason.name")?;
        self.expect_keyword("precedence", "reason.precedence");
        let rank = self.take_u32("reason.precedence")?;
        self.expect_symbol(TokenKind::Semicolon, "reason");
        Some(ReasonDecl::new(id, name, rank))
    }
    fn parse_effect(&mut self) -> Option<EffectDecl> {
        let id = self.take_id("effect.id")?;
        let name = self.take_identifier("effect.name")?;
        self.expect_keyword("destination", "effect.destination");
        let destination = self.take_id("effect.destination")?;
        self.expect_keyword("payload", "effect.payload");
        let payload = self.take_id("effect.payload")?;
        self.expect_symbol(TokenKind::Semicolon, "effect");
        Some(EffectDecl::new(id, name, destination, payload))
    }
    fn parse_channel(&mut self) -> Option<ChannelDecl> {
        let id = self.take_id("channel.id")?;
        let name = self.take_identifier("channel.name")?;
        self.expect_keyword("destination", "channel.destination");
        let destination = self.take_id("channel.destination")?;
        self.expect_keyword("payload", "channel.payload");
        let payload = self.take_id("channel.payload")?;
        self.expect_symbol(TokenKind::Semicolon, "channel");
        Some(ChannelDecl::new(id, name, destination, payload))
    }

    fn parse_component(&mut self) -> Option<ComponentDecl> {
        let id = self.take_id("component.id")?;
        let name = self.take_identifier("component.name")?;
        self.expect_symbol(TokenKind::LBrace, "component");
        let (mut owned, mut ports, mut footprints, mut budgets, mut assumptions, mut guarantees) = (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof) {
            if self.take_keyword("owns") {
                if let Some(value) = self.take_id("component.owns") {
                    owned.push(value);
                }
                self.expect_symbol(TokenKind::Semicolon, "component.owns");
            } else if self.take_keyword("port") {
                let port_id = self.take_id("component.port.id")?;
                let direction = if self.take_keyword("input") {
                    PortDirection::Input
                } else if self.take_keyword("output") {
                    PortDirection::Output
                } else {
                    self.error(
                        DiagnosticCode::ExpectedToken,
                        "component.port.direction",
                        "input|output",
                        self.describe_current(),
                        "declare an explicit direction",
                    );
                    PortDirection::Input
                };
                let port_name = self.take_identifier("component.port.name")?;
                let payload = self.take_id("component.port.type")?;
                self.expect_symbol(TokenKind::Semicolon, "component.port");
                ports.push(PortDecl::new(port_id, direction, port_name, payload));
            } else if let Some(kind) = self.take_footprint_kind() {
                if let Some(path) = self.parse_path() {
                    footprints.push(FootprintDecl::new(kind, path));
                }
                self.expect_symbol(TokenKind::Semicolon, "component.footprint");
            } else if self.take_keyword("budget") {
                let resource_name = self.take_identifier("component.budget.resource")?;
                let resource = match resource_name.as_str() {
                    "steps" => BudgetResource::Steps,
                    "nodes" => BudgetResource::Nodes,
                    "bytes" => BudgetResource::Bytes,
                    "predicate_calls" => BudgetResource::PredicateCalls,
                    _ => {
                        self.error(
                            DiagnosticCode::InvalidDeclaration,
                            "component.budget.resource",
                            "steps|nodes|bytes|predicate_calls",
                            resource_name.as_str(),
                            "select one closed resource kind",
                        );
                        BudgetResource::Steps
                    }
                };
                let limit = self.take_number("component.budget.limit")?;
                if let Some(budget) = BudgetDecl::try_new(resource, limit) {
                    budgets.push(budget)
                } else {
                    self.error(
                        DiagnosticCode::InvalidDeclaration,
                        "component.budget.limit",
                        "nonzero u64",
                        text(limit),
                        "use a nonzero resource limit",
                    )
                }
                self.expect_symbol(TokenKind::Semicolon, "component.budget");
            } else if self.take_keyword("assume") {
                if let Some(value) = self.take_id("component.assume") {
                    assumptions.push(value)
                }
                self.expect_symbol(TokenKind::Semicolon, "component.assume");
            } else if self.take_keyword("guarantee") {
                if let Some(value) = self.take_id("component.guarantee") {
                    guarantees.push(value)
                }
                self.expect_symbol(TokenKind::Semicolon, "component.guarantee");
            } else {
                self.error(
                    DiagnosticCode::UnexpectedToken,
                    "component",
                    "component declaration",
                    self.describe_current(),
                    "use owns, port, a footprint, budget, assume, or guarantee",
                );
                self.recover_member();
            }
        }
        self.expect_symbol(TokenKind::RBrace, "component");
        Some(ComponentDecl::new(
            id,
            name,
            owned,
            ports,
            footprints,
            budgets,
            assumptions,
            guarantees,
        ))
    }

    fn take_footprint_kind(&mut self) -> Option<FootprintKind> {
        if self.take_keyword("reads") {
            Some(FootprintKind::Read)
        } else if self.take_keyword("writes") {
            Some(FootprintKind::Write)
        } else if self.take_keyword("contexts") {
            Some(FootprintKind::Context)
        } else if self.take_keyword("effects") {
            Some(FootprintKind::Effect)
        } else if self.take_keyword("outbox") {
            Some(FootprintKind::Outbox)
        } else {
            None
        }
    }
    fn parse_path(&mut self) -> Option<ProjectionPath> {
        let root_name = self.take_identifier("path.root")?;
        let root = match root_name.as_str() {
            "pre" => ProjectionRoot::Pre,
            "post" => ProjectionRoot::Post,
            "command" => ProjectionRoot::Command,
            "context" => ProjectionRoot::Context,
            "effects" => ProjectionRoot::Effects,
            "outbox" => ProjectionRoot::Outbox,
            "events" => ProjectionRoot::Events,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "path.root",
                    "pre|post|command|context|effects|outbox|events",
                    root_name.as_str(),
                    "select one typed projection root",
                );
                ProjectionRoot::Pre
            }
        };
        let mut segments = Vec::new();
        while self.take_symbol(&TokenKind::Dot) {
            if let Some(id) = self.take_id("path.segment") {
                segments.push(id)
            } else {
                break;
            }
        }
        match ProjectionPath::try_new(root, segments) {
            Some(path) => Some(path),
            None => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "path",
                    "nonempty path with at most 64 stable IDs",
                    root_name.as_str(),
                    "append one or more `.ID` segments",
                );
                None
            }
        }
    }
    fn parse_wiring(&mut self) -> Option<WiringDecl> {
        let sc = self.take_id("wire.source.component")?;
        self.expect_symbol(TokenKind::Dot, "wire.source");
        let sp = self.take_id("wire.source.port")?;
        self.expect_symbol(TokenKind::Arrow, "wire");
        let dc = self.take_id("wire.destination.component")?;
        self.expect_symbol(TokenKind::Dot, "wire.destination");
        let dp = self.take_id("wire.destination.port")?;
        self.expect_symbol(TokenKind::Semicolon, "wire");
        Some(WiringDecl::new(sc, sp, dc, dp))
    }
    fn parse_merge(&mut self) {
        self.expect_symbol(TokenKind::LBracket, "merge");
        let mut values = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBracket | TokenKind::Eof) {
            if let Some(id) = self.take_id("merge.component") {
                values.push(id)
            }
            if !self.take_symbol(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBracket, "merge");
        self.expect_symbol(TokenKind::Semicolon, "merge");
        self.merge_order = values;
    }
    fn distribution_path(&mut self) -> Option<ProjectionPath> {
        match self.parse_value() {
            ValueExpr::Projection(path) => Some(path),
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "distribution.path",
                    "scalar projection",
                    "expression",
                    "write a root and numeric type/field path",
                );
                None
            }
        }
    }

    fn parse_random(&mut self) -> Option<crate::RandomInputDecl> {
        let id = self.take_id("random.id")?;
        let name = self.take_identifier("random.name")?;
        self.expect_keyword("on", "random");
        let input = self.distribution_path()?;
        self.expect_keyword("bits", "random");
        let bits = self.take_u16("random.bits")?;
        self.expect_keyword("independent", "random premise");
        self.expect_keyword("uniform", "random premise");
        self.expect_symbol(TokenKind::Semicolon, "random");
        Some(crate::RandomInputDecl {
            id,
            name,
            input,
            bits,
        })
    }

    fn parse_reward(&mut self) -> Option<crate::RewardDecl> {
        let id = self.take_id("reward.id")?;
        let name = self.take_identifier("reward.name")?;
        self.expect_keyword("random", "reward");
        self.expect_symbol(TokenKind::LBrace, "reward.random");
        let mut random = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof) {
            random.push(self.take_id("reward.random")?);
            if !self.take_symbol(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBrace, "reward.random");
        random.sort();
        self.expect_keyword("on", "reward");
        let outcome = self.distribution_path()?;
        let failure = if self.take_keyword("failure") {
            Some(self.take_i128("reward.failure")?)
        } else {
            None
        };
        self.expect_symbol(TokenKind::LBrace, "reward.measures");
        let mut measures = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof) {
            let kind = if self.take_keyword("expectation") {
                crate::RewardKind::Expectation
            } else if self.take_keyword("variance") {
                crate::RewardKind::Variance
            } else if self.take_keyword("threshold") {
                crate::RewardKind::Threshold(self.take_i128("reward.threshold")?)
            } else if self.take_keyword("excess") {
                crate::RewardKind::Excess(self.take_i128("reward.excess")?)
            } else {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "reward.measure",
                    "expectation, variance, threshold t or excess b",
                    self.describe_current(),
                    "declare a supported finite reward measure",
                );
                return None;
            };
            let comparison = if self.take_symbol(&TokenKind::LessEq) {
                crate::RewardComparison::AtMost
            } else if self.take_symbol(&TokenKind::GreaterEq) {
                crate::RewardComparison::AtLeast
            } else {
                self.expect_symbol(TokenKind::Equal, "reward.comparison");
                crate::RewardComparison::Equal
            };
            let numerator = self.take_i128("reward.bound")?;
            let denominator = if self.take_symbol(&TokenKind::Slash) {
                self.take_unsigned("reward.denominator")?
            } else {
                1
            };
            let Some(bound) = crate::Rational::try_new(numerator, denominator) else {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "reward.bound",
                    "reduced i64/u64 rational with positive denominator",
                    "invalid rational",
                    "use bounded exact rational components",
                );
                return None;
            };
            measures.push(crate::RewardMeasure {
                kind,
                comparison,
                bound,
            });
            self.expect_symbol(TokenKind::Semicolon, "reward.measure");
        }
        self.expect_symbol(TokenKind::RBrace, "reward.measures");
        measures.sort();
        if random.is_empty()
            || random.windows(2).any(|w| w[0] == w[1])
            || measures.is_empty()
            || measures.windows(2).any(|w| w[0] == w[1])
        {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "reward",
                "nonempty distinct premises and measures",
                "empty or duplicate entry",
                "name each random premise and measure once",
            );
            return None;
        }
        Some(crate::RewardDecl {
            id,
            name,
            random: random.into_boxed_slice(),
            outcome,
            failure,
            measures: measures.into_boxed_slice(),
        })
    }

    fn parse_distribution(&mut self) -> Option<crate::DistributionDecl> {
        let id = self.take_id("distribution.id")?;
        let name = self.take_identifier("distribution.name")?;
        self.expect_keyword("on", "distribution");
        let outcome = self.distribution_path()?;
        self.expect_symbol(TokenKind::Equal, "distribution");
        let uniform = self.take_keyword("uniform");
        if uniform {
            self.expect_keyword("over", "distribution");
        }
        self.expect_symbol(TokenKind::LBrace, "distribution");
        let mut target = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof) {
            let value = if self.take_keyword("committed-failure") {
                crate::DistributionOutcome::CommittedFailure
            } else {
                crate::DistributionOutcome::Value(self.take_i128("distribution.value")?)
            };
            let probability = if uniform {
                crate::Probability::try_new(1, 1)?
            } else {
                self.expect_symbol(TokenKind::Colon, "distribution");
                let numerator = self.take_unsigned("distribution.numerator")?;
                let denominator = if self.take_symbol(&TokenKind::Slash) {
                    self.take_unsigned("distribution.denominator")?
                } else {
                    1
                };
                match crate::Probability::try_new(numerator, denominator) {
                    Some(p) => p,
                    None => {
                        self.error(DiagnosticCode::InvalidDeclaration, "distribution.weight", "bounded nonnegative probability", "invalid rational", "use a positive denominator, mass at most one and reduced 64-bit components");
                        return None;
                    }
                }
            };
            target.push((value, probability));
            if !self.take_symbol(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBrace, "distribution");
        self.expect_symbol(TokenKind::Semicolon, "distribution");
        if uniform && !target.is_empty() {
            let p = crate::Probability::try_new(1, target.len() as u128)?;
            for (_, weight) in &mut target {
                *weight = p;
            }
        }
        let Some(target) = crate::DistributionDecl::normalized_target(target) else {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "distribution.target",
                "distinct outcomes with weights summing exactly to 1",
                "invalid target",
                "supply a nonempty normalized rational distribution within the arithmetic bounds",
            );
            return None;
        };
        Some(crate::DistributionDecl {
            id,
            name,
            outcome,
            target,
        })
    }

    fn parse_law(&mut self) -> Option<LawDecl> {
        let id = self.take_id("law.id")?;
        let name = self.take_identifier("law.name")?;
        let applicability = if self.take_keyword("on") {
            self.parse_law_applicability()
        } else {
            None
        };
        self.expect_symbol(TokenKind::Equal, "law");
        let formula = self.parse_rel();
        self.expect_symbol(TokenKind::Semicolon, "law");
        let law = LawDecl::new(id, name, formula);
        Some(match applicability {
            Some(applicability) => law.with_applicability(applicability),
            None => law,
        })
    }
    /// Parses `SCOPE[, genesis]` after `on` in a law declaration.
    fn parse_law_applicability(&mut self) -> Option<LawApplicability> {
        let scope_name = self.take_identifier("law.scope")?;
        let scope = match scope_name.as_str() {
            "any" => LawScope::Always,
            "accept" => LawScope::Accept,
            "reject" => LawScope::Reject,
            "failure" => LawScope::CommittedFailure,
            "commit" => LawScope::Committing,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "law.scope",
                    "any|accept|reject|failure|commit",
                    scope_name.as_str(),
                    "select the decisions the law is enforced on",
                );
                return None;
            }
        };
        let scope = if self.take_symbol(&TokenKind::Colon) {
            let reason = self.take_id("law.reason")?.get();
            if scope != LawScope::Reject {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "law.scope",
                    "reject:ID",
                    scope_name.as_str(),
                    "reason scopes require reject",
                );
                return None;
            }
            LawScope::RejectReason(reason)
        } else {
            scope
        };
        let genesis = self.take_symbol(&TokenKind::Comma);
        if genesis && !self.take_keyword("genesis") {
            self.error(
                DiagnosticCode::ExpectedToken,
                "law.scope",
                "genesis",
                self.describe_current(),
                "write `, genesis` for a law that also constrains the genesis state",
            );
            if matches!(self.current().kind, TokenKind::Ident(_)) {
                self.advance();
            }
            return None;
        }
        Some(LawApplicability::new(scope, genesis))
    }
    fn parse_claim(&mut self) -> Option<ClaimDecl> {
        let id = self.take_id("claim.id")?;
        let name = self.take_identifier("claim.name")?;
        let backend_name = self.take_identifier("claim.backend")?;
        let backends = match backend_name.as_str() {
            "cvc5" => vec![BackendId::Cvc5],
            "z3" => vec![BackendId::Z3],
            "lean" => vec![BackendId::Lean],
            "all" => vec![BackendId::Cvc5, BackendId::Z3, BackendId::Lean],
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "claim.backend",
                    "cvc5|z3|lean|all",
                    backend_name.as_str(),
                    "select a closed backend ID",
                );
                Vec::new()
            }
        };
        let mode_name = self.take_identifier("claim.mode")?;
        let mode = match mode_name.as_str() {
            "relational" => ClaimMode::Relational,
            "finite" => ClaimMode::Finite {
                horizon: self.take_u32("claim.horizon").unwrap_or(0),
            },
            "unbounded" => ClaimMode::UnboundedProof,
            "inductive" => ClaimMode::Inductive,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "claim.mode",
                    "relational|finite N|unbounded|inductive",
                    mode_name.as_str(),
                    "select an explicit claim mode",
                );
                ClaimMode::Relational
            }
        };
        let assumptions = if matches!(mode, ClaimMode::Inductive) {
            self.parse_inductive_assumptions()
        } else {
            InductiveAssumptions::default()
        };
        self.expect_symbol(TokenKind::Equal, "claim");
        let claim = match mode {
            ClaimMode::Relational => ClaimDecl::new(
                id,
                name,
                backends,
                mode,
                ClaimFormula::Relational(self.parse_rel()),
            ),
            ClaimMode::Inductive => {
                ClaimDecl::inductive(id, name, backends, assumptions, self.parse_rel())
            }
            ClaimMode::Finite { .. } | ClaimMode::UnboundedProof => ClaimDecl::new(
                id,
                name,
                backends,
                mode,
                ClaimFormula::Temporal(self.parse_temporal()),
            ),
        };
        self.expect_symbol(TokenKind::Semicolon, "claim");
        Some(claim)
    }

    /// Parses the groups after an inductive claim's mode, in this order, each
    /// optional: `assume [...]` for laws enforced on every committing decision,
    /// `accept [...]` for accepts, and `failure [...]` for committed failures.
    /// Elaboration checks that at least one law is named, once, and declared.
    fn parse_inductive_assumptions(&mut self) -> InductiveAssumptions {
        let mut groups: [Vec<StableId>; 3] = Default::default();
        let mut present = false;
        for (index, keyword) in ["assume", "accept", "failure"].into_iter().enumerate() {
            if self.take_keyword(keyword) {
                present = true;
                groups[index] = self.parse_law_list(keyword);
            }
        }
        if !present {
            self.error(
                DiagnosticCode::ExpectedToken,
                "claim.assume",
                "assume|accept|failure [LAW_ID, ...]",
                self.describe_current(),
                "list the laws the induction step may assume",
            );
        }
        let [every_commit, accepts, committed_failures] = groups;
        InductiveAssumptions::new(every_commit, accepts, committed_failures)
    }

    fn parse_law_list(&mut self, keyword: &str) -> Vec<StableId> {
        let context = alloc::format!("claim.{keyword}");
        self.expect_symbol(TokenKind::LBracket, &context);
        let mut laws = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBracket | TokenKind::Eof) {
            if let Some(law) = self.take_id("claim.law") {
                laws.push(law)
            }
            if !self.take_symbol(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBracket, &context);
        laws
    }

    fn parse_rel(&mut self) -> RelExpr {
        self.parse_implies()
    }
    fn parse_implies(&mut self) -> RelExpr {
        let left = self.parse_or();
        if self.take_symbol(&TokenKind::Arrow) {
            let right = self.nested("formula.implication", RelExpr::Bool(false), |parser| {
                parser.parse_implies()
            });
            RelExpr::Implies(Box::new(left), Box::new(right))
        } else {
            left
        }
    }
    fn parse_or(&mut self) -> RelExpr {
        let mut value = self.parse_and();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::OrOr) {
            if !self.chain_available("formula.or", operators) {
                break;
            }
            self.take_symbol(&TokenKind::OrOr);
            operators += 1;
            value = RelExpr::Or(Box::new(value), Box::new(self.parse_and()));
        }
        value
    }
    fn parse_and(&mut self) -> RelExpr {
        let mut value = self.parse_not();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::AndAnd) {
            if !self.chain_available("formula.and", operators) {
                break;
            }
            self.take_symbol(&TokenKind::AndAnd);
            operators += 1;
            value = RelExpr::And(Box::new(value), Box::new(self.parse_not()));
        }
        value
    }
    fn parse_not(&mut self) -> RelExpr {
        if self.take_symbol(&TokenKind::Bang) || self.take_keyword("not") {
            let value = self.nested("formula.not", RelExpr::Bool(false), |parser| {
                parser.parse_not()
            });
            RelExpr::Not(Box::new(value))
        } else {
            self.parse_rel_atom()
        }
    }
    fn parse_rel_atom(&mut self) -> RelExpr {
        if self.take_keyword("true") {
            return RelExpr::Bool(true);
        }
        if self.take_keyword("false") {
            return RelExpr::Bool(false);
        }
        if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("formula.group", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.expect_symbol(TokenKind::RParen, "formula");
            return value;
        }
        if (self.peek_keyword("forall")
            || self.peek_keyword("exists")
            || self.peek_keyword("count"))
            && self.entries_form_ahead(1)
        {
            return self.parse_entries_formula();
        }
        if (self.peek_keyword("forall")
            || self.peek_keyword("exists")
            || (self.peek_keyword("count") && self.legacy_count_ahead()))
            && self.table_form_ahead(1)
        {
            return self.parse_table_formula();
        }
        if self.peek_keyword("forall") || self.peek_keyword("exists") {
            let universal = self.take_keyword("forall");
            if !universal {
                self.take_keyword("exists");
            }
            let variable = self
                .take_identifier("quantifier.variable")
                .unwrap_or_else(fallback_identifier);
            self.expect_keyword("in", "quantifier");
            let start = self.take_i128("quantifier.start").unwrap_or(0);
            self.expect_symbol(TokenKind::Range, "quantifier");
            let end = self.take_i128("quantifier.end").unwrap_or(0);
            self.expect_symbol(TokenKind::LBrace, "quantifier");
            self.bound.push((variable.as_str().into(), None));
            let body = self.nested("formula.quantifier", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.bound.pop();
            self.expect_symbol(TokenKind::RBrace, "quantifier");
            return if universal {
                RelExpr::ForAll {
                    variable,
                    start,
                    end,
                    body: Box::new(body),
                }
            } else {
                RelExpr::Exists {
                    variable,
                    start,
                    end,
                    body: Box::new(body),
                }
            };
        }
        let start = self.current().span;
        let values = self.table_values;
        let atom = self.parse_comparison();
        if self.table_values != values && self.table_depth == 0 {
            self.rewrite_from(start, tables::rel_text(&atom));
        }
        atom
    }
    /// A named predicate or a comparison of two values.
    fn parse_comparison(&mut self) -> RelExpr {
        let named_predicate = matches!(
            (&self.current().kind, self.tokens.get(self.index + 1)),
            (TokenKind::Ident(name), Some(next))
                if matches!(next.kind, TokenKind::LParen)
                    && !matches!(name.as_ref(), "choose" | "div_exact" | "div_floor" | "div_ceil" | "mul_div_floor" | "mul_div_ceil" | "signed_mul_div_floor" | "repeat_mul_div_floor" | "repeat_signed_mul_div_floor" | "u128" | "to_i128" | "to_u128")
        );
        if named_predicate {
            let name = self
                .take_identifier("predicate.name")
                .unwrap_or_else(fallback_identifier);
            self.expect_symbol(TokenKind::LParen, "predicate");
            let mut arguments = Vec::new();
            while !matches!(self.current().kind, TokenKind::RParen | TokenKind::Eof) {
                arguments.push(self.parse_value());
                if !self.take_symbol(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_symbol(TokenKind::RParen, "predicate");
            return RelExpr::Predicate {
                name,
                arguments: arguments.into_boxed_slice(),
            };
        }
        let left = self.parse_value();
        let op = if self.take_symbol(&TokenKind::EqEq) {
            Some(CompareOp::Eq)
        } else if self.take_symbol(&TokenKind::NotEq) {
            Some(CompareOp::NotEq)
        } else if self.take_symbol(&TokenKind::LessEq) {
            Some(CompareOp::LessEq)
        } else if self.take_symbol(&TokenKind::Less) {
            Some(CompareOp::Less)
        } else if self.take_symbol(&TokenKind::GreaterEq) {
            Some(CompareOp::GreaterEq)
        } else if self.take_symbol(&TokenKind::Greater) {
            Some(CompareOp::Greater)
        } else {
            None
        };
        if let Some(op) = op {
            RelExpr::Compare(op, left, self.parse_value())
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "formula.comparison",
                "comparison operator",
                self.describe_current(),
                "compare scalar expressions or use a named predicate",
            );
            RelExpr::Bool(false)
        }
    }
    fn parse_value(&mut self) -> ValueExpr {
        let mut value = self.parse_term();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::Plus | TokenKind::Minus) {
            if !self.chain_available("value.additive", operators) {
                break;
            }
            let add = self.take_symbol(&TokenKind::Plus);
            if !add {
                self.take_symbol(&TokenKind::Minus);
            }
            operators += 1;
            let right = self.parse_term();
            value = if add {
                ValueExpr::Add(Box::new(value), Box::new(right))
            } else {
                ValueExpr::Sub(Box::new(value), Box::new(right))
            };
        }
        value
    }
    fn parse_term(&mut self) -> ValueExpr {
        let sum_ahead = self.peek_keyword("sum") && self.table_form_ahead(1);
        let mut value = self.parse_value_atom();
        if sum_ahead
            && matches!(self.current().kind, TokenKind::Star)
            && matches!(value, ValueExpr::Add(..))
        {
            self.error(
                DiagnosticCode::InvalidDeclaration,
                "table.sum",
                "a table sum after its factor",
                "a table sum before `*`",
                "write the factor first, as in `2 * sum i in table T { ... }`",
            );
        }
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::Star) {
            if !self.chain_available("value.multiplicative", operators) {
                break;
            }
            self.take_symbol(&TokenKind::Star);
            operators += 1;
            value = ValueExpr::Mul(Box::new(value), Box::new(self.parse_value_atom()));
        }
        value
    }
    fn parse_value_atom(&mut self) -> ValueExpr {
        if self.take_keyword("choose") {
            self.expect_symbol(TokenKind::LParen, "value.choose");
            let condition = self.nested("value.choose.condition", RelExpr::Bool(false), |p| {
                p.parse_rel()
            });
            self.expect_symbol(TokenKind::Comma, "value.choose");
            let then = self.nested("value.choose.then", ValueExpr::Int(0), |p| p.parse_value());
            self.expect_symbol(TokenKind::Comma, "value.choose");
            let otherwise = self.nested("value.choose.otherwise", ValueExpr::Int(0), |p| {
                p.parse_value()
            });
            self.expect_symbol(TokenKind::RParen, "value.choose");
            return ValueExpr::Select(Box::new(condition), Box::new(then), Box::new(otherwise));
        }
        if self.take_symbol(&TokenKind::Minus) {
            if self.current().kind == TokenKind::Number(1u128 << 127) {
                self.advance();
                return ValueExpr::Int(i128::MIN);
            }
            let value = self.nested("value.negation", ValueExpr::Int(0), |parser| {
                parser.parse_value_atom()
            });
            return ValueExpr::Sub(Box::new(ValueExpr::Int(0)), Box::new(value));
        }
        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            return match i128::try_from(value) {
                Ok(value) => ValueExpr::Int(value),
                Err(_) => {
                    self.error(
                        DiagnosticCode::InvalidNumber,
                        "value.integer",
                        "i128",
                        text(value),
                        "use a signed 128-bit integer",
                    );
                    ValueExpr::Int(0)
                }
            };
        }
        if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("value.group", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::RParen, "value");
            return value;
        }
        if self.take_keyword("u128") {
            self.expect_symbol(TokenKind::LParen, "u128 literal");
            let value = self.take_unsigned("u128.literal").unwrap_or(0);
            self.expect_symbol(TokenKind::RParen, "u128 literal");
            return ValueExpr::U128(value);
        }
        for (name, signed) in [("to_i128", true), ("to_u128", false)] {
            if self.take_keyword(name) {
                self.expect_symbol(TokenKind::LParen, "numeric cast");
                let value = self.nested("numeric cast", ValueExpr::Int(0), |p| p.parse_value());
                self.expect_symbol(TokenKind::RParen, "numeric cast");
                return if signed {
                    ValueExpr::ToI128(Box::new(value))
                } else {
                    ValueExpr::ToU128(Box::new(value))
                };
            }
        }
        let name = self
            .take_identifier("value")
            .unwrap_or_else(fallback_identifier);
        if matches!(
            name.as_str(),
            "mul_div_floor"
                | "mul_div_ceil"
                | "signed_mul_div_floor"
                | "repeat_mul_div_floor"
                | "repeat_signed_mul_div_floor"
        ) && self.take_symbol(&TokenKind::LParen)
        {
            let a = self.nested("value.fused.left", ValueExpr::Int(0), |p| p.parse_value());
            self.expect_symbol(TokenKind::Comma, "fused division");
            let b = self.nested("value.fused.right", ValueExpr::Int(0), |p| p.parse_value());
            self.expect_symbol(TokenKind::Comma, "fused division");
            let d = self.nested("value.fused.denominator", ValueExpr::Int(0), |p| {
                p.parse_value()
            });
            let signed = name.as_str().contains("signed");
            let a = Box::new(a);
            let b = Box::new(b);
            let d = Box::new(d);
            if name.as_str().starts_with("repeat_") {
                self.expect_symbol(TokenKind::Comma, "repeat count");
                let count = self.take_u16("repeat count").filter(|p| *p <= 16);
                if count.is_none() {
                    self.error(
                        DiagnosticCode::InvalidDeclaration,
                        "repeat count",
                        "literal 0..16",
                        "unsupported",
                        "use a bounded literal count",
                    );
                }
                self.expect_symbol(TokenKind::RParen, "repeat");
                return ValueExpr::RepeatMulDiv(signed, a, b, d, count.unwrap_or(0) as u8);
            }
            let mode = if name.as_str() == "mul_div_ceil" {
                DivisionMode::Ceil
            } else {
                DivisionMode::Floor
            };
            if self.take_symbol(&TokenKind::Comma) {
                let scale = self.parse_scale().unwrap_or(0);
                self.expect_symbol(TokenKind::RParen, "scaled division");
                return ValueExpr::ScaledMulDiv(mode, signed, a, b, d, scale);
            }
            self.expect_symbol(TokenKind::RParen, "fused division");
            return if signed {
                ValueExpr::SignedMulDivFloor(a, b, d)
            } else {
                ValueExpr::MulDiv(mode, a, b, d)
            };
        }
        if matches!(name.as_str(), "div_exact" | "div_floor" | "div_ceil")
            && self.take_symbol(&TokenKind::LParen)
        {
            let left = self.nested("value.division.left", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::Comma, "division");
            let right = self.nested("value.division.right", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::RParen, "division");
            let mode = match name.as_str() {
                "div_floor" => DivisionMode::Floor,
                "div_ceil" => DivisionMode::Ceil,
                _ => DivisionMode::Exact,
            };
            return ValueExpr::Div(mode, Box::new(left), Box::new(right));
        }
        if name.as_str() == "sum" && self.table_form_ahead(0) {
            return self.parse_table_sum();
        }
        if matches!(name.as_str(), "sum" | "count") && self.entries_form_ahead(0) {
            let start = self.tokens[self.index.saturating_sub(1)].span;
            self.refuse_entries_aggregate(start);
            return ValueExpr::Int(0);
        }
        if name.as_str() == "count" && self.table_form_ahead(0) {
            let start = self.tokens[self.index.saturating_sub(1)].span;
            let Some((variable, size)) = self.table_header(start) else {
                return ValueExpr::Int(0);
            };
            self.table_depth += 1;
            let conditions = self.count_conditions(&variable, size);
            self.table_depth -= 1;
            self.table_values += 1;
            return self.linear_count(start, conditions);
        }
        if name.as_str() == "sum" {
            let variable = self
                .take_identifier("sum.variable")
                .unwrap_or_else(fallback_identifier);
            self.expect_keyword("in", "sum");
            let start = self.take_i128("sum.start").unwrap_or(0);
            self.expect_symbol(TokenKind::Range, "sum");
            let end = self.take_i128("sum.end").unwrap_or(0);
            self.expect_symbol(TokenKind::LBrace, "sum");
            self.bound.push((variable.as_str().into(), None));
            let body = self.nested("value.sum", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.bound.pop();
            self.expect_symbol(TokenKind::RBrace, "sum");
            return ValueExpr::Sum {
                variable,
                start,
                end,
                body: Box::new(body),
            };
        }
        if let Some(root) = projection_root(name.as_str()) {
            let start = self.tokens[self.index.saturating_sub(1)].span;
            let mut segments = Vec::new();
            while self.take_symbol(&TokenKind::Dot) {
                let bound = match &self.current().kind {
                    TokenKind::Ident(word) => self.entry_variable(word),
                    _ => None,
                };
                if let Some(entry) = bound.and_then(StableId::new) {
                    self.advance();
                    self.table_values += 1;
                    segments.push(entry);
                } else if let Some(id) = self.take_id("projection.segment") {
                    segments.push(id)
                }
            }
            if matches!(self.current().kind, TokenKind::LBracket) {
                self.table_entry(&mut segments);
            }
            self.entry_path(start, &mut segments);
            if let Some(path) = ProjectionPath::try_new(root, segments) {
                return ValueExpr::Projection(path);
            }
        }
        if let Some(value) = self.index_value(name.as_str()) {
            return ValueExpr::Int(value);
        }
        ValueExpr::Var(name)
    }

    fn legacy_count_ahead(&self) -> bool {
        let mut at = self.index + 5;
        if self
            .tokens
            .get(at)
            .is_some_and(|t| t.kind == TokenKind::LBrace)
        {
            let mut braces = 1usize;
            at += 1;
            while let Some(token) = self.tokens.get(at) {
                match token.kind {
                    TokenKind::LBrace => braces += 1,
                    TokenKind::RBrace => braces -= 1,
                    _ => {}
                }
                at += 1;
                if braces == 0 {
                    break;
                }
            }
        }
        if !self.tokens.get(at).is_some_and(|t| {
            matches!(
                t.kind,
                TokenKind::EqEq
                    | TokenKind::NotEq
                    | TokenKind::Less
                    | TokenKind::LessEq
                    | TokenKind::Greater
                    | TokenKind::GreaterEq
            )
        }) {
            return false;
        }
        at += 1;
        if self
            .tokens
            .get(at)
            .is_some_and(|t| t.kind == TokenKind::Minus)
        {
            at += 1;
        }
        if !self
            .tokens
            .get(at)
            .is_some_and(|t| matches!(t.kind, TokenKind::Number(_)))
        {
            return false;
        }
        self.tokens.get(at + 1).is_some_and(|t| {
            matches!(
                t.kind,
                TokenKind::Semicolon
                    | TokenKind::RBrace
                    | TokenKind::RParen
                    | TokenKind::Comma
                    | TokenKind::AndAnd
                    | TokenKind::OrOr
                    | TokenKind::Arrow
                    | TokenKind::Eof
            )
        })
    }
    /// True when `IDENT in table` starts `offset` tokens ahead.
    fn table_form_ahead(&self, offset: usize) -> bool {
        let word = |at: usize, expected: Option<&str>| {
            matches!(self.tokens.get(self.index + offset + at).map(|t| &t.kind),
                Some(TokenKind::Ident(value)) if expected.is_none_or(|e| value.as_ref() == e))
        };
        word(0, None) && word(1, Some("in")) && word(2, Some("table"))
    }
    /// The value of a bound table index variable, unless a quantifier or
    /// sum variable of the same name shadows it.
    fn index_value(&self, name: &str) -> Option<i128> {
        self.bound
            .iter()
            .rev()
            .find(|(bound, _)| bound.as_ref() == name)
            .and_then(|(_, value)| *value)
    }
    /// Replaces the table ID ending `segments` with the ID of the entry the
    /// following `[INDEX]` names: an integer literal or a table index.
    fn table_entry(&mut self, segments: &mut [StableId]) {
        let start = self.current().span;
        self.take_symbol(&TokenKind::LBracket);
        let index = match self.current().kind.clone() {
            TokenKind::Number(value) => {
                self.advance();
                i128::try_from(value).ok()
            }
            TokenKind::Ident(name) => {
                self.advance();
                self.index_value(&name)
            }
            _ => None,
        };
        self.expect_symbol(TokenKind::RBracket, "table.entry");
        self.table_values += 1;
        let table = segments.last().map(|id| id.get());
        let size = table.and_then(|table| self.tables.get(&table).copied());
        let entry = match (table, size, index) {
            (Some(table), Some(size), Some(index)) if (0..i128::from(size)).contains(&index) => {
                u32::try_from(index)
                    .ok()
                    .and_then(|index| table.checked_add(index))
                    .and_then(StableId::new)
            }
            _ => None,
        };
        match (entry, segments.last_mut()) {
            (Some(entry), Some(last)) => *last = entry,
            _ => self.error_at(
                start,
                DiagnosticCode::InvalidDeclaration,
                "table.entry",
                match size {
                    Some(size) => format!(
                        "an entry index 0..={} of table {}",
                        size.max(1) - 1,
                        table.unwrap_or(0)
                    ),
                    None => "a declared table ID before `[`".into(),
                },
                "invalid table entry",
                "index a declared table with an integer literal or a table index variable",
            ),
        }
    }
    /// Reads `VAR in table T`, after the form's keyword. Returns the
    /// variable and the table's size, or None after reporting why.
    fn table_header(&mut self, start: SourceSpan) -> Option<(Identifier, u32)> {
        let variable = self.take_identifier("table.variable")?;
        self.expect_keyword("in", "table.variable");
        self.expect_keyword("table", "table.variable");
        let table = self.take_id("table.reference")?;
        if projection_root(variable.as_str()).is_some()
            || self
                .bound
                .iter()
                .any(|(bound, _)| bound.as_ref() == variable.as_str())
        {
            self.error_at(
                start,
                DiagnosticCode::InvalidDeclaration,
                "table.variable",
                "a fresh variable name",
                variable.as_str(),
                "choose a name that no enclosing form binds and that is not a projection root",
            );
        }
        match self.tables.get(&table.get()).copied() {
            Some(size) if (1..=MAX_TABLE_ENTRIES).contains(&size) => Some((variable, size)),
            _ => {
                self.error_at(
                    start,
                    DiagnosticCode::UnknownReference,
                    "table.reference",
                    "a declared table",
                    text(table.get()),
                    "declare `table ID OWNER name TYPE size N;` for this ID",
                );
                Some((variable, 1))
            }
        }
    }
    /// Parses the tokens from here once per entry, with `variable` bound to
    /// the entry's index, and leaves the parser after them.
    fn per_entry<T>(
        &mut self,
        variable: &Identifier,
        size: u32,
        mut parse: impl FnMut(&mut Self) -> T,
    ) -> Vec<T> {
        let start = self.index;
        let mut values = Vec::new();
        for k in 0..size {
            self.index = start;
            self.bound
                .push((variable.as_str().into(), Some(i128::from(k))));
            values.push(parse(self));
            self.bound.pop();
        }
        values
    }
    /// Charges expanded formula nodes; false, after reporting, when the
    /// expansion would exceed the formula-node limit.
    fn charge(&mut self, start: SourceSpan, nodes: usize) -> bool {
        self.expanded = self.expanded.saturating_add(nodes);
        if self.expanded <= MAX_FORMULA_NODES {
            return true;
        }
        self.error_at(
            start,
            DiagnosticCode::ResourceLimit,
            "table.expansion",
            text(MAX_FORMULA_NODES),
            "table expansion exceeds the formula-node limit",
            "use a smaller table or bound; compact checking of large tables is not supported",
        );
        false
    }
    /// `forall|exists VAR in table T [where C] { F }` and
    /// `count VAR in table T [{ C }] OP K`.
    fn parse_table_formula(&mut self) -> RelExpr {
        let start = self.current().span;
        let keyword = self
            .take_identifier("table.form")
            .unwrap_or_else(fallback_identifier);
        let Some((variable, size)) = self.table_header(start) else {
            return RelExpr::Bool(false);
        };
        self.table_depth += 1;
        let formula = if keyword.as_str() == "count" {
            self.parse_table_count(start, &variable, size)
        } else {
            let universal = keyword.as_str() == "forall";
            let entries: Vec<RelExpr> = self.per_entry(&variable, size, |parser| {
                let condition = parser
                    .take_keyword("where")
                    .then(|| parser.nested("table.where", RelExpr::Bool(false), |p| p.parse_rel()));
                parser.expect_symbol(TokenKind::LBrace, "table.body");
                let body = parser.nested("table.body", RelExpr::Bool(false), |p| p.parse_rel());
                parser.expect_symbol(TokenKind::RBrace, "table.body");
                match (condition, universal) {
                    (Some(c), true) => RelExpr::Implies(Box::new(c), Box::new(body)),
                    (Some(c), false) => RelExpr::And(Box::new(c), Box::new(body)),
                    (None, _) => body,
                }
            });
            let nodes = entries.iter().map(|e| formula_shape_rel(e).0).sum();
            if self.charge(start, nodes) {
                tables::chain(
                    entries,
                    if universal { RelExpr::And } else { RelExpr::Or },
                    universal,
                )
            } else {
                RelExpr::Bool(false)
            }
        };
        self.table_depth -= 1;
        if self.table_depth == 0 {
            self.rewrite_from(start, format!("({})", tables::rel_text(&formula)));
        }
        formula
    }
    fn count_conditions(&mut self, variable: &Identifier, size: u32) -> Vec<RelExpr> {
        if matches!(self.current().kind, TokenKind::LBrace) {
            self.per_entry(variable, size, |parser| {
                parser.expect_symbol(TokenKind::LBrace, "table.count");
                let condition =
                    parser.nested("table.count", RelExpr::Bool(false), |p| p.parse_rel());
                parser.expect_symbol(TokenKind::RBrace, "table.count");
                condition
            })
        } else {
            vec![RelExpr::Bool(true); size as usize]
        }
    }
    fn linear_count(&mut self, start: SourceSpan, conditions: Vec<RelExpr>) -> ValueExpr {
        let terms = conditions.len() as u128;
        let value = tables::total(
            conditions
                .into_iter()
                .map(|c| {
                    ValueExpr::Select(
                        Box::new(c),
                        Box::new(ValueExpr::Int(1)),
                        Box::new(ValueExpr::Int(0)),
                    )
                })
                .collect(),
        );
        let info = self.counts.entry(start.start()).or_insert(CountInfo {
            line: start.line(),
            column: start.column(),
            clauses: terms,
            linear: true,
            expansions: 0,
        });
        info.expansions += 1;
        if self.charge(start, formula_shape_value(&value).0) {
            value
        } else {
            ValueExpr::Int(0)
        }
    }
    fn parse_table_count(
        &mut self,
        start: SourceSpan,
        variable: &Identifier,
        size: u32,
    ) -> RelExpr {
        let conditions = self.count_conditions(variable, size);
        let op = [
            (TokenKind::EqEq, CompareOp::Eq),
            (TokenKind::NotEq, CompareOp::NotEq),
            (TokenKind::LessEq, CompareOp::LessEq),
            (TokenKind::Less, CompareOp::Less),
            (TokenKind::GreaterEq, CompareOp::GreaterEq),
            (TokenKind::Greater, CompareOp::Greater),
        ]
        .into_iter()
        .find_map(|(token, op)| self.take_symbol(&token).then_some(op));
        let Some(op) = op else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "table.count",
                "a comparison with an integer constant",
                self.describe_current(),
                "compare the count, as in `count i in table T { C } <= 2`",
            );
            return RelExpr::Bool(false);
        };
        let Some(bound) = self.take_i128("table.count.bound") else {
            return RelExpr::Bool(false);
        };
        let statics: Option<Vec<bool>> = conditions.iter().map(tables::static_truth).collect();
        if let Some(statics) = statics {
            let held = statics.iter().filter(|held| **held).count() as i128;
            return RelExpr::Bool(match op {
                CompareOp::Eq => held == bound,
                CompareOp::NotEq => held != bound,
                CompareOp::Less => held < bound,
                CompareOp::LessEq => held <= bound,
                CompareOp::Greater => held > bound,
                CompareOp::GreaterEq => held >= bound,
            });
        }
        let largest = conditions
            .iter()
            .map(|c| formula_shape_rel(c).0)
            .max()
            .unwrap_or(1);
        let (clauses, copies) = tables::count_size(conditions.len(), op, bound);
        let info = self.counts.entry(start.start()).or_insert(CountInfo {
            line: start.line(),
            column: start.column(),
            clauses,
            linear: false,
            expansions: 0,
        });
        info.expansions += 1;
        let nodes =
            usize::try_from(copies.saturating_mul(largest as u128 + 2)).unwrap_or(usize::MAX);
        self.expanded = self.expanded.saturating_add(nodes);
        if self.expanded > MAX_FORMULA_NODES {
            self.error_at(
                start,
                DiagnosticCode::ResourceLimit,
                "table.count",
                format!("at most {MAX_FORMULA_NODES} expanded formula nodes"),
                format!(
                    "{clauses} clauses of {} entries each, per expansion",
                    copies.checked_div(clauses).unwrap_or(0)
                ),
                "count over fewer entries or with a smaller bound; compact counting is future work",
            );
            return RelExpr::Bool(false);
        }
        tables::count(&conditions, op, bound)
    }
    /// `sum VAR in table T [where C] { V }`, after `sum`; dynamic C
    /// lowers to an eager Select and static filtering stays compatible.
    fn parse_table_sum(&mut self) -> ValueExpr {
        let start = self.tokens[self.index.saturating_sub(1)].span;
        let Some((variable, size)) = self.table_header(start) else {
            return ValueExpr::Int(0);
        };
        self.table_depth += 1;
        let entries = self.per_entry(&variable, size, |parser| {
            let condition = parser
                .take_keyword("where")
                .then(|| parser.nested("table.where", RelExpr::Bool(false), |p| p.parse_rel()));
            parser.expect_symbol(TokenKind::LBrace, "table.sum");
            let body = parser.nested("table.sum", ValueExpr::Int(0), |p| p.parse_value());
            parser.expect_symbol(TokenKind::RBrace, "table.sum");
            (condition, body)
        });
        self.table_depth -= 1;
        self.table_values += 1;
        let mut kept = Vec::new();
        for (condition, body) in entries {
            match condition.as_ref().map(tables::static_truth) {
                None | Some(Some(true)) => kept.push(body),
                Some(Some(false)) => {}
                Some(None) => kept.push(ValueExpr::Select(
                    Box::new(condition.unwrap_or(RelExpr::Bool(false))),
                    Box::new(body),
                    Box::new(ValueExpr::Int(0)),
                )),
            }
        }
        let nodes = kept.iter().map(|v| formula_shape_value(v).0).sum();
        if !self.charge(start, nodes) {
            return ValueExpr::Int(0);
        }
        tables::total(kept)
    }
    fn parse_temporal(&mut self) -> TemporalFormula {
        self.parse_temporal_until()
    }
    fn parse_temporal_until(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_or();
        let mut operators = 0usize;
        while self.peek_keyword("until") {
            if !self.chain_available("temporal.until", operators) {
                break;
            }
            self.take_keyword("until");
            operators += 1;
            value = TemporalFormula::Until(Box::new(value), Box::new(self.parse_temporal_or()));
        }
        value
    }
    fn parse_temporal_or(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_and();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::OrOr) {
            if !self.chain_available("temporal.or", operators) {
                break;
            }
            self.take_symbol(&TokenKind::OrOr);
            operators += 1;
            value = TemporalFormula::Or(Box::new(value), Box::new(self.parse_temporal_and()));
        }
        value
    }
    fn parse_temporal_and(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_atom();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::AndAnd) {
            if !self.chain_available("temporal.and", operators) {
                break;
            }
            self.take_symbol(&TokenKind::AndAnd);
            operators += 1;
            value = TemporalFormula::And(Box::new(value), Box::new(self.parse_temporal_atom()));
        }
        value
    }
    fn parse_temporal_atom(&mut self) -> TemporalFormula {
        if self.take_keyword("next") {
            let value = self.nested("temporal.next", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Next(Box::new(value))
        } else if self.take_keyword("always") {
            let value = self.nested("temporal.always", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Always(Box::new(value))
        } else if self.take_keyword("eventually") {
            let value = self.nested("temporal.eventually", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Eventually(Box::new(value))
        } else if self.take_keyword("not") || self.take_symbol(&TokenKind::Bang) {
            let value = self.nested("temporal.not", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Not(Box::new(value))
        } else if self.take_keyword("atom") {
            self.expect_symbol(TokenKind::LParen, "temporal.atom");
            let rel = self.nested("temporal.atom", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.expect_symbol(TokenKind::RParen, "temporal.atom");
            TemporalFormula::Atom(rel)
        } else if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("temporal.group", temporal_false(), |parser| {
                parser.parse_temporal()
            });
            self.expect_symbol(TokenKind::RParen, "temporal");
            value
        } else if self.take_keyword("true") {
            TemporalFormula::Atom(RelExpr::Bool(true))
        } else if self.take_keyword("false") {
            TemporalFormula::Atom(RelExpr::Bool(false))
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "temporal",
                "temporal operator or atom(<relational>)",
                self.describe_current(),
                "wrap relational formulas with `atom(...)`",
            );
            TemporalFormula::Atom(RelExpr::Bool(false))
        }
    }

    fn nested<T>(&mut self, path: &str, fallback: T, parse: impl FnOnce(&mut Self) -> T) -> T {
        if self.nesting >= MAX_FORMULA_DEPTH {
            self.error(
                DiagnosticCode::ResourceLimit,
                path,
                text(MAX_FORMULA_DEPTH),
                "expression nesting exceeds the parser limit",
                "reduce nested operators or split the claim into named laws",
            );
            return fallback;
        }
        self.nesting += 1;
        let value = parse(self);
        self.nesting -= 1;
        value
    }
    fn chain_available(&mut self, path: &str, operators: usize) -> bool {
        if operators.saturating_add(1) < MAX_FORMULA_DEPTH {
            return true;
        }
        self.error(
            DiagnosticCode::ResourceLimit,
            path,
            text(MAX_FORMULA_DEPTH),
            "operator chain exceeds the parser limit",
            "split the expression into named laws or predicates",
        );
        false
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len().saturating_sub(1))]
    }
    fn advance(&mut self) {
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
    }
    fn peek_keyword(&self, word: &str) -> bool {
        matches!(&self.current().kind,TokenKind::Ident(value) if value.as_ref()==word)
    }
    fn take_keyword(&mut self, word: &str) -> bool {
        if self.peek_keyword(word) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect_keyword(&mut self, word: &str, path: &str) {
        if !self.take_keyword(word) {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                word,
                self.describe_current(),
                format!("insert `{word}`"),
            );
        }
    }
    fn take_symbol(&mut self, kind: &TokenKind) -> bool {
        if same_symbol(&self.current().kind, kind) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect_symbol(&mut self, kind: TokenKind, path: &str) {
        if !self.take_symbol(&kind) {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                describe_kind(&kind),
                self.describe_current(),
                "insert the required punctuation",
            );
        }
    }
    fn take_number(&mut self, path: &str) -> Option<u64> {
        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            match u64::try_from(value) {
                Ok(value) => Some(value),
                Err(_) => {
                    self.error(
                        DiagnosticCode::InvalidNumber,
                        path,
                        "u64",
                        text(value),
                        "use a value within the u64 range",
                    );
                    None
                }
            }
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                "unsigned integer",
                self.describe_current(),
                "use an explicit decimal integer",
            );
            None
        }
    }
    fn take_u32(&mut self, path: &str) -> Option<u32> {
        let value = self.take_number(path)?;
        match u32::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "u32",
                    text(value),
                    "use a value no greater than 4294967295",
                );
                None
            }
        }
    }
    fn take_u16(&mut self, path: &str) -> Option<u16> {
        let value = self.take_number(path)?;
        match u16::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "u16",
                    text(value),
                    "use a value no greater than 65535",
                );
                None
            }
        }
    }
    fn take_i128(&mut self, path: &str) -> Option<i128> {
        let negative = self.take_symbol(&TokenKind::Minus);
        let TokenKind::Number(magnitude) = self.current().kind.clone() else {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                "integer",
                self.describe_current(),
                "use an explicit decimal integer",
            );
            return None;
        };
        self.advance();
        let value = if negative && magnitude == 1u128 << 127 {
            Some(i128::MIN)
        } else {
            i128::try_from(magnitude)
                .ok()
                .map(|value| if negative { -value } else { value })
        };
        if value.is_none() {
            self.error(
                DiagnosticCode::InvalidNumber,
                path,
                "i128",
                text(magnitude),
                "use a signed 128-bit integer",
            );
        }
        value
    }
    fn take_id(&mut self, path: &str) -> Option<StableId> {
        let value = self.take_u32(path)?;
        match StableId::new(value) {
            Some(value) => Some(value),
            None => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "nonzero stable ID",
                    "0",
                    "allocate an explicit nonzero stable ID",
                );
                None
            }
        }
    }
    fn take_identifier(&mut self, path: &str) -> Option<Identifier> {
        if let TokenKind::Ident(value) = self.current().kind.clone() {
            let span = self.current().span;
            self.advance();
            match Identifier::try_new(value) {
                Some(value) => Some(value),
                None => {
                    self.diagnostics.push(Diagnostic::new(DiagnosticCode::InvalidDeclaration,DiagnosticStage::Parse,AstPath::new(path),span,"ASCII identifier up to 128 bytes","invalid identifier","use letters, digits, underscore, and hyphen with a letter or underscore first"));
                    None
                }
            }
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                "ASCII identifier",
                self.describe_current(),
                "add an explicit stable name",
            );
            None
        }
    }
    fn describe_current(&self) -> Box<str> {
        match &self.current().kind {
            TokenKind::Ident(value) => value.clone(),
            TokenKind::Number(value) => text(value),
            TokenKind::Hex(value) => value.clone(),
            kind => Box::from(describe_kind(kind)),
        }
    }
    fn error(
        &mut self,
        code: DiagnosticCode,
        path: &str,
        expected: impl Into<Box<str>>,
        actual: impl Into<Box<str>>,
        remediation: impl Into<Box<str>>,
    ) {
        let span = self.current().span;
        self.error_at(span, code, path, expected, actual, remediation);
    }
    /// Reports at `span`. A table body is parsed once per entry, so a
    /// diagnostic already reported is not repeated.
    fn error_at(
        &mut self,
        span: SourceSpan,
        code: DiagnosticCode,
        path: &str,
        expected: impl Into<Box<str>>,
        actual: impl Into<Box<str>>,
        remediation: impl Into<Box<str>>,
    ) {
        let diagnostic = Diagnostic::new(
            code,
            DiagnosticStage::Parse,
            AstPath::new(path),
            span,
            expected,
            actual,
            remediation,
        );
        if !self.diagnostics.contains(&diagnostic) {
            self.diagnostics.push(diagnostic);
        }
    }
    fn recover_top(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) {
            self.advance();
        }
        if matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace
        ) {
            self.advance();
        }
    }
    fn recover_member(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) {
            self.advance();
        }
        if self.take_symbol(&TokenKind::Semicolon) {}
    }
}

/// The relational atoms of a temporal formula.
fn temporal_atoms<'a>(formula: &'a TemporalFormula, out: &mut Vec<&'a RelExpr>) {
    match formula {
        TemporalFormula::Atom(atom) => out.push(atom),
        TemporalFormula::Not(a)
        | TemporalFormula::Next(a)
        | TemporalFormula::Always(a)
        | TemporalFormula::Eventually(a) => temporal_atoms(a, out),
        TemporalFormula::And(a, b) | TemporalFormula::Or(a, b) | TemporalFormula::Until(a, b) => {
            temporal_atoms(a, out);
            temporal_atoms(b, out);
        }
    }
}

/// The stable IDs a declaration introduces, with its kind.
fn declaration_ids(declaration: &Declaration) -> Vec<(u32, &'static str)> {
    match declaration {
        Declaration::Namespace(d) => vec![(d.id().get(), "namespace")],
        Declaration::Type(d) => vec![(d.id().get(), "type")],
        Declaration::Field(d) => vec![(d.id().get(), "field")],
        Declaration::Variant(d) => vec![(d.id().get(), "variant")],
        Declaration::Reason(d) => vec![(d.id().get(), "reason")],
        Declaration::Effect(d) => vec![(d.id().get(), "effect")],
        Declaration::Channel(d) => vec![(d.id().get(), "channel")],
        Declaration::Component(d) => core::iter::once((d.id().get(), "component"))
            .chain(d.ports().iter().map(|port| (port.id().get(), "port")))
            .collect(),
        Declaration::Wiring(_) => Vec::new(),
        Declaration::Law(d) => vec![(d.id().get(), "law")],
        Declaration::Claim(d) => vec![(d.id().get(), "claim")],
        Declaration::Random(d) => vec![(d.id().get(), "random")],
        Declaration::Distribution(d) => vec![(d.id().get(), "distribution")],
        Declaration::Reward(d) => vec![(d.id().get(), "reward")],
    }
}

fn temporal_false() -> TemporalFormula {
    TemporalFormula::Atom(RelExpr::Bool(false))
}

fn projection_root(value: &str) -> Option<ProjectionRoot> {
    match value {
        "pre" => Some(ProjectionRoot::Pre),
        "post" => Some(ProjectionRoot::Post),
        "command" => Some(ProjectionRoot::Command),
        "context" => Some(ProjectionRoot::Context),
        "effects" => Some(ProjectionRoot::Effects),
        "outbox" => Some(ProjectionRoot::Outbox),
        "events" => Some(ProjectionRoot::Events),
        _ => None,
    }
}
fn one_id() -> StableId {
    StableId::new(1).unwrap_or_else(|| unreachable!())
}
fn fallback_identifier() -> Identifier {
    Identifier::try_new("invalid").unwrap_or_else(|| unreachable!())
}
fn same_symbol(left: &TokenKind, right: &TokenKind) -> bool {
    matches!(
        (left, right),
        (TokenKind::LBrace, TokenKind::LBrace)
            | (TokenKind::RBrace, TokenKind::RBrace)
            | (TokenKind::LParen, TokenKind::LParen)
            | (TokenKind::RParen, TokenKind::RParen)
            | (TokenKind::LBracket, TokenKind::LBracket)
            | (TokenKind::RBracket, TokenKind::RBracket)
            | (TokenKind::Semicolon, TokenKind::Semicolon)
            | (TokenKind::Comma, TokenKind::Comma)
            | (TokenKind::Dot, TokenKind::Dot)
            | (TokenKind::Range, TokenKind::Range)
            | (TokenKind::Colon, TokenKind::Colon)
            | (TokenKind::Equal, TokenKind::Equal)
            | (TokenKind::EqEq, TokenKind::EqEq)
            | (TokenKind::NotEq, TokenKind::NotEq)
            | (TokenKind::Less, TokenKind::Less)
            | (TokenKind::LessEq, TokenKind::LessEq)
            | (TokenKind::Greater, TokenKind::Greater)
            | (TokenKind::GreaterEq, TokenKind::GreaterEq)
            | (TokenKind::Plus, TokenKind::Plus)
            | (TokenKind::Minus, TokenKind::Minus)
            | (TokenKind::Star, TokenKind::Star)
            | (TokenKind::Slash, TokenKind::Slash)
            | (TokenKind::AndAnd, TokenKind::AndAnd)
            | (TokenKind::OrOr, TokenKind::OrOr)
            | (TokenKind::Bang, TokenKind::Bang)
            | (TokenKind::Arrow, TokenKind::Arrow)
            | (TokenKind::Eof, TokenKind::Eof)
    )
}
fn describe_kind(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Ident(_) => "identifier",
        TokenKind::Number(_) => "number",
        TokenKind::Hex(_) => "commitment",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::Semicolon => ";",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::Range => "..",
        TokenKind::Colon => ":",
        TokenKind::Equal => "=",
        TokenKind::EqEq => "==",
        TokenKind::NotEq => "!=",
        TokenKind::Less => "<",
        TokenKind::LessEq => "<=",
        TokenKind::Greater => ">",
        TokenKind::GreaterEq => ">=",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::AndAnd => "&&",
        TokenKind::OrOr => "||",
        TokenKind::Bang => "!",
        TokenKind::Arrow => "->",
        TokenKind::Eof => "end of file",
    }
}
