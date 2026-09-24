//! Resolution of law and claim projection paths against declared types and
//! fields.
//!
//! Elaboration does not check that a formula path names declared schema
//! elements, so a mistyped segment elaborates silently and becomes a missing
//! observation at evaluation time. This module reports such paths. It changes
//! no canonical bytes and no elaboration result.
//!
//! A path resolves when its first segment is a declared type of the kind its
//! root reads (`pre` and `post` read state types, `command` a command type, and
//! `context` a context type) and each later segment is a field of the type
//! before it. The `effects`, `outbox`, and `events` roots have no declared
//! segment rules, so their paths are reported as unchecked, not as resolved.

use alloc::vec::Vec;

use crate::{
    ClaimDecl, ClaimFormula, LawDecl, ProjectSpec, ProjectionPath, ProjectionRoot, RelExpr,
    StableId, TemporalFormula, TypeKind, ValueExpr,
};

/// How one projection path relates to the declared types and fields.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathResolution {
    /// The first segment is a declared type of the kind the root reads, and
    /// each later segment is a field of the type before it.
    Resolved,
    /// The root has no declared segment rules, so the path is not checked.
    Unchecked,
    /// The first segment is not a declared type of the kind the root reads.
    UnknownRootType {
        /// The unmatched first segment.
        segment: StableId,
    },
    /// A later segment is not a field of the type before it.
    UnknownField {
        /// The type the segment was looked up in.
        owner: StableId,
        /// The unmatched segment.
        segment: StableId,
    },
}

impl PathResolution {
    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Unchecked => "unchecked",
            Self::UnknownRootType { .. } => "unknown-root-type",
            Self::UnknownField { .. } => "unknown-field",
        }
    }

    /// Returns true when the path names something the project does not declare.
    #[must_use]
    pub const fn is_unresolved(self) -> bool {
        matches!(
            self,
            Self::UnknownRootType { .. } | Self::UnknownField { .. }
        )
    }
}

/// Returns the values an observation at `path` can take in any decision the
/// authority admits, when `project.zeno` alone determines them: the variant
/// IDs of an enumerated type, or 0 and 1 for a bool.
///
/// Admission refuses a command, context, or state whose enumerated fields
/// hold undeclared variants, and law checkers observe an enumerated field as
/// its variant ID and a bool as 0 or 1. Returns `None` for an integer, whose
/// range is supplied outside `project.zeno`, and for a path that does not
/// resolve to a declared field.
#[must_use]
pub fn declared_domain(spec: &ProjectSpec, path: &ProjectionPath) -> Option<Vec<i128>> {
    let leaf = leaf_type(spec, path)?;
    let mut variants: Vec<i128> = spec
        .variants()
        .iter()
        .filter(|variant| variant.owner() == leaf)
        .map(|variant| i128::from(variant.id().get()))
        .collect();
    if !variants.is_empty() {
        variants.sort_unstable();
        return Some(variants);
    }
    spec.types()
        .iter()
        .find(|declared| declared.id() == leaf && declared.kind() == TypeKind::Bool)
        .map(|_| alloc::vec![0, 1])
}

/// The declared type of the value at `path`, when every segment resolves.
fn leaf_type(spec: &ProjectSpec, path: &ProjectionPath) -> Option<StableId> {
    if resolve_path(spec, path) != PathResolution::Resolved {
        return None;
    }
    let (first, rest) = path.segments().split_first()?;
    let mut owner = *first;
    for segment in rest {
        owner = spec
            .fields()
            .iter()
            .find(|field| field.id() == *segment && field.owner() == owner)?
            .field_type();
    }
    Some(owner)
}

/// Resolves one projection path against the project's declared types and
/// fields.
#[must_use]
pub fn resolve_path(spec: &ProjectSpec, path: &ProjectionPath) -> PathResolution {
    let kind = match path.root() {
        ProjectionRoot::Pre | ProjectionRoot::Post => TypeKind::State,
        ProjectionRoot::Command => TypeKind::Command,
        ProjectionRoot::Context => TypeKind::Context,
        ProjectionRoot::Effects | ProjectionRoot::Outbox | ProjectionRoot::Events => {
            return PathResolution::Unchecked;
        }
    };
    // Paths are nonempty by construction.
    let Some((first, rest)) = path.segments().split_first() else {
        return PathResolution::Resolved;
    };
    if !spec
        .types()
        .iter()
        .any(|declared| declared.id() == *first && declared.kind() == kind)
    {
        return PathResolution::UnknownRootType { segment: *first };
    }
    let mut owner = *first;
    for segment in rest {
        match spec
            .fields()
            .iter()
            .find(|field| field.id() == *segment && field.owner() == owner)
        {
            Some(field) => owner = field.field_type(),
            None => {
                return PathResolution::UnknownField {
                    owner,
                    segment: *segment,
                };
            }
        }
    }
    PathResolution::Resolved
}

