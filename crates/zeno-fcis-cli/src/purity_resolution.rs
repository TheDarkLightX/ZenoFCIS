//! Scoped source resolution for the mandatory V2 purity inspection.

use super::{Finding, ParsedSource, Report, ResolutionSummary, Severity};
use super::{
    MAX_FILE_BYTES, PATH_RULES, WARNING_PATH_RULES, matches_prefix, normalize, read_bounded,
};
use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};

type ScopeId = usize;
type SourceId = usize;
type Position = (usize, usize);

#[derive(Clone, Copy, Eq, PartialEq)]
enum Namespace {
    Type,
    Value,
    Macro,
    Any,
}

#[derive(Clone)]
enum Target {
    Module(ScopeId),
    Local(Vec<String>),
    Import {
        path: Vec<String>,
        absolute: bool,
        scope: ScopeId,
    },
    Macro,
}

#[derive(Clone)]
struct Binding {
    target: Target,
    namespace: Option<Namespace>,
    available: Position,
}

#[derive(Clone)]
struct Scope {
    parent: Option<ScopeId>,
    /// Position in the parent's source at which this lexical scope was declared.
    /// Macro textual inheritance uses this position, including across files.
    entered: Position,
    module: ScopeId,
    root: ScopeId,
    source: SourceId,
    directory: PathBuf,
    canonical: Vec<String>,
    names: BTreeMap<String, Vec<Binding>>,
    globs: Vec<(Vec<String>, bool)>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Resolved {
    External(Vec<String>),
    Local(Vec<String>),
    Module(ScopeId),
    Macro,
}

#[derive(Clone, Debug)]
struct Refusal {
    rule: &'static str,
    message: String,
}

fn refusal(rule: &'static str, message: impl Into<String>) -> Refusal {
    Refusal {
        rule,
        message: message.into(),
    }
}

#[derive(Clone)]
struct Graph<'a> {
    sources: &'a [ParsedSource],
    scopes: Vec<Scope>,
    nodes: BTreeMap<(SourceId, usize, usize, u8), ScopeId>,
    modules: BTreeMap<ScopeId, &'a [syn::Item]>,
    inactive_modules: BTreeSet<(SourceId, usize, usize)>,
    source_paths: BTreeMap<PathBuf, SourceId>,
    crates: BTreeMap<String, ScopeId>,
    roots: Vec<ScopeId>,
    externals: BTreeMap<ScopeId, BTreeSet<String>>,
    /// Immutable package context for the only supported compiler tool path.
    tool_context: BTreeMap<ScopeId, Result<(), String>>,
    findings: Vec<Finding>,
    summary: ResolutionSummary,
}

fn position(span: Span) -> Position {
    let p = span.start();
    (p.line, p.column)
}

impl<'a> Graph<'a> {
    fn new(sources: &'a [ParsedSource]) -> Self {
        Self {
            sources,
            scopes: Vec::new(),
            nodes: BTreeMap::new(),
            modules: BTreeMap::new(),
            inactive_modules: BTreeSet::new(),
            source_paths: sources
                .iter()
                .enumerate()
                .map(|(i, s)| (s.path.clone(), i))
                .collect(),
            crates: BTreeMap::new(),
            roots: Vec::new(),
            findings: Vec::new(),
            externals: BTreeMap::new(),
            tool_context: BTreeMap::new(),
            summary: ResolutionSummary::default(),
        }
    }

    fn report(&mut self, source: SourceId, span: Span, rule: &'static str, subject: String) {
        let (line, column) = position(span);
        self.findings.push(Finding {
            file: self.sources[source].path.display().to_string(),
            line,
            column: column + 1,
            rule,
            severity: Severity::Error,
            subject,
        });
    }

    fn active(&mut self, source: SourceId, attrs: &[syn::Attribute]) -> bool {
        if attrs.iter().any(inactive_cfg) {
            let line = attrs
                .first()
                .and_then(|a| a.path().segments.first())
                .map_or(0, |s| s.ident.span().start().line);
            self.summary.skipped_cfg.push(format!(
                "{}:{line}: explicit inactive native cfg",
                self.sources[source].path.display()
            ));
            false
        } else {
            true
        }
    }

    fn add_root(&mut self, source: SourceId, name: String) -> ScopeId {
        let id = self.scopes.len();
        self.tool_context
            .insert(id, compiler_tool_context(&self.sources[source].path, &name));
        let directory = self.sources[source]
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        self.scopes.push(Scope {
            parent: None,
            entered: (0, 0),
            module: id,
            root: id,
            source,
            directory,
            canonical: vec![name.clone()],
            names: BTreeMap::new(),
            globs: Vec::new(),
        });
        let key = name.replace('-', "_");
        if self.sources[source]
            .path
            .file_name()
            .is_some_and(|n| n == "lib.rs")
            || !self.crates.contains_key(&key)
        {
            self.crates.insert(key, id);
        }
        self.modules.insert(id, &self.sources[source].syntax.items);
        let mut externals: BTreeSet<String> = ["std", "core", "alloc"]
            .into_iter()
            .map(str::to_string)
            .collect();
        for (_, prefixes) in PATH_RULES {
            for path in *prefixes {
                externals.insert(path.split("::").next().unwrap_or_default().to_string());
            }
        }
        if let Some(parent) = self.sources[source].path.parent().and_then(Path::parent)
            && let Ok(text) = read_bounded(&parent.join("Cargo.toml"), MAX_FILE_BYTES)
        {
            externals.extend(
                super::manifest_dependencies(&text)
                    .names
                    .into_iter()
                    .map(|n| n.replace('-', "_")),
            );
        }
        self.externals.insert(id, externals);
        if let Some(externals) = self.externals.get_mut(&id) {
            externals.insert(name.replace('-', "_"));
        }
        self.roots.push(id);
        id
    }

    fn nested(&mut self, parent: ScopeId, source: SourceId, span: Span, kind: u8) -> ScopeId {
        let (line, column) = position(span);
        let id = self.scopes.len();
        let p = &self.scopes[parent];
        self.scopes.push(Scope {
            parent: Some(parent),
            entered: (line, column),
            module: p.module,
            root: p.root,
            source,
            directory: p.directory.clone(),
            canonical: p.canonical.clone(),
            names: BTreeMap::new(),
            globs: Vec::new(),
        });
        self.nodes.insert((source, line, column, kind), id);
        id
    }

    fn bind(
        &mut self,
        scope: ScopeId,
        name: String,
        target: Target,
        namespace: Option<Namespace>,
        available: Position,
    ) {
        self.scopes[scope]
            .names
            .entry(name)
            .or_default()
            .push(Binding {
                target,
                namespace,
                available,
            });
    }

    fn local(
        &mut self,
        scope: ScopeId,
        name: String,
        namespace: Option<Namespace>,
        available: Position,
    ) {
        let mut canonical = self.scopes[scope].canonical.clone();
        canonical.push(name.clone());
        self.bind(scope, name, Target::Local(canonical), namespace, available);
    }

