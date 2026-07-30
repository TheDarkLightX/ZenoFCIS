//! Deterministic accumulated diagnostics.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Half-open UTF-8 byte span with one-based display coordinates.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceSpan {
    start: u32,
    end: u32,
    line: u32,
    column: u32,
}

impl SourceSpan {
    /// Creates a span.
    #[must_use]
    pub const fn new(start: u32, end: u32, line: u32, column: u32) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Returns the zero-based start byte.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }
    /// Returns the zero-based exclusive end byte.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }
    /// Returns the one-based line.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }
    /// Returns the one-based byte column.
    #[must_use]
    pub const fn column(self) -> u32 {
        self.column
    }
}

/// Stable path into a parsed or elaborated project.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AstPath(Box<str>);

impl AstPath {
    /// Creates an owned path.
    #[must_use]
    pub fn new(path: impl Into<Box<str>>) -> Self {
        Self(path.into())
    }
    /// Returns the path text.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

/// Compiler stage producing a diagnostic.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStage {
    Lex,
    Parse,
    Elaborate,
    Lower,
    Evaluate,
}

impl DiagnosticStage {
    /// Returns the stable machine-readable stage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lex => "lex",
            Self::Parse => "parse",
            Self::Elaborate => "elaborate",
            Self::Lower => "lower",
            Self::Evaluate => "evaluate",
        }
    }
}

/// Closed RC3 diagnostic-code registry.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCode {
    SourceTooLarge,
    TokenLimit,
    UnexpectedCharacter,
    InvalidNumber,
    ExpectedToken,
    UnexpectedToken,
    MissingHeader,
    UnsupportedVersion,
    InvalidDeclaration,
    DuplicateId,
    DuplicateName,
    UnknownReference,
    InvalidPrecedence,
    InvalidMergeOrder,
    IncompatibleBackend,
    ResourceLimit,
    EmptyTrace,
    Indeterminate,
}

impl DiagnosticCode {
    /// Every code in stable registry order.
    pub const ALL: [Self; 18] = [
        Self::SourceTooLarge,
        Self::TokenLimit,
        Self::UnexpectedCharacter,
        Self::InvalidNumber,
        Self::ExpectedToken,
        Self::UnexpectedToken,
        Self::MissingHeader,
        Self::UnsupportedVersion,
        Self::InvalidDeclaration,
        Self::DuplicateId,
        Self::DuplicateName,
        Self::UnknownReference,
        Self::InvalidPrecedence,
        Self::InvalidMergeOrder,
        Self::IncompatibleBackend,
        Self::ResourceLimit,
        Self::EmptyTrace,
        Self::Indeterminate,
    ];

    /// Returns the stable public code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceTooLarge => "ZENO-E0001",
            Self::TokenLimit => "ZENO-E0002",
            Self::UnexpectedCharacter => "ZENO-E0003",
            Self::InvalidNumber => "ZENO-E0004",
            Self::ExpectedToken => "ZENO-E0101",
            Self::UnexpectedToken => "ZENO-E0102",
            Self::MissingHeader => "ZENO-E0103",
            Self::UnsupportedVersion => "ZENO-E0104",
            Self::InvalidDeclaration => "ZENO-E0105",
            Self::DuplicateId => "ZENO-E0201",
            Self::DuplicateName => "ZENO-E0202",
            Self::UnknownReference => "ZENO-E0203",
            Self::InvalidPrecedence => "ZENO-E0204",
            Self::InvalidMergeOrder => "ZENO-E0205",
            Self::IncompatibleBackend => "ZENO-E0206",
            Self::ResourceLimit => "ZENO-E0207",
            Self::EmptyTrace => "ZENO-E0301",
            Self::Indeterminate => "ZENO-E0302",
        }
    }
}

/// One complete actionable compiler diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    code: DiagnosticCode,
    stage: DiagnosticStage,
    path: AstPath,
    span: SourceSpan,
    expected: Box<str>,
    actual: Box<str>,
    remediation: Box<str>,
}

impl Diagnostic {
    /// Creates a diagnostic with all stable schema fields.
    #[must_use]
    pub fn new(
        code: DiagnosticCode,
        stage: DiagnosticStage,
        path: AstPath,
        span: SourceSpan,
        expected: impl Into<Box<str>>,
        actual: impl Into<Box<str>>,
        remediation: impl Into<Box<str>>,
    ) -> Self {
        Self {
            code,
            stage,
            path,
            span,
            expected: expected.into(),
            actual: actual.into(),
            remediation: remediation.into(),
        }
    }
    /// Returns the stable code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }
    /// Returns the producing stage.
    #[must_use]
    pub const fn stage(&self) -> DiagnosticStage {
        self.stage
    }
    /// Returns the AST path.
    #[must_use]
    pub const fn path(&self) -> &AstPath {
        &self.path
    }
    /// Returns the source span.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.span
    }
    /// Returns the expected value description.
    #[must_use]
    pub const fn expected(&self) -> &str {
        &self.expected
    }
    /// Returns the actual value description.
    #[must_use]
    pub const fn actual(&self) -> &str {
        &self.actual
    }
    /// Returns the deterministic remediation.
    #[must_use]
    pub const fn remediation(&self) -> &str {
        &self.remediation
    }
}

/// Sorted bounded collection of accumulated diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticSet {
    diagnostics: Box<[Diagnostic]>,
    truncated: bool,
}

impl DiagnosticSet {
    pub(crate) fn from_vec(mut diagnostics: Vec<Diagnostic>, limit: usize) -> Self {
        diagnostics.sort_by(|left, right| {
            (
                left.span,
                &left.path,
                left.code,
                left.stage,
                &left.expected,
                &left.actual,
            )
                .cmp(&(
                    right.span,
                    &right.path,
                    right.code,
                    right.stage,
                    &right.expected,
                    &right.actual,
                ))
        });
        diagnostics.dedup();
        let truncated = diagnostics.len() > limit;
        diagnostics.truncate(limit);
        Self {
            diagnostics: diagnostics.into_boxed_slice(),
            truncated,
        }
    }
    /// Returns diagnostics in canonical source-span, path, and code order.
    #[must_use]
    pub const fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Returns the number of retained diagnostics.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.diagnostics.len()
    }
    /// Returns whether the collection is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
    /// Returns whether additional diagnostics were omitted by the bound.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Display for DiagnosticSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index != 0 {
                formatter.write_str("\n")?;
            }
            write!(
                formatter,
                "{} {}:{}:{} {} expected {}; got {}; {}",
                diagnostic.code.as_str(),
                diagnostic.stage.as_str(),
                diagnostic.span.line,
                diagnostic.span.column,
                diagnostic.path.as_str(),
                diagnostic.expected,
                diagnostic.actual,
                diagnostic.remediation
            )?;
        }
        if self.truncated {
            formatter.write_str("\nadditional diagnostics omitted by limit")?;
        }
        Ok(())
    }
}

pub(crate) fn text(value: impl fmt::Display) -> Box<str> {
    let mut output = String::new();
    use core::fmt::Write as _;
    let _ = write!(output, "{value}");
    output.into_boxed_str()
}