/// Returns every projection path a law reads, in syntax order.
#[must_use]
pub fn law_paths(law: &LawDecl) -> Vec<&ProjectionPath> {
    let mut paths = Vec::new();
    relation_paths(law.formula(), &mut paths);
    paths
}

/// Returns every projection path a claim reads, in syntax order.
#[must_use]
pub fn claim_paths(claim: &ClaimDecl) -> Vec<&ProjectionPath> {
    let mut paths = Vec::new();
    match claim.formula() {
        ClaimFormula::Relational(expr) => relation_paths(expr, &mut paths),
        ClaimFormula::Temporal(formula) => temporal_paths(formula, &mut paths),
    }
    paths
}

fn temporal_paths<'a>(formula: &'a TemporalFormula, paths: &mut Vec<&'a ProjectionPath>) {
    match formula {
        TemporalFormula::Atom(expr) => relation_paths(expr, paths),
        TemporalFormula::Not(inner)
        | TemporalFormula::Next(inner)
        | TemporalFormula::Always(inner)
        | TemporalFormula::Eventually(inner) => temporal_paths(inner, paths),
        TemporalFormula::And(left, right)
        | TemporalFormula::Or(left, right)
        | TemporalFormula::Until(left, right) => {
            temporal_paths(left, paths);
            temporal_paths(right, paths);
        }
    }
}

fn relation_paths<'a>(expr: &'a RelExpr, paths: &mut Vec<&'a ProjectionPath>) {
    match expr {
        RelExpr::Bool(_) => {}
        RelExpr::Not(inner) => relation_paths(inner, paths),
        RelExpr::And(left, right) | RelExpr::Or(left, right) | RelExpr::Implies(left, right) => {
            relation_paths(left, paths);
            relation_paths(right, paths);
        }
        RelExpr::Compare(_, left, right) => {
            value_paths(left, paths);
            value_paths(right, paths);
        }
        RelExpr::Predicate { arguments, .. } => {
            for argument in arguments {
                value_paths(argument, paths);
            }
        }
        RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => relation_paths(body, paths),
    }
}