    fn import(
        &mut self,
        scope: ScopeId,
        tree: &syn::UseTree,
        prefix: &mut Vec<String>,
        absolute: bool,
    ) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.unraw().to_string());
                self.import(scope, &p.tree, prefix, absolute);
                prefix.pop();
            }
            syn::UseTree::Group(g) => {
                for item in &g.items {
                    self.import(scope, item, prefix, absolute)
                }
            }
            syn::UseTree::Glob(_) => self.scopes[scope].globs.push((prefix.clone(), absolute)),
            syn::UseTree::Name(n) => {
                let mut path = prefix.clone();
                let local = if n.ident == "self" {
                    prefix.last().cloned().unwrap_or_default()
                } else {
                    path.push(n.ident.unraw().to_string());
                    n.ident.unraw().to_string()
                };
                self.bind(
                    scope,
                    local,
                    Target::Import {
                        path,
                        absolute,
                        scope,
                    },
                    None,
                    (0, 0),
                );
            }
            syn::UseTree::Rename(n) => {
                let mut path = prefix.clone();
                if n.ident != "self" {
                    path.push(n.ident.unraw().to_string())
                }
                self.bind(
                    scope,
                    n.rename.unraw().to_string(),
                    Target::Import {
                        path,
                        absolute,
                        scope,
                    },
                    None,
                    (0, 0),
                );
            }
        }
    }

    fn module(&mut self, parent: ScopeId, source: SourceId, item: &'a syn::ItemMod) {
        let name = item.ident.unraw().to_string();
        let directory = self.scopes[parent].directory.join(&name);
        if item.attrs.iter().any(has_module_path) {
            self.report(
                source,
                item.ident.span(),
                "resolution-module-path",
                format!("module {name}: path attribute is unsupported"),
            );
            return;
        }
        let (child_source, items) = if let Some((_, items)) = &item.content {
            (source, items.as_slice())
        } else {
            let flat = self.scopes[parent].directory.join(format!("{name}.rs"));
            let nested = directory.join("mod.rs");
            let matches: Vec<_> = [flat, nested]
                .into_iter()
                .filter_map(|p| self.source_paths.get(&p).copied())
                .collect();
            if matches.len() != 1 {
                self.report(
                    source,
                    item.ident.span(),
                    "resolution-module",
                    format!(
                        "module {name}: expected one readable declared module file, found {}",
                        matches.len()
                    ),
                );
                return;
            }
            let child = matches[0];
            (child, self.sources[child].syntax.items.as_slice())
        };
        if child_source != source
            && !self.active(child_source, &self.sources[child_source].syntax.attrs)
        {
            let (line, column) = position(item.ident.span());
            self.inactive_modules.insert((source, line, column));
            return;
        }
        let id = self.nested(parent, child_source, item.ident.span(), 1);
        self.scopes[id].module = id;
        self.scopes[id].directory = directory;
        self.scopes[id].canonical.push(name.clone());
        self.bind(
            parent,
            name,
            Target::Module(id),
            Some(Namespace::Type),
            (0, 0),
        );
        self.modules.insert(id, items);
        self.summary
            .active_files
            .push(self.sources[child_source].path.display().to_string());
        let mut index = Index {
            graph: self,
            scope: id,
            source: child_source,
        };
        for child in items {
            index.visit_item(child);
        }
    }
}

fn cfg_value(meta: &syn::Meta) -> Option<bool> {
    match meta {
        syn::Meta::Path(path)
            if path.is_ident("test")
                || path.is_ident("verus")
                || path.is_ident("verus_keep_ghost") =>
        {
            Some(false)
        }
        syn::Meta::List(list) => {
            let Ok(parts) = Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
            else {
                return None;
            };
            if list.path.is_ident("all") {
                if parts.iter().any(|m| cfg_value(m) == Some(false)) {
                    Some(false)
                } else if parts.iter().all(|m| cfg_value(m) == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if parts.iter().any(|m| cfg_value(m) == Some(true)) {
                    Some(true)
                } else if parts.iter().all(|m| cfg_value(m) == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && parts.len() == 1 {
                parts.first().and_then(cfg_value).map(|v| !v)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn meta_inactive(meta: &syn::Meta) -> bool {
    cfg_value(meta) == Some(false)
}

fn inactive_cfg(attribute: &syn::Attribute) -> bool {
    attribute.path().is_ident("cfg")
        && attribute
            .parse_args::<syn::Meta>()
            .is_ok_and(|m| meta_inactive(&m))
}

fn has_module_path(attribute: &syn::Attribute) -> bool {
    if attribute.path().is_ident("path") {
        return true;
    }
    if !attribute.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(parts) =
        attribute.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    else {
        return true;
    };
    let mut parts = parts.iter();
    if parts.next().is_some_and(meta_inactive) {
        return false;
    }
    parts.any(|m| m.path().is_ident("path"))
}

fn has_possible_attribute(meta: &syn::Meta, name: &str) -> bool {
    if meta.path().is_ident(name) {
        return true;
    }
    if !meta.path().is_ident("cfg_attr") {
        return false;
    }
    let syn::Meta::List(list) = meta else {
        return false;
    };
    let Ok(parts) =
        Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated.parse2(list.tokens.clone())
    else {
        return false;
    };
    let mut parts = parts.iter();
    if parts.next().is_some_and(meta_inactive) {
        return false;
    }
    parts.any(|m| has_possible_attribute(m, name))
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(x) => &x.attrs,
        syn::Item::Enum(x) => &x.attrs,
        syn::Item::ExternCrate(x) => &x.attrs,
        syn::Item::Fn(x) => &x.attrs,
        syn::Item::ForeignMod(x) => &x.attrs,
        syn::Item::Impl(x) => &x.attrs,
        syn::Item::Macro(x) => &x.attrs,
        syn::Item::Mod(x) => &x.attrs,
        syn::Item::Static(x) => &x.attrs,
        syn::Item::Struct(x) => &x.attrs,
        syn::Item::Trait(x) => &x.attrs,
        syn::Item::TraitAlias(x) => &x.attrs,
        syn::Item::Type(x) => &x.attrs,
        syn::Item::Union(x) => &x.attrs,
        syn::Item::Use(x) => &x.attrs,
        _ => &[],
    }
}

struct Index<'g, 'a> {
    graph: &'g mut Graph<'a>,
    scope: ScopeId,
    source: SourceId,
}

impl Index<'_, '_> {
    fn generics(&mut self, generics: &syn::Generics) {
        for parameter in &generics.params {
            match parameter {
                syn::GenericParam::Type(t) => self.graph.local(
                    self.scope,
                    t.ident.unraw().to_string(),
                    Some(Namespace::Type),
                    (0, 0),
                ),
                syn::GenericParam::Const(c) => self.graph.local(
                    self.scope,
                    c.ident.unraw().to_string(),
                    Some(Namespace::Value),
                    (0, 0),
                ),
                _ => {}
            }
        }
    }
    fn parameters(&mut self, signature: &syn::Signature) {
        self.generics(&signature.generics);
        for argument in &signature.inputs {
            match argument {
                syn::FnArg::Typed(arg) => self.pattern(&arg.pat, (0, 0)),
                syn::FnArg::Receiver(_) => self.graph.local(
                    self.scope,
                    "self".to_string(),
                    Some(Namespace::Value),
                    (0, 0),
                ),
            }
        }
    }
    fn pattern(&mut self, pattern: &syn::Pat, available: Position) {
        struct Names(Vec<String>);
        impl<'ast> Visit<'ast> for Names {
            fn visit_pat_ident(&mut self, p: &'ast syn::PatIdent) {
                self.0.push(p.ident.unraw().to_string());
                visit::visit_pat_ident(self, p)
            }
        }
        let mut names = Names(Vec::new());
        names.visit_pat(pattern);
        for name in names.0 {
            self.graph
                .local(self.scope, name, Some(Namespace::Value), available)
        }
    }
}

impl<'ast> Visit<'ast> for Index<'_, 'ast> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if !self.graph.active(self.source, item_attrs(item)) {
            return;
        }
        match item {
            syn::Item::Struct(x) => {
                self.graph
                    .local(self.scope, x.ident.unraw().to_string(), None, (0, 0))
            }
            syn::Item::Enum(x) => self.graph.local(
                self.scope,
                x.ident.unraw().to_string(),
                Some(Namespace::Type),
                (0, 0),
            ),
            syn::Item::Union(x) => self.graph.local(
                self.scope,
                x.ident.unraw().to_string(),
                Some(Namespace::Type),
                (0, 0),
            ),
            syn::Item::Trait(x) => self.graph.local(
                self.scope,
                x.ident.unraw().to_string(),
                Some(Namespace::Type),
                (0, 0),
            ),
            syn::Item::Const(x) => self.graph.local(
                self.scope,
                x.ident.unraw().to_string(),
                Some(Namespace::Value),
                (0, 0),
            ),
            syn::Item::Static(x) => self.graph.local(
                self.scope,
                x.ident.unraw().to_string(),
                Some(Namespace::Value),
                (0, 0),
            ),
            _ => {}
        }
        visit::visit_item(self, item);
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.graph.module(self.scope, self.source, item)
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.graph.import(
            self.scope,
            &item.tree,
            &mut Vec::new(),
            item.leading_colon.is_some(),
        )
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let local = item.rename.as_ref().map_or_else(
            || item.ident.unraw().to_string(),
            |(_, n)| n.unraw().to_string(),
        );
        let target = if item.ident == "self" {
            Target::Module(self.graph.scopes[self.scope].root)
        } else {
            Target::Import {
                path: vec![item.ident.unraw().to_string()],
                absolute: true,
                scope: self.scope,
            }
        };
        self.graph.bind(self.scope, local, target, None, (0, 0));
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        let old = self.scope;
        self.scope = self.graph.nested(old, self.source, item.ident.span(), 8);
        self.generics(&item.generics);
        let target = if let syn::Type::Path(path) = item.ty.as_ref() {
            Target::Import {
                path: path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.unraw().to_string())
                    .collect(),
                absolute: path.path.leading_colon.is_some(),
                scope: self.scope,
            }
        } else {
            let mut p = self.graph.scopes[self.scope].canonical.clone();
            p.push(item.ident.unraw().to_string());
            Target::Local(p)
        };
        self.graph.bind(
            old,
            item.ident.unraw().to_string(),
            target,
            Some(Namespace::Type),
            (0, 0),
        );
        visit::visit_item_type(self, item);
        self.scope = old;
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if let Some(name) = &item.ident
            && item.mac.path.is_ident("macro_rules")
        {
            self.graph.bind(
                self.scope,
                name.unraw().to_string(),
                Target::Macro,
                Some(Namespace::Macro),
                position(name.span()),
            );
            if item
                .attrs
                .iter()
                .any(|a| has_possible_attribute(&a.meta, "macro_export"))
            {
                self.graph.bind(
                    self.graph.scopes[self.scope].root,
                    name.unraw().to_string(),
                    Target::Macro,
                    Some(Namespace::Macro),
                    (0, 0),
                );
            }
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.graph.local(
            self.scope,
            item.sig.ident.unraw().to_string(),
            Some(Namespace::Value),
            (0, 0),
        );
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, item.sig.ident.span(), 2);
        self.parameters(&item.sig);
        visit::visit_item_fn(self, item);
        self.scope = previous;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !self.graph.active(self.source, &item.attrs) {
            return;
        }
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, item.sig.ident.span(), 2);
        self.parameters(&item.sig);
        visit::visit_impl_item_fn(self, item);
        self.scope = previous;
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, item.impl_token.span, 3);
        self.graph
            .local(self.scope, "Self".to_string(), None, (0, 0));
        self.generics(&item.generics);
        visit::visit_item_impl(self, item);
        self.scope = previous;
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, item.ident.span(), 3);
        self.graph
            .local(self.scope, "Self".to_string(), None, (0, 0));
        self.generics(&item.generics);
        visit::visit_item_trait(self, item);
        self.scope = previous;
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, block.brace_token.span.open(), 4);
        visit::visit_block(self, block);
        self.scope = previous;
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        let p = local.semi_token.span.end();
        self.pattern(&local.pat, (p.line, p.column));
        visit::visit_local(self, local);
    }
    fn visit_expr_closure(&mut self, expr: &'ast syn::ExprClosure) {
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, expr.or1_token.span, 5);
        for pat in &expr.inputs {
            self.pattern(pat, (0, 0))
        }
        visit::visit_expr_closure(self, expr);
        self.scope = previous;
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, arm.fat_arrow_token.spans[0], 6);
        self.pattern(&arm.pat, (0, 0));
        visit::visit_arm(self, arm);
        self.scope = previous;
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        let previous = self.scope;
        self.scope = self
            .graph
            .nested(previous, self.source, expr.for_token.span, 7);
        self.pattern(&expr.pat, (0, 0));
        self.visit_block(&expr.body);
        self.scope = previous;
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        let old = self.scope;
        self.scope = self.graph.nested(old, self.source, item.ident.span(), 8);
        self.generics(&item.generics);
        visit::visit_item_struct(self, item);
        self.scope = old;
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        let old = self.scope;
        self.scope = self.graph.nested(old, self.source, item.ident.span(), 8);
        self.generics(&item.generics);
        visit::visit_item_enum(self, item);
        self.scope = old;
    }
    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        let old = self.scope;
        self.scope = self.graph.nested(old, self.source, item.ident.span(), 8);
        self.generics(&item.generics);
        visit::visit_item_union(self, item);
        self.scope = old;
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if !self.graph.active(self.source, &item.attrs) {
            return;
        }
        let old = self.scope;
        self.scope = self
            .graph
            .nested(old, self.source, item.sig.ident.span(), 2);
        self.parameters(&item.sig);
        visit::visit_trait_item_fn(self, item);
        self.scope = old;
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        let old = self.scope;
        self.scope = self.graph.nested(old, self.source, expr.if_token.span, 9);
        self.condition(
            &expr.cond,
            position(expr.then_branch.brace_token.span.open()),
        );
        self.visit_block(&expr.then_branch);
        self.scope = old;
        if let Some((_, other)) = &expr.else_branch {
            self.visit_expr(other)
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        let old = self.scope;
        self.scope = self
            .graph
            .nested(old, self.source, expr.while_token.span, 9);
        self.condition(&expr.cond, position(expr.body.brace_token.span.open()));
        self.visit_block(&expr.body);
        self.scope = old;
    }
    fn visit_expr_macro(&mut self, expr: &'ast syn::ExprMacro) {
        self.graph.active(self.source, &expr.attrs);
    }
    fn visit_stmt_macro(&mut self, stmt: &'ast syn::StmtMacro) {
        self.graph.active(self.source, &stmt.attrs);
    }
}

