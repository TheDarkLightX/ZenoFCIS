//! Table syntax is preparation only: it produces the existing scalar tree.
use super::{Ast, Binary, Parser};
use zeno_fcis_spec::{
    DeclaredDomain, ProjectLimits, ProjectSpec, ProjectionPath, ProjectionRoot, SourceLimits,
    StableId, TableInfo, elaborate_project, expand_tables, parse_project,
};

pub(in crate::internals) struct TableDeclarations {
    tables: Vec<TableInfo>,
    project: ProjectSpec,
}

impl TableDeclarations {
    pub(in crate::internals) fn read(source: &str) -> Result<Self, String> {
        let limits = SourceLimits::default();
        let tables = expand_tables(source, limits)
            .map_err(|e| e.to_string())?
            .tables;
        let project = elaborate_project(
            parse_project(source, limits).map_err(|e| e.to_string())?,
            ProjectLimits::default(),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { tables, project })
    }
    fn table(&self, id: u32) -> Result<&TableInfo, String> {
        self.tables
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("table {id} is not declared"))
    }
    fn projection(&self, name: &str) -> Result<(&TableInfo, String), String> {
        let parts: Vec<_> = name.split('.').collect();
        let [root, owner, table] = parts.as_slice() else {
            return Err("a table read is ROOT.OWNER.TABLE[index]".to_owned());
        };
        if !matches!(*root, "pre" | "command" | "context") {
            return Err("table reads use pre, command or context".to_owned());
        }
        let table = self.table(table.parse().map_err(|_| "invalid table ID")?)?;
        if owner.parse::<u32>() != Ok(table.owner) {
            return Err("the table belongs to a different record".to_owned());
        }
        Ok((table, format!("{root}.{owner}")))
    }
    pub(super) fn index(&self, text: &str, size: u32) -> Result<Ast, String> {
        if let Ok(index) = text.parse::<i128>() {
            return if (0..i128::from(size)).contains(&index) {
                Ok(Ast::Int(index))
            } else {
                Err(format!("table index must lie in 0..{}", size - 1))
            };
        }
        let mut words = text.split('.');
        let root = match words.next() {
            Some("pre") => ProjectionRoot::Pre,
            Some("command") => ProjectionRoot::Command,
            Some("context") => ProjectionRoot::Context,
            _ => {
                return Err(
                    "a table index is a literal, bound index or input projection".to_owned(),
                );
            }
        };
        let segments = words
            .map(|word| word.parse::<u32>().ok().and_then(StableId::new))
            .collect::<Option<Vec<_>>>()
            .ok_or("invalid input index path")?;
        let path = ProjectionPath::try_new(root, segments).ok_or("invalid input index path")?;
        let valid = match zeno_fcis_spec::declared_domain(&self.project, &path) {
            Some(DeclaredDomain::Range(range)) => {
                range.min() >= 0 && range.max() < i128::from(size)
            }
            Some(DeclaredDomain::Values(values)) => {
                !values.is_empty() && values.iter().all(|v| (0..i128::from(size)).contains(v))
            }
            None => false,
        };
        if !valid {
            return Err(format!("input index range must lie in 0..{}", size - 1));
        }
        Ok(Ast::Name(text.to_owned()))
    }
    pub(in crate::internals) fn writes(
        &self,
        key: &str,
        value: Ast,
    ) -> Result<Vec<(u16, Ast)>, String> {
        let (id, rest) = key
            .split_once('[')
            .ok_or("indexed write needs TABLE[index]")?;
        let index = rest
            .strip_suffix(']')
            .ok_or("indexed write needs closing ]")?;
        let table = self.table(id.parse().map_err(|_| "invalid table ID")?)?;
        if table.owner != 100 {
            return Err("indexed post write must name a state table".to_owned());
        }
        let index = self.index(index, table.size)?;
        if super::render(&value)
            .len()
            .checked_mul(table.size as usize)
            .is_none_or(|bytes| bytes > 1 << 20)
        {
            return Err("table rule expansion exceeds text budget".to_owned());
        }
        let mut fields = Vec::new();
        for k in 0..table.size {
            let field = u16::try_from(table.id + k)
                .map_err(|_| "table field exceeds contract field IDs")?;
            let written = match &index {
                Ast::Int(selected) if *selected == i128::from(k) => value.clone(),
                Ast::Int(_) => continue,
                _ => Ast::Choose(
                    Box::new(Ast::Binary(
                        Binary::Eq,
                        Box::new(index.clone()),
                        Box::new(Ast::Int(i128::from(k))),
                    )),
                    Box::new(value.clone()),
                    Box::new(Ast::Name(format!("pre.100.{}", table.id + k))),
                ),
            };
            fields.push((field, written));
        }
        Ok(fields)
    }
}