fn value_paths<'a>(expr: &'a ValueExpr, paths: &mut Vec<&'a ProjectionPath>) {
    match expr {
        ValueExpr::Int(_) | ValueExpr::Var(_) => {}
        ValueExpr::Projection(path) => paths.push(path),
        ValueExpr::Add(left, right)
        | ValueExpr::Sub(left, right)
        | ValueExpr::Mul(left, right)
        | ValueExpr::Div(_, left, right) => {
            value_paths(left, paths);
            value_paths(right, paths);
        }
        ValueExpr::Sum { body, .. } => value_paths(body, paths),
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

    const DECLARATIONS: &str = "zeno 1;\nproject 1 paths;\n\
        type 100 state State;\ntype 101 command Command;\ntype 102 context Context;\n\
        type 103 data Inner;\ntype 105 int Count;\n\
        field 110 100 count 105;\nfield 111 100 inner 103;\nfield 112 103 depth 105;\n\
        field 120 101 amount 105;\n\
        reason 200 bad precedence 0;\n\
        component 300 machine { owns 100; reads pre.100; writes post.100; budget steps 10; }\n\
        merge [300];\n";

    fn spec(laws: &str) -> ProjectSpec {
        let source = alloc::format!("{DECLARATIONS}{laws}");
        let parsed =
            parse_project(&source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|set| panic!("{set}"))
    }

    fn id(value: u32) -> StableId {
        StableId::new(value).unwrap_or_else(|| unreachable!())
    }

    fn codes(spec: &ProjectSpec) -> Vec<(u32, PathResolution)> {
        spec.laws()
            .iter()
            .flat_map(|law| {
                law_paths(law)
                    .into_iter()
                    .map(|path| (law.id().get(), resolve_path(spec, path)))
            })
            .collect()
    }

    #[test]
    fn declared_paths_resolve_through_nested_field_types() {
        let spec = spec(
            "law 400 ok = pre.100.110 <= post.100.111.112 && command.101.120 >= 0 \
             && context.102 == 1 && post.100 == pre.100;\n",
        );
        let resolutions = codes(&spec);
        assert_eq!(resolutions.len(), 6);
        assert!(
            resolutions
                .iter()
                .all(|(_, resolution)| *resolution == PathResolution::Resolved)
        );
    }

    #[test]
    fn declared_domains_come_from_variants_and_bools_only() {
        let source = "zeno 1;\nproject 1 domains;\n\
            type 100 state State;\ntype 101 command Command;\ntype 102 context Context;\n\
            type 105 int Count;\ntype 106 bool Flag;\ntype 107 data Mode;\n\
            field 110 100 count 105;\nfield 111 100 mode 107;\nfield 130 102 admin 106;\n\
            variant 151 107 Fast none;\nvariant 150 107 Slow none;\n\
            variant 160 101 Go none;\nvariant 161 101 Stop none;\n\
            reason 200 bad precedence 0;\n\
            component 300 machine { owns 100; reads pre.100; writes post.100; budget steps 10; }\n\
            merge [300];\nlaw 400 ok = post.100.110 >= 0;\n";
        let parsed =
            parse_project(source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
        let spec = elaborate_project(parsed, ProjectLimits::default())
            .unwrap_or_else(|set| panic!("{set}"));
        let path = |root, segments: &[u32]| {
            ProjectionPath::try_new(root, segments.iter().map(|value| id(*value)).collect())
                .unwrap_or_else(|| unreachable!())
        };
        // Variant IDs, sorted, whichever state root observes them.
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Pre, &[100, 111])),
            Some(vec![150, 151])
        );
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Post, &[100, 111])),
            Some(vec![150, 151])
        );
        // A command type with variants is observed as its variant.
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Command, &[101])),
            Some(vec![160, 161])
        );
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Context, &[102, 130])),
            Some(vec![0, 1])
        );
        // Integer ranges come from outside project.zeno, and unresolved or
        // unchecked paths have no declared domain.
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Pre, &[100, 110])),
            None
        );
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Pre, &[100, 199])),
            None
        );
        assert_eq!(
            declared_domain(&spec, &path(ProjectionRoot::Outbox, &[300])),
            None
        );
    }

    #[test]
    fn undeclared_segments_are_reported_with_their_owner() {
        let spec = spec(
            "law 400 typo = post.100.999 == 0;\n\
             law 401 wrong_kind = pre.101 == 0 && command.100 == 0;\n\
             law 402 past_scalar = pre.100.110.5 == 0;\n\
             law 403 missing = context.777 == 0;\n",
        );
        assert_eq!(
            codes(&spec),
            vec![
                (
                    400,
                    PathResolution::UnknownField {
                        owner: id(100),
                        segment: id(999)
                    }
                ),
                (401, PathResolution::UnknownRootType { segment: id(101) }),
                (401, PathResolution::UnknownRootType { segment: id(100) }),
                (
                    402,
                    PathResolution::UnknownField {
                        owner: id(105),
                        segment: id(5)
                    }
                ),
                (403, PathResolution::UnknownRootType { segment: id(777) }),
            ]
        );
    }

    #[test]
    fn roots_without_segment_rules_are_unchecked_not_resolved() {
        let spec = spec("law 400 effects = effects.300 == 0 && outbox.301.1 == 0;\n");
        assert_eq!(
            codes(&spec),
            vec![
                (400, PathResolution::Unchecked),
                (400, PathResolution::Unchecked)
            ]
        );
        assert!(!PathResolution::Unchecked.is_unresolved());
    }

    #[test]
    fn claim_paths_include_temporal_and_quantified_bodies() {
        let spec = spec(
            "claim 500 eventually cvc5 relational = forall i in 0..2 { post.100.999 >= i };\n\
             claim 501 temporal all finite 2 = always atom(pre.100.110 <= post.100.110);\n",
        );
        let found: Vec<(u32, &'static str)> = spec
            .claims()
            .iter()
            .flat_map(|claim| {
                claim_paths(claim)
                    .into_iter()
                    .map(|path| (claim.id().get(), resolve_path(&spec, path).code()))
            })
            .collect();
        assert_eq!(
            found,
            vec![(500, "unknown-field"), (501, "resolved"), (501, "resolved")]
        );
    }

    #[test]
    fn resolution_codes_are_stable() {
        assert_eq!(PathResolution::Resolved.code(), "resolved");
        assert_eq!(PathResolution::Unchecked.code(), "unchecked");
        assert_eq!(
            PathResolution::UnknownRootType { segment: id(1) }.code(),
            "unknown-root-type"
        );
        assert_eq!(
            PathResolution::UnknownField {
                owner: id(1),
                segment: id(2)
            }
            .code(),
            "unknown-field"
        );
    }
}