impl<'ast> Index<'_, 'ast> {
    fn condition(&mut self, expr: &'ast syn::Expr, available: Position) {
        match expr {
            syn::Expr::Let(x) => {
                self.visit_expr(&x.expr);
                self.visit_pat(&x.pat);
                self.pattern(&x.pat, available)
            }
            syn::Expr::Binary(x) if matches!(x.op, syn::BinOp::And(_)) => {
                let syn::BinOp::And(token) = &x.op else {
                    return;
                };
                let end = token.spans[1].end();
                self.condition(&x.left, (end.line, end.column));
                self.condition(&x.right, available);
            }
            syn::Expr::Paren(x) => self.condition(&x.expr, available),
            _ => self.visit_expr(expr),
        }
    }
}

impl Graph<'_> {
    /// A tool qualifier cannot inherit its guarantee from a declared crate,
    /// an applicable lexical type/module/import binding, or unknown lookup.
    fn compiler_tool(&self, scope: ScopeId, at: Position) -> Result<(), Refusal> {
        let root = self.scopes[scope].root;
        match self.tool_context.get(&root) {
            Some(Ok(())) => {}
            Some(Err(reason)) => {
                return Err(refusal("resolution-attribute", reason.clone()));
            }
            None => {
                return Err(refusal(
                    "resolution-attribute",
                    "compiler tool namespace rustfmt has no established package context",
                ));
            }
        }
        match self.lookup(scope, "rustfmt", Namespace::Type, at, &mut BTreeSet::new()) {
            Ok(None) => Ok(()),
            Ok(Some(target)) => Err(refusal(
                "resolution-attribute",
                format!("compiler tool namespace rustfmt is shadowed by {target:?}"),
            )),
            Err(error) => Err(refusal(
                "resolution-attribute",
                format!(
                    "compiler tool namespace rustfmt is unresolved: {}: {}",
                    error.rule, error.message
                ),
            )),
        }
    }

    /// Bare macro names first use textual scope, then the ordinary macro
    /// namespace. Only textual macro definitions inherit across module edges;
    /// type, value, import and qualified-member lookup keep their boundaries.
    fn lookup_macro(
        &self,
        scope: ScopeId,
        name: &str,
        at: Position,
        seen: &mut BTreeSet<(ScopeId, String, u8)>,
    ) -> Result<Option<Resolved>, Refusal> {
        let mut textual_scope = scope;
        let mut textual_at = at;
        loop {
            let node = &self.scopes[textual_scope];
            if node.names.get(name).is_some_and(|bindings| {
                bindings.iter().any(|binding| {
                    binding.namespace == Some(Namespace::Macro)
                        && matches!(binding.target, Target::Macro)
                        // Zero marks an exported, path-based binding, not a
                        // textual definition inherited by child modules.
                        && binding.available != (0, 0)
                        && binding.available <= textual_at
                })
            }) {
                return Ok(Some(Resolved::Macro));
            }
            let Some(parent) = node.parent else {
                break;
            };
            textual_at = node.entered;
            textual_scope = parent;
        }
        self.lookup(scope, name, Namespace::Macro, at, seen)
    }

    fn resolve(
        &self,
        scope: ScopeId,
        path: &[String],
        absolute: bool,
        namespace: Namespace,
        at: Position,
        seen: &mut BTreeSet<(ScopeId, String, u8)>,
    ) -> Result<Resolved, Refusal> {
        let Some(first) = path.first() else {
            return Err(refusal("resolution-unresolved", "empty path"));
        };
        let module = self.scopes[scope].module;
        let mut rest = &path[1..];
        let target = if first == "crate" {
            Resolved::Module(self.scopes[scope].root)
        } else if first == "self" && (path.len() > 1 || namespace != Namespace::Value) {
            Resolved::Module(module)
        } else if first == "super" {
            let mut m = module;
            let mut count = 1;
            while rest.first().is_some_and(|s| s == "super") {
                count += 1;
                rest = &rest[1..];
            }
            for _ in 0..count {
                m = self.scopes[m]
                    .parent
                    .map(|p| self.scopes[p].module)
                    .ok_or_else(|| {
                        refusal("resolution-unresolved", "super path escapes crate root")
                    })?;
            }
            Resolved::Module(m)
        } else if absolute {
            self.external(scope, first)?
        } else {
            match self.lookup(
                scope,
                first,
                if path.len() > 1 {
                    Namespace::Type
                } else {
                    namespace
                },
                at,
                seen,
            )? {
                Some(t) => t,
                None => self.external(scope, first)?,
            }
        };
        self.extend(target, rest, namespace, at, seen)
    }

    fn external(&self, scope: ScopeId, name: &str) -> Result<Resolved, Refusal> {
        if self.externals[&self.scopes[scope].root].contains(name)
            && let Some(root) = self.crates.get(name)
        {
            return Ok(Resolved::Module(*root));
        }
        let prelude = match name {
            "Vec" => Some(&["alloc", "vec", "Vec"][..]),
            "String" => Some(&["alloc", "string", "String"][..]),
            "Box" => Some(&["alloc", "boxed", "Box"][..]),
            "Option" | "Some" | "None" => Some(&["core", "option", "Option"][..]),
            "Result" | "Ok" | "Err" => Some(&["core", "result", "Result"][..]),
            "Iterator" | "IntoIterator" => Some(&["core", "iter", name][..]),
            "Default" => Some(&["core", "default", "Default"][..]),
            "ToOwned" => Some(&["alloc", "borrow", "ToOwned"][..]),
            "ToString" => Some(&["alloc", "string", "ToString"][..]),
            _ => None,
        };
        let primitive = [
            "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32",
            "i64", "i128", "isize", "f32", "f64",
        ];
        let other_prelude = [
            "Clone",
            "Copy",
            "Send",
            "Sync",
            "Sized",
            "Unpin",
            "Drop",
            "Eq",
            "PartialEq",
            "Ord",
            "PartialOrd",
            "From",
            "Into",
            "TryFrom",
            "TryInto",
            "Fn",
            "FnMut",
            "FnOnce",
            "AsRef",
            "AsMut",
            "Extend",
            "DoubleEndedIterator",
            "ExactSizeIterator",
        ];
        if prelude.is_none()
            && !primitive.contains(&name)
            && !other_prelude.contains(&name)
            && !self.externals[&self.scopes[scope].root].contains(name)
        {
            return Err(refusal(
                "resolution-unresolved",
                format!("unresolved binding or undeclared external root {name}"),
            ));
        }
        Ok(Resolved::External(prelude.map_or_else(
            || vec![name.to_string()],
            |p| p.iter().map(|s| (*s).to_string()).collect(),
        )))
    }

    fn lookup(
        &self,
        mut scope: ScopeId,
        name: &str,
        namespace: Namespace,
        at: Position,
        seen: &mut BTreeSet<(ScopeId, String, u8)>,
    ) -> Result<Option<Resolved>, Refusal> {
        loop {
            let key = (scope, name.to_string(), namespace as u8);
            if !seen.insert(key.clone()) {
                return Err(refusal(
                    "resolution-cycle",
                    format!("cyclic binding {name}"),
                ));
            }
            let node = &self.scopes[scope];
            let mut choices = BTreeSet::new();
            if let Some(bindings) = node.names.get(name) {
                let available = bindings
                    .iter()
                    .filter(|b| {
                        b.namespace
                            .is_none_or(|n| n == namespace || namespace == Namespace::Any)
                            && b.available <= at
                    })
                    .map(|b| b.available)
                    .max();
                for binding in bindings.iter().filter(|b| {
                    b.namespace
                        .is_none_or(|n| n == namespace || namespace == Namespace::Any)
                        && b.available <= at
                        && Some(b.available) == available
                }) {
                    let target = match &binding.target {
                        Target::Module(m) => Resolved::Module(*m),
                        Target::Local(p) => Resolved::Local(p.clone()),
                        Target::Macro => Resolved::Macro,
                        Target::Import {
                            path,
                            absolute,
                            scope: s,
                        } => self.resolve(*s, path, *absolute, namespace, at, seen)?,
                    };
                    choices.insert(target);
                }
            }
            if choices.is_empty() {
                for (path, absolute) in &node.globs {
                    // Looking up the root of this very glob must not import
                    // that root through itself before the extern prelude.
                    if !absolute && path.first().is_some_and(|p| p == name) {
                        continue;
                    }
                    let target = self.resolve(scope, path, *absolute, Namespace::Type, at, seen)?;
                    match target {
                        Resolved::Module(m) => {
                            if let Some(target) =
                                self.lookup_member(m, name, namespace, at, seen)?
                            {
                                choices.insert(target);
                            }
                        }
                        _ => {
                            return Err(refusal(
                                "resolution-glob",
                                format!("external or opaque glob {}::*", path.join("::")),
                            ));
                        }
                    }
                }
            }
            seen.remove(&key);
            if choices.len() > 1 {
                return Err(refusal(
                    "resolution-ambiguity",
                    format!("multiple targets for {name}"),
                ));
            }
            if let Some(target) = choices.into_iter().next() {
                return Ok(Some(target));
            }
            if node.module == scope {
                return Ok(None);
            }
            match node.parent {
                Some(parent) => scope = parent,
                None => return Ok(None),
            }
        }
    }

    fn lookup_member(
        &self,
        module: ScopeId,
        name: &str,
        namespace: Namespace,
        at: Position,
        seen: &mut BTreeSet<(ScopeId, String, u8)>,
    ) -> Result<Option<Resolved>, Refusal> {
        self.lookup(module, name, namespace, at, seen)
    }

    fn extend(
        &self,
        target: Resolved,
        rest: &[String],
        namespace: Namespace,
        at: Position,
        seen: &mut BTreeSet<(ScopeId, String, u8)>,
    ) -> Result<Resolved, Refusal> {
        if rest.is_empty() {
            return Ok(target);
        }
        match target {
            Resolved::Module(m) => {
                let next = self
                    .lookup_member(
                        m,
                        &rest[0],
                        if rest.len() == 1 {
                            namespace
                        } else {
                            Namespace::Type
                        },
                        at,
                        seen,
                    )?
                    .ok_or_else(|| {
                        refusal(
                            "resolution-unresolved",
                            format!(
                                "unknown declared module member {}::{}",
                                self.scopes[m].canonical.join("::"),
                                rest[0]
                            ),
                        )
                    })?;
                self.extend(next, &rest[1..], namespace, at, seen)
            }
            Resolved::External(mut p) => {
                p.extend_from_slice(rest);
                Ok(Resolved::External(p))
            }
            Resolved::Local(mut p) => {
                p.extend_from_slice(rest);
                Ok(Resolved::Local(p))
            }
            Resolved::Macro => Err(refusal("resolution-macro", "macro used as a namespace")),
        }
    }
}