impl<'t> Parser<'t> {
    pub(super) fn table_name(&mut self, name: &'t str) -> Result<Ast, String> {
        self.expansion_work(name.len())?;
        if self.tokens.get(self.index) != Some(&"[") {
            return Ok(self
                .bound
                .iter()
                .rev()
                .find(|(n, _)| *n == name)
                .map_or_else(|| Ast::Name(name.to_owned()), |(_, k)| Ast::Int(*k)));
        }
        self.index += 1;
        let index = self.next()?;
        self.expect("]")?;
        let declarations = self
            .tables
            .ok_or("table reads require project declarations")?;
        let (table, prefix) = declarations.projection(name)?;
        let index = match self.bound.iter().rev().find(|(n, _)| *n == index) {
            Some((_, k)) => declarations.index(&k.to_string(), table.size)?,
            None => declarations.index(index, table.size)?,
        };
        let field = |k| Ast::Name(format!("{prefix}.{}", table.id + k));
        if let Ast::Int(k) = index {
            return Ok(field(k as u32));
        }
        self.expansion_work(table.size as usize * 4)?;
        let mut read = field(table.size - 1);
        for k in (0..table.size - 1).rev() {
            read = Ast::Choose(
                Box::new(Ast::Binary(
                    Binary::Eq,
                    Box::new(index.clone()),
                    Box::new(Ast::Int(i128::from(k))),
                )),
                Box::new(field(k)),
                Box::new(read),
            );
        }
        Ok(read)
    }
    pub(super) fn table_form(&mut self, form: &str, depth: usize) -> Result<Ast, String> {
        let variable = self.next()?;
        if variable.contains('.')
            || !variable
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            || self.bound.iter().any(|(name, _)| *name == variable)
            || matches!(
                variable,
                "pre" | "command" | "context" | "post" | "outbox" | "true" | "false"
            )
        {
            return Err("table form needs a fresh index variable".to_owned());
        }
        self.expect("in")?;
        self.expect("table")?;
        let id = self.next()?.parse().map_err(|_| "invalid table ID")?;
        let size = self
            .tables
            .ok_or("table forms require project declarations")?
            .table(id)?
            .size;
        let start = self.index;
        let mut total = None;
        for k in 0..size {
            self.index = start;
            self.bound.push((variable, i128::from(k)));
            let condition = if self.tokens.get(self.index) == Some(&"where") {
                self.index += 1;
                Some(self.expression(0, depth + 1)?)
            } else {
                None
            };
            self.expect("{")?;
            let mut body = self.expression(0, depth + 1)?;
            self.expect("}")?;
            self.bound.pop();
            if let Some(condition) = condition {
                body = match form {
                    "sum" => {
                        Ast::Choose(Box::new(condition), Box::new(body), Box::new(Ast::Int(0)))
                    }
                    _ => Ast::Binary(
                        if form == "forall" {
                            Binary::Implies
                        } else {
                            Binary::And
                        },
                        Box::new(condition),
                        Box::new(body),
                    ),
                };
            }
            total = Some(match total {
                None => body,
                Some(previous) => Ast::Binary(
                    match form {
                        "forall" => Binary::And,
                        "exists" => Binary::Or,
                        _ => Binary::Add,
                    },
                    Box::new(previous),
                    Box::new(body),
                ),
            });
            self.expansion_work(2)?;
        }
        total.ok_or_else(|| "empty table".to_owned())
    }
    pub(super) fn expansion_work(&mut self, nodes: usize) -> Result<(), String> {
        if self.tables.is_some() {
            self.expanded = self
                .expanded
                .checked_add(nodes)
                .ok_or("table rule expansion exceeds work cap")?;
            if self.expanded > 65_536 {
                return Err("table rule expansion exceeds work cap".to_owned());
            }
        }
        Ok(())
    }
}