pub(super) fn check(paths: &[PathBuf], mut report: Report, sources: &[ParsedSource]) -> Report {
    // Retain manifest rule findings, but do not let the file-wide reference
    // alias map make decisions for the resolved route.
    report.findings.retain(|f| f.file.ends_with("Cargo.toml"));
    let mut graph = Graph::new(sources);
    let mut selected = BTreeSet::new();
    for path in paths {
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            let manifest = path.join("Cargo.toml");
            let package = package_name(&manifest);
            let name = package.clone().unwrap_or_else(|| {
                path.file_name()
                    .map_or_else(|| "crate".to_string(), |n| n.to_string_lossy().into_owned())
            });
            let mut established = false;
            for root in [path.join("src/lib.rs"), path.join("src/main.rs")] {
                if let Some(source) = graph.source_paths.get(&root).copied() {
                    established = true;
                    if selected.insert(source) {
                        graph.add_root(source, name.clone());
                    }
                }
            }
            if !established {
                report.findings.push(package_finding(
                    &manifest,
                    "resolution-target",
                    "requested package has no established readable supported source root (src/lib.rs or src/main.rs)".to_string(),
                ));
            }
            if package.is_none() {
                report.findings.push(package_finding(
                    &manifest,
                    "resolution-manifest",
                    "package identity is not established by the supported manifest syntax"
                        .to_string(),
                ));
            }
            // The original exact structural diagnostics remain present;
            // unsupported target/source resolution also refuses this route.
            if let Some(structure) = report
                .structures
                .iter()
                .find(|s| s.krate == path.display().to_string())
            {
                if !structure.unrecognized_manifest.is_empty() {
                    report.findings.push(package_finding(
                        &manifest,
                        "resolution-manifest",
                        format!(
                            "unresolved manifest identity/targets: {}",
                            structure.unrecognized_manifest.join("; ")
                        ),
                    ));
                }
                if !structure.binary_targets.is_empty() {
                    report.findings.push(package_finding(
                        &manifest,
                        "resolution-target",
                        format!(
                            "binary target closure is unsupported: {}",
                            structure.binary_targets.join("; ")
                        ),
                    ));
                }
            }
        } else if path.is_dir() {
            for (source, item) in sources
                .iter()
                .enumerate()
                .filter(|(_, s)| s.path.starts_with(path))
            {
                if selected.insert(source) {
                    graph.add_root(source, format!("source{source}"));
                }
                let _ = item;
            }
        } else if let Some(source) = graph.source_paths.get(path).copied()
            && selected.insert(source)
        {
            graph.add_root(source, format!("source{source}"));
        }
    }
    let roots = graph.roots.clone();
    for root in &roots {
        let source = graph.scopes[*root].source;
        if !graph.active(source, &sources[source].syntax.attrs) {
            continue;
        }
        graph
            .summary
            .active_files
            .push(sources[source].path.display().to_string());
        let mut index = Index {
            graph: &mut graph,
            scope: *root,
            source,
        };
        for item in &sources[source].syntax.items {
            index.visit_item(item)
        }
    }
    let mut findings = Vec::new();
    for root in roots {
        let source = graph.scopes[root].source;
        if sources[source].syntax.attrs.iter().any(inactive_cfg) {
            continue;
        }
        let mut scanner = Scan {
            graph: &graph,
            scope: root,
            source,
            findings: Vec::new(),
            temporary: Vec::new(),
        };
        for attribute in &sources[source].syntax.attrs {
            scanner.visit_attribute(attribute)
        }
        for item in &sources[source].syntax.items {
            scanner.visit_item(item)
        }
        findings.extend(scanner.findings);
    }
    report.findings.extend(graph.findings);
    report.findings.extend(findings);
    graph.summary.active_files.sort();
    graph.summary.active_files.dedup();
    graph.summary.skipped_cfg.sort();
    graph.summary.skipped_cfg.dedup();
    report.resolution = Some(graph.summary);
    report.findings.sort();
    report.findings.dedup();
    report
}

/// Package closure failures exist even when no source root could be selected.
fn package_finding(path: &Path, rule: &'static str, subject: String) -> Finding {
    Finding {
        file: path.display().to_string(),
        line: 0,
        column: 0,
        rule,
        severity: Severity::Error,
        subject,
    }
}

fn package_name(path: &Path) -> Option<String> {
    let text = read_bounded(path, MAX_FILE_BYTES).ok()?;
    package_name_text(&text)
}

fn package_name_text(text: &str) -> Option<String> {
    let mut package = false;
    for raw in text.strip_prefix('\u{feff}').unwrap_or(text).lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            package = line == "[package]";
            continue;
        }
        if package {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim().trim_matches(['\'', '"']) == "name" {
                let value = value.trim();
                let quote = value.chars().next()?;
                if !matches!(quote, '\'' | '"') {
                    return None;
                }
                let name = value[1..].split(quote).next()?;
                return super::dependency_name(name);
            }
        }
    }
    None
}

/// Find the closest enclosing manifest even for a directly selected nested
/// source file. Standalone inputs use the declared default compiler prelude;
/// caller-injected undeclared extern crates remain outside that profile.
fn compiler_tool_context(source: &Path, name: &str) -> Result<(), String> {
    if name.replace('-', "_") == "rustfmt" {
        return Err("compiler tool namespace rustfmt collides with the selected crate name".into());
    }
    let source = compiler_context_path(source)?;
    for directory in source.ancestors().skip(1) {
        let manifest = directory.join("Cargo.toml");
        match std::fs::symlink_metadata(&manifest) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "compiler tool namespace rustfmt has unreadable package context {}: {error}",
                    manifest.display()
                ));
            }
            Ok(_) => {}
        }
        let text = read_bounded(&manifest, MAX_FILE_BYTES).map_err(|error| {
            format!(
                "compiler tool namespace rustfmt has unreadable manifest {}: {error}",
                manifest.display()
            )
        })?;
        let dependencies = super::manifest_dependencies(&text);
        let package = package_name_text(&text);
        // A binary target cannot add a dependency or rename the library, so it
        // cannot bind `rustfmt`. Every other unread manifest form still refuses.
        let unrecognized: Vec<&str> = dependencies
            .unrecognized
            .iter()
            .map(String::as_str)
            .filter(|line| !line.ends_with(super::BINARY_TARGET))
            .collect();
        if !unrecognized.is_empty() || package.is_none() {
            return Err(format!(
                "compiler tool namespace rustfmt has unsupported manifest identity {}: {}",
                manifest.display(),
                unrecognized.join("; ")
            ));
        }
        if package.as_deref() == Some("rustfmt")
            || dependencies
                .names
                .iter()
                .any(|dependency| dependency.replace('-', "_") == "rustfmt")
        {
            return Err(format!(
                "compiler tool namespace rustfmt collides with a declared package/crate binding in {}",
                manifest.display()
            ));
        }
        // The bounded manifest reader retains every runtime target/feature
        // alternative, so an optional or target-gated collision also refuses.
        return Ok(());
    }
    Ok(())
}

/// Anchor lookup to the actual working directory, retaining lexical package
/// ancestry. Following a source symlink could select a different package.
fn compiler_context_path(source: &Path) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| {
        format!("compiler tool namespace rustfmt cannot inspect current working directory: {error}")
    })?;
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        cwd.join(source)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let metadata = std::fs::symlink_metadata(&normalized).map_err(|error| {
                    format!(
                        "compiler tool namespace rustfmt has unreadable parent context {}: {error}",
                        normalized.display()
                    )
                })?;
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "compiler tool namespace rustfmt cannot establish parent context through symbolic link {}",
                        normalized.display()
                    ));
                }
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

struct Scan<'g, 'a> {
    graph: &'g Graph<'a>,
    scope: ScopeId,
    source: SourceId,
    findings: Vec<Finding>,
    temporary: Vec<BTreeSet<String>>,
}

impl Scan<'_, '_> {
    fn report(&mut self, span: Span, rule: &'static str, severity: Severity, subject: String) {
        let (line, column) = position(span);
        self.findings.push(Finding {
            file: self.graph.sources[self.source].path.display().to_string(),
            line,
            column: column + 1,
            rule,
            severity,
            subject,
        });
    }
    fn error(&mut self, span: Span, rule: &'static str, subject: impl Into<String>) {
        self.report(span, rule, Severity::Error, subject.into())
    }
    fn target(
        &mut self,
        path: &[String],
        absolute: bool,
        namespace: Namespace,
        span: Span,
    ) -> Option<Resolved> {
        if !absolute
            && namespace == Namespace::Value
            && path.len() == 1
            && self
                .temporary
                .iter()
                .rev()
                .any(|names| names.contains(&path[0]))
        {
            return Some(Resolved::Local(path.to_vec()));
        }
        match self.graph.resolve(
            self.scope,
            path,
            absolute,
            namespace,
            position(span),
            &mut BTreeSet::new(),
        ) {
            Ok(target) => Some(target),
            Err(e) => {
                self.error(span, e.rule, e.message);
                None
            }
        }
    }
    fn path(&mut self, path: &syn::Path, namespace: Namespace) {
        let names: Vec<String> = path
            .segments
            .iter()
            .map(|s| s.ident.unraw().to_string())
            .collect();
        if let Some(first) = path.segments.first()
            && let Some(Resolved::External(path)) = self.target(
                &names,
                path.leading_colon.is_some(),
                namespace,
                first.ident.span(),
            )
        {
            self.rules(&path, first.ident.span());
        }
    }
    fn rules(&mut self, path: &[String], span: Span) {
        let name = normalize(path).join("::");
        for (rule, prefixes) in PATH_RULES {
            if prefixes.iter().any(|p| matches_prefix(&name, p)) {
                self.error(span, rule, name.clone())
            }
        }
        for (rule, prefixes) in WARNING_PATH_RULES {
            if prefixes.iter().any(|p| matches_prefix(&name, p)) {
                self.report(span, rule, Severity::Warning, name.clone())
            }
        }
    }
    fn enter(&mut self, span: Span, kind: u8) -> Option<ScopeId> {
        let (line, column) = position(span);
        let old = self.scope;
        match self.graph.nodes.get(&(self.source, line, column, kind)) {
            Some(scope) => {
                self.scope = *scope;
                Some(old)
            }
            None => {
                self.error(
                    span,
                    "resolution-scope",
                    "unsupported scope introduced by macro/expansion",
                );
                None
            }
        }
    }
    fn imports(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>, absolute: bool) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.unraw().to_string());
                self.imports(&p.tree, prefix, absolute);
                prefix.pop();
            }
            syn::UseTree::Group(g) => {
                for child in &g.items {
                    self.imports(child, prefix, absolute)
                }
            }
            syn::UseTree::Name(n) => {
                let mut path = prefix.clone();
                if n.ident != "self" {
                    path.push(n.ident.unraw().to_string())
                }
                if let Some(Resolved::External(p)) =
                    self.target(&path, absolute, Namespace::Any, n.ident.span())
                {
                    self.rules(&p, n.ident.span())
                }
            }
            syn::UseTree::Rename(n) => {
                let mut path = prefix.clone();
                if n.ident != "self" {
                    path.push(n.ident.unraw().to_string())
                }
                if let Some(Resolved::External(p)) =
                    self.target(&path, absolute, Namespace::Any, n.ident.span())
                {
                    self.rules(&p, n.ident.span())
                }
            }
            syn::UseTree::Glob(g) => {
                if let Some(Resolved::External(p)) =
                    self.target(prefix, absolute, Namespace::Type, g.star_token.spans[0])
                {
                    let joined = normalize(&p).join("::");
                    for (rule, prefixes) in PATH_RULES {
                        if prefixes.iter().any(|q| {
                            matches_prefix(&joined, q) || q.starts_with(&format!("{joined}::"))
                        }) {
                            self.error(g.star_token.spans[0], rule, format!("{joined}::*"))
                        }
                    }
                    self.error(
                        g.star_token.spans[0],
                        "resolution-glob",
                        format!("external glob {joined}::* has no checked export set"),
                    );
                }
            }
        }
    }
    fn meta(&mut self, meta: &syn::Meta) {
        let path = meta.path();
        let Some(first) = path.segments.first() else {
            return;
        };
        if path.is_ident("cfg_attr") {
            let syn::Meta::List(list) = meta else { return };
            match Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
            {
                Ok(parts) => {
                    let mut parts = parts.iter();
                    if parts.next().is_some_and(meta_inactive) {
                        return;
                    }
                    for m in parts {
                        self.meta(m)
                    }
                }
                Err(_) => self.error(
                    first.ident.span(),
                    "resolution-attribute",
                    "unsupported cfg_attr syntax",
                ),
            }
            return;
        }
        if path.is_ident("derive") {
            let syn::Meta::List(list) = meta else { return };
            match Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
            {
                Ok(paths) => {
                    for path in paths {
                        let name = path
                            .segments
                            .first()
                            .map_or(String::new(), |s| s.ident.unraw().to_string());
                        if path.segments.len() != 1
                            || ![
                                "Clone",
                                "Copy",
                                "Debug",
                                "Default",
                                "Eq",
                                "PartialEq",
                                "Ord",
                                "PartialOrd",
                                "Hash",
                            ]
                            .contains(&name.as_str())
                        {
                            self.error(
                                first.ident.span(),
                                "resolution-macro",
                                format!("unsupported procedural derive {name}"),
                            );
                        } else {
                            match self.graph.lookup_macro(
                                self.scope,
                                &name,
                                position(first.ident.span()),
                                &mut BTreeSet::new(),
                            ) {
                                Ok(None) => {}
                                Ok(Some(Resolved::External(p)))
                                    if p.first().is_some_and(|root| {
                                        ["std", "core"].contains(&root.as_str())
                                    }) => {}
                                Ok(Some(_)) => self.error(
                                    first.ident.span(),
                                    "resolution-macro",
                                    format!("shadowed derive expansion {name}"),
                                ),
                                Err(e) => self.error(first.ident.span(), e.rule, e.message),
                            }
                        }
                    }
                }
                Err(_) => self.error(
                    first.ident.span(),
                    "resolution-macro",
                    "unsupported derive syntax",
                ),
            }
            return;
        }
        let known = [
            "cfg",
            "no_std",
            "no_main",
            "doc",
            "allow",
            "warn",
            "deny",
            "forbid",
            "must_use",
            "deprecated",
            "non_exhaustive",
            "repr",
            "inline",
            "cold",
            "track_caller",
            "no_mangle",
            "export_name",
            "link",
            "link_name",
            "macro_export",
            "automatically_derived",
        ];
        if path.is_ident("path") {
            self.error(
                first.ident.span(),
                "resolution-module-path",
                "path attribute is unsupported",
            )
        } else if path.leading_colon.is_none()
            && path.segments.len() == 2
            && first.ident == "rustfmt"
            && path.segments.last().is_some_and(|s| s.ident == "skip")
        {
            if let Err(error) = self
                .graph
                .compiler_tool(self.scope, position(first.ident.span()))
            {
                self.error(first.ident.span(), error.rule, error.message);
            }
        } else if !(path.leading_colon.is_none()
            && path.segments.len() == 1
            && known.contains(&first.ident.unraw().to_string().as_str()))
        {
            let name = path
                .segments
                .iter()
                .map(|s| s.ident.unraw().to_string())
                .collect::<Vec<_>>()
                .join("::");
            self.error(
                first.ident.span(),
                "resolution-attribute",
                format!("unsupported attribute expansion {name}"),
            )
        }
    }
}

struct Expressions(Punctuated<syn::Expr, syn::Token![,]>);
impl Parse for Expressions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self(Punctuated::parse_terminated(input)?))
    }
}
struct VectorArguments {
    values: Vec<syn::Expr>,
}
impl Parse for VectorArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut values = Vec::new();
        if input.is_empty() {
            return Ok(Self { values });
        }
        values.push(input.parse()?);
        if input.peek(syn::Token![;]) {
            let _: syn::Token![;] = input.parse()?;
            values.push(input.parse()?);
        } else {
            while !input.is_empty() {
                let _: syn::Token![,] = input.parse()?;
                if !input.is_empty() {
                    values.push(input.parse()?)
                }
            }
        }
        Ok(Self { values })
    }
}
struct MatchArguments {
    value: syn::Expr,
    pattern: syn::Pat,
    guard: Option<syn::Expr>,
}
impl Parse for MatchArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let value = input.parse()?;
        let _: syn::Token![,] = input.parse()?;
        let pattern = syn::Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(syn::Token![if]) {
            let _: syn::Token![if] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
        }
        Ok(Self {
            value,
            pattern,
            guard,
        })
    }
}

impl Scan<'_, '_> {
    fn macro_name(&mut self, mac: &syn::Macro) -> Option<String> {
        let names: Vec<String> = mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.unraw().to_string())
            .collect();
        let first = mac.path.segments.first()?;
        let target = if names.len() == 1 && mac.path.leading_colon.is_none() {
            match self.graph.lookup_macro(
                self.scope,
                &names[0],
                position(first.ident.span()),
                &mut BTreeSet::new(),
            ) {
                Ok(None) => return Some(names[0].clone()),
                Ok(Some(target)) => target,
                Err(e) => {
                    self.error(first.ident.span(), e.rule, e.message);
                    return None;
                }
            }
        } else {
            self.target(
                &names,
                mac.path.leading_colon.is_some(),
                Namespace::Macro,
                first.ident.span(),
            )?
        };
        match target {
            Resolved::External(path) => {
                self.rules(&path, first.ident.span());
                if path
                    .first()
                    .is_some_and(|p| ["std", "core", "alloc"].contains(&p.as_str()))
                {
                    return path.last().cloned();
                }
                self.error(
                    first.ident.span(),
                    "resolution-macro",
                    format!("unsupported external macro {}", path.join("::")),
                );
            }
            _ => self.error(
                first.ident.span(),
                "resolution-macro",
                format!("unsupported user macro {}", names.join("::")),
            ),
        }
        None
    }
    fn macro_arguments(&mut self, mac: &syn::Macro, name: &str) {
        let span = mac
            .path
            .segments
            .first()
            .map_or(Span::call_site(), |s| s.ident.span());
        if ["print", "println", "eprint", "eprintln", "dbg"].contains(&name) {
            self.error(span, "io", format!("{name}!"));
            return;
        }
        if ["env", "option_env"].contains(&name) {
            self.error(span, "environment", format!("{name}!"));
            return;
        }
        if ["thread_local", "lazy_static"].contains(&name) {
            self.error(span, "shared-state", format!("{name}!"));
            return;
        }
        if ["asm", "global_asm", "naked_asm"].contains(&name) {
            self.error(span, "unsafe", format!("{name}!"));
            return;
        }
        match name {
            "include" => {
                self.error(
                    span,
                    "resolution-source",
                    "include! code expansion is outside declared modules",
                );
                self.error(
                    span,
                    "resolution-macro",
                    "include! code expansion is outside declared modules",
                );
            }
            "include_str" | "include_bytes" => {
                if syn::parse2::<syn::LitStr>(mac.tokens.clone()).is_err() {
                    self.error(
                        span,
                        "resolution-macro",
                        format!("{name}! requires one literal immutable data path"),
                    );
                }
            }
            "stringify" => {}
            "cfg" => {
                if syn::parse2::<syn::Meta>(mac.tokens.clone()).is_err() {
                    self.error(span, "resolution-macro", "unsupported cfg! predicate")
                }
            }
            "vec" => match syn::parse2::<VectorArguments>(mac.tokens.clone()) {
                Ok(args) => {
                    for expression in &args.values {
                        self.expanded_expression(expression)
                    }
                }
                Err(_) => self.error(span, "resolution-macro", "unsupported vec! arguments"),
            },
            "matches" => match syn::parse2::<MatchArguments>(mac.tokens.clone()) {
                Ok(args) => {
                    self.expanded_expression(&args.value);
                    self.visit_pat(&args.pattern);
                    struct Names(BTreeSet<String>);
                    impl<'ast> Visit<'ast> for Names {
                        fn visit_pat_ident(&mut self, p: &'ast syn::PatIdent) {
                            self.0.insert(p.ident.unraw().to_string());
                            visit::visit_pat_ident(self, p)
                        }
                    }
                    let mut names = Names(BTreeSet::new());
                    names.visit_pat(&args.pattern);
                    self.temporary.push(names.0);
                    if let Some(guard) = &args.guard {
                        self.expanded_expression(guard)
                    }
                    self.temporary.pop();
                }
                Err(_) => self.error(span, "resolution-macro", "unsupported matches! arguments"),
            },
            "concat" | "format" | "format_args" | "format_args_nl" | "write" | "writeln"
            | "panic" | "assert" | "assert_eq" | "assert_ne" | "debug_assert"
            | "debug_assert_eq" | "debug_assert_ne" | "todo" | "unimplemented" | "unreachable" => {
                match syn::parse2::<Expressions>(mac.tokens.clone()) {
                    Ok(args) => {
                        for expression in &args.0 {
                            if let syn::Expr::Lit(literal) = expression
                                && let syn::Lit::Str(string) = &literal.lit
                                && super::address_format(&string.value())
                            {
                                self.error(string.span(), "address", "pointer formatting")
                            }
                            self.expanded_expression(expression);
                        }
                    }
                    Err(_) => self.error(
                        span,
                        "resolution-macro",
                        format!("unsupported {name}! argument syntax"),
                    ),
                }
            }
            _ => self.error(
                span,
                "resolution-macro",
                format!("unsupported macro expansion {name}!"),
            ),
        }
    }
    fn path_arguments(&mut self, path: &syn::Path) {
        for segment in &path.segments {
            self.visit_path_arguments(&segment.arguments)
        }
    }
    fn expanded_expression(&mut self, expression: &syn::Expr) {
        struct HasScope(bool);
        impl<'ast> Visit<'ast> for HasScope {
            fn visit_block(&mut self, _: &'ast syn::Block) {
                self.0 = true
            }
            fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {
                self.0 = true
            }
            fn visit_arm(&mut self, _: &'ast syn::Arm) {
                self.0 = true
            }
        }
        let mut scope = HasScope(false);
        scope.visit_expr(expression);
        if !scope.0 {
            self.visit_expr(expression);
            return;
        }
        // Parsed arguments of fixed compiler built-ins are source expressions,
        // not arbitrary expansion output. Index their actual lexical scopes in
        // a temporary graph, retaining the importing scope and full rule set.
        let mut graph: Graph<'_> = self.graph.clone();
        let mut index = Index {
            graph: &mut graph,
            scope: self.scope,
            source: self.source,
        };
        index.visit_expr(expression);
        let mut scanner = Scan {
            graph: &graph,
            scope: self.scope,
            source: self.source,
            findings: Vec::new(),
            temporary: self.temporary.clone(),
        };
        scanner.visit_expr(expression);
        self.findings.extend(scanner.findings);
    }
}

impl<'ast> Visit<'ast> for Scan<'_, '_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_attrs(item).iter().any(inactive_cfg) {
            return;
        }
        if matches!(item, syn::Item::Verbatim(_)) {
            self.error(
                Span::call_site(),
                "resolution-syntax",
                "unsupported item syntax",
            );
            return;
        }
        visit::visit_item(self, item);
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        self.meta(&attr.meta)
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.path(path, Namespace::Type);
        self.path_arguments(path)
    }
    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        for attr in &expr.attrs {
            self.visit_attribute(attr)
        }
        if let Some(qself) = &expr.qself {
            self.visit_type(&qself.ty)
        }
        self.path(&expr.path, Namespace::Value);
        self.path_arguments(&expr.path);
    }
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if matches!(expr, syn::Expr::Verbatim(_)) {
            self.error(
                Span::call_site(),
                "resolution-syntax",
                "unsupported expression syntax",
            );
            return;
        }
        // Expression cfg is only supported where syn exposes its attributes.
        let attrs = match expr {
            syn::Expr::Macro(x) => &x.attrs,
            syn::Expr::Path(x) => &x.attrs,
            syn::Expr::Block(x) => &x.attrs,
            syn::Expr::Call(x) => &x.attrs,
            syn::Expr::MethodCall(x) => &x.attrs,
            syn::Expr::If(x) => &x.attrs,
            syn::Expr::Match(x) => &x.attrs,
            syn::Expr::Unsafe(x) => &x.attrs,
            syn::Expr::Async(x) => &x.attrs,
            _ => return visit::visit_expr(self, expr),
        };
        if attrs.iter().any(inactive_cfg) {
            return;
        }
        visit::visit_expr(self, expr);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for attr in &item.attrs {
            self.visit_attribute(attr)
        }
        self.imports(&item.tree, &mut Vec::new(), item.leading_colon.is_some());
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        for attr in &item.attrs {
            self.visit_attribute(attr)
        }
        if item.ident != "self"
            && let Some(Resolved::External(path)) = self.target(
                &[item.ident.unraw().to_string()],
                true,
                Namespace::Type,
                item.ident.span(),
            )
        {
            self.rules(&path, item.ident.span())
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        for attr in &item.attrs {
            self.visit_attribute(attr)
        }
        let old = self.scope;
        let source = self.source;
        let (line, column) = position(item.ident.span());
        if self
            .graph
            .inactive_modules
            .contains(&(source, line, column))
        {
            return;
        }
        let names = [item.ident.unraw().to_string()];
        if let Some(Resolved::Module(child)) =
            self.target(&names, false, Namespace::Type, item.ident.span())
        {
            self.scope = child;
            self.source = self.graph.scopes[child].source;
            if self.source != source {
                for attribute in &self.graph.sources[self.source].syntax.attrs {
                    self.visit_attribute(attribute)
                }
            }
            if let Some(items) = self.graph.modules.get(&child) {
                for child in *items {
                    self.visit_item(child)
                }
            }
        }
        self.scope = old;
        self.source = source;
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if let Some(old) = self.enter(item.sig.ident.span(), 2) {
            visit::visit_item_fn(self, item);
            self.scope = old
        }
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if item.attrs.iter().any(inactive_cfg) {
            return;
        }
        if let Some(old) = self.enter(item.sig.ident.span(), 2) {
            visit::visit_impl_item_fn(self, item);
            self.scope = old
        }
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if item.attrs.iter().any(inactive_cfg) {
            return;
        }
        if let Some(old) = self.enter(item.sig.ident.span(), 2) {
            visit::visit_trait_item_fn(self, item);
            self.scope = old
        }
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if let Some(token) = &item.unsafety {
            self.error(token.span, "unsafe", "unsafe impl")
        }
        if let Some(old) = self.enter(item.impl_token.span, 3) {
            visit::visit_item_impl(self, item);
            self.scope = old
        }
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if let Some(token) = &item.unsafety {
            self.error(token.span, "unsafe", "unsafe trait")
        }
        if let Some(old) = self.enter(item.ident.span(), 3) {
            visit::visit_item_trait(self, item);
            self.scope = old
        }
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if let Some(old) = self.enter(item.ident.span(), 8) {
            visit::visit_item_struct(self, item);
            self.scope = old
        }
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if let Some(old) = self.enter(item.ident.span(), 8) {
            visit::visit_item_enum(self, item);
            self.scope = old
        }
    }
    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        if let Some(old) = self.enter(item.ident.span(), 8) {
            visit::visit_item_union(self, item);
            self.scope = old
        }
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if let Some(old) = self.enter(item.ident.span(), 8) {
            visit::visit_item_type(self, item);
            self.scope = old
        }
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        if let Some(old) = self.enter(block.brace_token.span.open(), 4) {
            visit::visit_block(self, block);
            self.scope = old
        }
    }
    fn visit_expr_closure(&mut self, expr: &'ast syn::ExprClosure) {
        if let Some(old) = self.enter(expr.or1_token.span, 5) {
            visit::visit_expr_closure(self, expr);
            self.scope = old
        }
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if let Some(old) = self.enter(arm.fat_arrow_token.spans[0], 6) {
            visit::visit_arm(self, arm);
            self.scope = old
        }
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        if let Some(old) = self.enter(expr.for_token.span, 7) {
            self.visit_pat(&expr.pat);
            self.visit_block(&expr.body);
            self.scope = old
        }
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        if let Some(old) = self.enter(expr.if_token.span, 9) {
            self.visit_expr(&expr.cond);
            self.visit_block(&expr.then_branch);
            self.scope = old
        }
        if let Some((_, other)) = &expr.else_branch {
            self.visit_expr(other)
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        if let Some(old) = self.enter(expr.while_token.span, 9) {
            self.visit_expr(&expr.cond);
            self.visit_block(&expr.body);
            self.scope = old
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(name) = self.macro_name(mac) {
            self.macro_arguments(mac, &name)
        }
    }
    fn visit_stmt_macro(&mut self, item: &'ast syn::StmtMacro) {
        if item.attrs.iter().any(inactive_cfg) {
            return;
        }
        for attribute in &item.attrs {
            self.visit_attribute(attribute)
        }
        self.visit_macro(&item.mac);
    }
    fn visit_generic_argument(&mut self, argument: &'ast syn::GenericArgument) {
        if let syn::GenericArgument::Type(syn::Type::Path(path)) = argument
            && path.qself.is_none()
            && path.path.segments.len() == 1
        {
            self.path(&path.path, Namespace::Any);
            self.path_arguments(&path.path);
            return;
        }
        visit::visit_generic_argument(self, argument);
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        for attr in &item.attrs {
            self.visit_attribute(attr)
        }
        if item.ident.is_none() || !item.mac.path.is_ident("macro_rules") {
            self.visit_macro(&item.mac)
        }
        // Definitions cannot execute without an invocation; every invocation
        // of a user macro refuses, rather than interpreting arbitrary tokens.
    }
    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if let Some(token) = &signature.unsafety {
            self.error(token.span, "unsafe", "unsafe function")
        }
        if let Some(token) = &signature.asyncness {
            self.error(token.span, "threads", "async function")
        }
        if let Some(abi) = &signature.abi {
            self.error(abi.extern_token.span, "foreign-code", "extern function")
        }
        visit::visit_signature(self, signature);
    }
    fn visit_expr_async(&mut self, expr: &'ast syn::ExprAsync) {
        self.error(expr.async_token.span, "threads", "async block");
        visit::visit_expr_async(self, expr)
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.error(item.ident.span(), "shared-state", "mutable static")
        }
        visit::visit_item_static(self, item);
    }
    fn visit_expr_unsafe(&mut self, expr: &'ast syn::ExprUnsafe) {
        self.error(expr.unsafe_token.span, "unsafe", "unsafe block");
        visit::visit_expr_unsafe(self, expr)
    }
    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.error(item.abi.extern_token.span, "foreign-code", "extern block");
        visit::visit_item_foreign_mod(self, item)
    }
    fn visit_type_ptr(&mut self, pointer: &'ast syn::TypePtr) {
        self.error(pointer.star_token.spans[0], "address", "raw pointer type");
        visit::visit_type_ptr(self, pointer)
    }
    fn visit_expr_raw_addr(&mut self, expression: &'ast syn::ExprRawAddr) {
        self.error(expression.raw.span, "address", "raw borrow expression");
        visit::visit_expr_raw_addr(self, expression)
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if super::ADDRESS_METHODS.contains(&call.method.unraw().to_string().as_str()) {
            self.error(call.method.span(), "address", format!(".{}()", call.method))
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_lit_float(&mut self, literal: &'ast syn::LitFloat) {
        self.report(
            literal.span(),
            "floating-point",
            Severity::Warning,
            literal.base10_digits().to_string(),
        )
    }
}
