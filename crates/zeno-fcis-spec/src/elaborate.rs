//! Accumulating elaboration and canonical sorting.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;
use crate::diagnostic::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, SourceSpan, text,
};
use crate::{MAX_FINITE_HORIZON, ProjectLimits, ZENO_DSL_VERSION};

/// Resolves, type-checks, bounds, and canonically orders a parsed project.
pub fn elaborate_project(
    parsed: ParsedProject,
    limits: ProjectLimits,
) -> Result<ProjectSpec, DiagnosticSet> {
    let ParsedProject {
        version,
        project_id,
        name,
        declarations,
        merge_order,
        diagnostic_limit,
    } = parsed;
    let mut diagnostics = Vec::new();
    if version != ZENO_DSL_VERSION {
        push(
            &mut diagnostics,
            DiagnosticCode::UnsupportedVersion,
            SourceSpan::new(0, 0, 1, 1),
            "header.version",
            text(ZENO_DSL_VERSION),
            text(version),
            "parse and elaborate only language version 1",
        );
    }
    if declarations.len() > limits.max_declarations() {
        push(
            &mut diagnostics,
            DiagnosticCode::ResourceLimit,
            SourceSpan::new(0, 0, 1, 1),
            "project.declarations",
            text(limits.max_declarations()),
            text(declarations.len()),
            "reduce the declaration inventory",
        );
    }

    let (
        mut namespaces,
        mut types,
        mut fields,
        mut variants,
        mut reasons,
        mut effects,
        mut channels,
        mut components,
        mut wirings,
        mut laws,
        mut claims,
    ) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut spans = BTreeMap::new();
    for spanned in declarations {
        match spanned.declaration {
            Declaration::Namespace(value) => {
                spans.insert((0, value.id().get()), spanned.span);
                namespaces.push(value)
            }
            Declaration::Type(value) => {
                spans.insert((1, value.id().get()), spanned.span);
                types.push(value)
            }
            Declaration::Field(value) => {
                spans.insert((2, value.id().get()), spanned.span);
                fields.push(value)
            }
            Declaration::Variant(value) => {
                spans.insert((3, value.id().get()), spanned.span);
                variants.push(value)
            }
            Declaration::Reason(value) => {
                spans.insert((4, value.id().get()), spanned.span);
                reasons.push(value)
            }
            Declaration::Effect(value) => {
                spans.insert((5, value.id().get()), spanned.span);
                effects.push(value)
            }
            Declaration::Channel(value) => {
                spans.insert((6, value.id().get()), spanned.span);
                channels.push(value)
            }
            Declaration::Component(value) => {
                spans.insert((7, value.id().get()), spanned.span);
                components.push(value)
            }
            Declaration::Wiring(value) => {
                spans.insert((8, wirings.len() as u32), spanned.span);
                wirings.push(value)
            }
            Declaration::Law(value) => {
                spans.insert((9, value.id().get()), spanned.span);
                laws.push(value)
            }
            Declaration::Claim(value) => {
                spans.insert((10, value.id().get()), spanned.span);
                claims.push(value)
            }
        }
    }

    sort_and_duplicates(
        &mut namespaces,
        |v| v.id(),
        |v| v.name(),
        "namespace",
        0,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut types,
        |v| v.id(),
        |v| v.name(),
        "type",
        1,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut fields,
        |v| v.id(),
        |v| v.name(),
        "field",
        2,
        &spans,
        &mut diagnostics,
    );
    fields.sort_by_key(|value| (value.owner(), value.id()));
    sort_and_duplicates(
        &mut variants,
        |v| v.id(),
        |v| v.name(),
        "variant",
        3,
        &spans,
        &mut diagnostics,
    );
    variants.sort_by_key(|value| (value.owner(), value.id()));
    sort_and_duplicates(
        &mut reasons,
        |v| v.id(),
        |v| v.name(),
        "reason",
        4,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut effects,
        |v| v.id(),
        |v| v.name(),
        "effect",
        5,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut channels,
        |v| v.id(),
        |v| v.name(),
        "channel",
        6,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut components,
        |v| v.id(),
        |v| v.name(),
        "component",
        7,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut laws,
        |v| v.id(),
        |v| v.name(),
        "law",
        9,
        &spans,
        &mut diagnostics,
    );
    sort_and_duplicates(
        &mut claims,
        |v| v.id(),
        |v| v.name(),
        "claim",
        10,
        &spans,
        &mut diagnostics,
    );

    let type_map: BTreeMap<StableId, TypeKind> = types
        .iter()
        .map(|value| (value.id(), value.kind()))
        .collect();
    for value in &fields {
        require_type(
            &type_map,
            value.owner(),
            span_for(&spans, 2, value.id()),
            format!("field.{}.owner", value.id().get()),
            &mut diagnostics,
        );
        require_type(
            &type_map,
            value.field_type(),
            span_for(&spans, 2, value.id()),
            format!("field.{}.type", value.id().get()),
            &mut diagnostics,
        );
    }
    for value in &variants {
        require_type(
            &type_map,
            value.owner(),
            span_for(&spans, 3, value.id()),
            format!("variant.{}.owner", value.id().get()),
            &mut diagnostics,
        );
        if let Some(payload) = value.payload_type() {
            require_type(
                &type_map,
                payload,
                span_for(&spans, 3, value.id()),
                format!("variant.{}.payload", value.id().get()),
                &mut diagnostics,
            );
        }
    }
    for value in &effects {
        require_kind(
            &type_map,
            value.destination_type(),
            TypeKind::Destination,
            span_for(&spans, 5, value.id()),
            format!("effect.{}.destination", value.id().get()),
            &mut diagnostics,
        );
        require_kind(
            &type_map,
            value.payload_type(),
            TypeKind::Payload,
            span_for(&spans, 5, value.id()),
            format!("effect.{}.payload", value.id().get()),
            &mut diagnostics,
        );
    }
    for value in &channels {
        require_kind(
            &type_map,
            value.destination_type(),
            TypeKind::Destination,
            span_for(&spans, 6, value.id()),
            format!("channel.{}.destination", value.id().get()),
            &mut diagnostics,
        );
        require_kind(
            &type_map,
            value.payload_type(),
            TypeKind::Payload,
            span_for(&spans, 6, value.id()),
            format!("channel.{}.payload", value.id().get()),
            &mut diagnostics,
        );
    }

    let mut ranks: Vec<u32> = reasons.iter().map(ReasonDecl::precedence).collect();
    ranks.sort_unstable();
    let expected: Vec<u32> = (0..u32::try_from(reasons.len()).unwrap_or(u32::MAX)).collect();
    if ranks != expected {
        push(
            &mut diagnostics,
            DiagnosticCode::InvalidPrecedence,
            SourceSpan::new(0, 0, 1, 1),
            "reasons.precedence",
            "each rank from 0 through reason_count-1 exactly once",
            format!("{ranks:?}"),
            "assign an explicit total zero-based precedence",
        );
    }

    if components.len() > limits.max_components() {
        push(
            &mut diagnostics,
            DiagnosticCode::ResourceLimit,
            SourceSpan::new(0, 0, 1, 1),
            "components",
            text(limits.max_components()),
            text(components.len()),
            "reduce the component count",
        );
    }
    let claim_ids: BTreeSet<StableId> = claims.iter().map(ClaimDecl::id).collect();
    for component in &mut components {
        let component_span = span_for(&spans, 7, component.id());
        if component.ports.len() > limits.max_ports_per_component() {
            push(
                &mut diagnostics,
                DiagnosticCode::ResourceLimit,
                component_span,
                format!("component.{}.ports", component.id().get()),
                text(limits.max_ports_per_component()),
                text(component.ports.len()),
                "reduce the port inventory",
            );
        }
        component.owned_state.sort_unstable();
        duplicate_ids(
            &component.owned_state,
            component_span,
            format!("component.{}.owns", component.id().get()),
            &mut diagnostics,
        );
        for state in component.owned_state.iter().copied() {
            require_kind(
                &type_map,
                state,
                TypeKind::State,
                component_span,
                format!("component.{}.owns", component.id().get()),
                &mut diagnostics,
            );
        }
        component.ports.sort_by_key(PortDecl::id);
        for pair in component.ports.windows(2) {
            if pair[0].id() == pair[1].id() {
                push(
                    &mut diagnostics,
                    DiagnosticCode::DuplicateId,
                    component_span,
                    format!("component.{}.ports", component.id().get()),
                    "unique port ID",
                    text(pair[0].id().get()),
                    "allocate one stable ID per port",
                );
            }
        }
        for port in component.ports.iter() {
            require_type(
                &type_map,
                port.payload_type(),
                component_span,
                format!(
                    "component.{}.port.{}.type",
                    component.id().get(),
                    port.id().get()
                ),
                &mut diagnostics,
            );
        }
        let mut normalized_footprints = component.footprints.to_vec();
        normalized_footprints.sort_by(|a, b| (a.kind(), a.path()).cmp(&(b.kind(), b.path())));
        normalized_footprints.dedup();
        component.footprints = normalized_footprints.into_boxed_slice();
        component.budgets.sort_by_key(|value| value.resource());
        for pair in component.budgets.windows(2) {
            if pair[0].resource() == pair[1].resource() {
                push(
                    &mut diagnostics,
                    DiagnosticCode::DuplicateId,
                    component_span,
                    format!("component.{}.budgets", component.id().get()),
                    "one budget per resource",
                    format!("{:?}", pair[0].resource()),
                    "combine duplicate resource budgets",
                );
            }
        }
        component.assumptions.sort_unstable();
        component.guarantees.sort_unstable();
        duplicate_ids(
            &component.assumptions,
            component_span,
            format!("component.{}.assumptions", component.id().get()),
            &mut diagnostics,
        );
        duplicate_ids(
            &component.guarantees,
            component_span,
            format!("component.{}.guarantees", component.id().get()),
            &mut diagnostics,
        );
        for claim in component
            .assumptions
            .iter()
            .chain(component.guarantees.iter())
        {
            if !claim_ids.contains(claim) {
                push(
                    &mut diagnostics,
                    DiagnosticCode::UnknownReference,
                    component_span,
                    format!("component.{}.claim", component.id().get()),
                    "declared claim ID",
                    text(claim.get()),
                    "declare the referenced claim",
                );
            }
        }
    }

    let component_map: BTreeMap<StableId, &ComponentDecl> =
        components.iter().map(|value| (value.id(), value)).collect();
    wirings.sort_unstable();
    for pair in wirings.windows(2) {
        if pair[0] == pair[1] {
            push(
                &mut diagnostics,
                DiagnosticCode::DuplicateId,
                SourceSpan::new(0, 0, 1, 1),
                "composition.wiring",
                "unique wiring",
                "duplicate wiring",
                "remove the duplicate edge",
            );
        }
    }
    for wiring in &wirings {
        let source = component_map.get(&wiring.source_component());
        let destination = component_map.get(&wiring.destination_component());
        if source.is_none() {
            push(
                &mut diagnostics,
                DiagnosticCode::UnknownReference,
                SourceSpan::new(0, 0, 1, 1),
                "wire.source.component",
                "declared component",
                text(wiring.source_component().get()),
                "declare the source component",
            );
        }
        if destination.is_none() {
            push(
                &mut diagnostics,
                DiagnosticCode::UnknownReference,
                SourceSpan::new(0, 0, 1, 1),
                "wire.destination.component",
                "declared component",
                text(wiring.destination_component().get()),
                "declare the destination component",
            );
        }
        let source_port = source.and_then(|value| {
            value
                .ports()
                .iter()
                .find(|port| port.id() == wiring.source_port())
        });
        let destination_port = destination.and_then(|value| {
            value
                .ports()
                .iter()
                .find(|port| port.id() == wiring.destination_port())
        });
        match source_port {
            Some(port) if port.direction() == PortDirection::Output => {}
            Some(_) => push(
                &mut diagnostics,
                DiagnosticCode::InvalidDeclaration,
                SourceSpan::new(0, 0, 1, 1),
                "wire.source.port",
                "output port",
                text(wiring.source_port().get()),
                "wire only from an output port",
            ),
            None => push(
                &mut diagnostics,
                DiagnosticCode::UnknownReference,
                SourceSpan::new(0, 0, 1, 1),
                "wire.source.port",
                "declared source port",
                text(wiring.source_port().get()),
                "declare the source port",
            ),
        }
        match destination_port {
            Some(port) if port.direction() == PortDirection::Input => {}
            Some(_) => push(
                &mut diagnostics,
                DiagnosticCode::InvalidDeclaration,
                SourceSpan::new(0, 0, 1, 1),
                "wire.destination.port",
                "input port",
                text(wiring.destination_port().get()),
                "wire only to an input port",
            ),
            None => push(
                &mut diagnostics,
                DiagnosticCode::UnknownReference,
                SourceSpan::new(0, 0, 1, 1),
                "wire.destination.port",
                "declared destination port",
                text(wiring.destination_port().get()),
                "declare the destination port",
            ),
        }
        if let (Some(left), Some(right)) = (source_port, destination_port)
            && left.payload_type() != right.payload_type()
        {
            push(
                &mut diagnostics,
                DiagnosticCode::InvalidDeclaration,
                SourceSpan::new(0, 0, 1, 1),
                "wire.payload",
                "equal port payload type IDs",
                format!(
                    "{} != {}",
                    left.payload_type().get(),
                    right.payload_type().get()
                ),
                "use ports with the same explicit payload type",
            );
        }
    }

    let canonical_component_ids: Vec<StableId> = components.iter().map(ComponentDecl::id).collect();
    let mut sorted_merge = merge_order.clone();
    sorted_merge.sort_unstable();
    if sorted_merge != canonical_component_ids {
        push(
            &mut diagnostics,
            DiagnosticCode::InvalidMergeOrder,
            SourceSpan::new(0, 0, 1, 1),
            "composition.merge_order",
            "exact component-ID permutation",
            format!(
                "{:?}",
                merge_order.iter().map(|id| id.get()).collect::<Vec<_>>()
            ),
            "list every component exactly once in semantic merge order",
        );
    }

    let mut formula_nodes = 0usize;
    for law in &laws {
        let (nodes, depth) = formula_shape_rel(law.formula());
        formula_nodes = formula_nodes.saturating_add(nodes);
        if depth > limits.max_formula_depth() {
            formula_limit(
                &mut diagnostics,
                span_for(&spans, 9, law.id()),
                format!("law.{}", law.id().get()),
                depth,
                limits.max_formula_depth(),
            );
        }
    }
    for claim in &mut claims {
        let mut normalized_backends = claim.backends.to_vec();
        normalized_backends.sort_unstable();
        normalized_backends.dedup();
        claim.backends = normalized_backends.into_boxed_slice();
        if claim.backends.is_empty() {
            push(
                &mut diagnostics,
                DiagnosticCode::InvalidDeclaration,
                span_for(&spans, 10, claim.id()),
                format!("claim.{}.backends", claim.id().get()),
                "one or more closed backend IDs",
                "empty backend set",
                "select cvc5, z3, lean, or all",
            );
        }
        match (claim.mode(), claim.formula()) {
            (ClaimMode::Relational, ClaimFormula::Relational(value)) => {
                let (nodes, depth) = formula_shape_rel(value);
                formula_nodes = formula_nodes.saturating_add(nodes);
                if depth > limits.max_formula_depth() {
                    formula_limit(
                        &mut diagnostics,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}", claim.id().get()),
                        depth,
                        limits.max_formula_depth(),
                    );
                }
            }
            (ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value)) => {
                if horizon == 0 {
                    push(
                        &mut diagnostics,
                        DiagnosticCode::InvalidDeclaration,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}.horizon", claim.id().get()),
                        "nonzero finite horizon",
                        "0",
                        "use at least one logical trace step",
                    );
                } else if horizon > MAX_FINITE_HORIZON {
                    push(
                        &mut diagnostics,
                        DiagnosticCode::ResourceLimit,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}.horizon", claim.id().get()),
                        text(MAX_FINITE_HORIZON),
                        text(horizon),
                        "reduce the finite logical-trace horizon",
                    );
                }
                let (nodes, depth) = formula_shape_temporal(value);
                formula_nodes = formula_nodes.saturating_add(nodes);
                if depth > limits.max_formula_depth() {
                    formula_limit(
                        &mut diagnostics,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}", claim.id().get()),
                        depth,
                        limits.max_formula_depth(),
                    );
                }
            }
            (ClaimMode::UnboundedProof, ClaimFormula::Temporal(value)) => {
                if claim.backends() != [BackendId::Lean] {
                    push(
                        &mut diagnostics,
                        DiagnosticCode::IncompatibleBackend,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}.backends", claim.id().get()),
                        "lean only for unbounded proof",
                        "non-Lean backend selection",
                        "export unbounded claims only to Lean",
                    );
                }
                let (nodes, depth) = formula_shape_temporal(value);
                formula_nodes = formula_nodes.saturating_add(nodes);
                if depth > limits.max_formula_depth() {
                    formula_limit(
                        &mut diagnostics,
                        span_for(&spans, 10, claim.id()),
                        format!("claim.{}", claim.id().get()),
                        depth,
                        limits.max_formula_depth(),
                    );
                }
            }
            _ => push(
                &mut diagnostics,
                DiagnosticCode::InvalidDeclaration,
                span_for(&spans, 10, claim.id()),
                format!("claim.{}.formula", claim.id().get()),
                "formula matching claim mode",
                "mismatched formula",
                "use a relational formula for relational mode and temporal formula otherwise",
            ),
        }
    }
    if formula_nodes > limits.max_formula_nodes() {
        push(
            &mut diagnostics,
            DiagnosticCode::ResourceLimit,
            SourceSpan::new(0, 0, 1, 1),
            "project.formulas",
            text(limits.max_formula_nodes()),
            text(formula_nodes),
            "reduce total formula size",
        );
    }

    if !diagnostics.is_empty() {
        return Err(DiagnosticSet::from_vec(diagnostics, diagnostic_limit));
    }
    Ok(ProjectSpec {
        project_id,
        name,
        namespaces: namespaces.into_boxed_slice(),
        types: types.into_boxed_slice(),
        fields: fields.into_boxed_slice(),
        variants: variants.into_boxed_slice(),
        reasons: reasons.into_boxed_slice(),
        effects: effects.into_boxed_slice(),
        channels: channels.into_boxed_slice(),
        components: components.into_boxed_slice(),
        composition: CompositionAst::new(wirings, merge_order),
        laws: laws.into_boxed_slice(),
        claims: claims.into_boxed_slice(),
    })
}

fn sort_and_duplicates<T, F, G>(
    values: &mut [T],
    id: F,
    name: G,
    label: &str,
    tag: u8,
    spans: &BTreeMap<(u8, u32), SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) where
    F: Fn(&T) -> StableId,
    G: Fn(&T) -> &Identifier,
{
    values.sort_by_key(|value| id(value));
    for pair in values.windows(2) {
        if id(&pair[0]) == id(&pair[1]) {
            push(
                diagnostics,
                DiagnosticCode::DuplicateId,
                span_for(spans, tag, id(&pair[1])),
                format!("{label}.id"),
                "unique stable ID",
                text(id(&pair[1]).get()),
                "allocate a distinct explicit ID",
            );
        }
    }
    let mut names = BTreeSet::new();
    for value in values.iter() {
        if !names.insert(name(value).as_str()) {
            push(
                diagnostics,
                DiagnosticCode::DuplicateName,
                span_for(spans, tag, id(value)),
                format!("{label}.name"),
                "unique stable name",
                name(value).as_str(),
                "choose a distinct stable name",
            );
        }
    }
}
fn span_for(spans: &BTreeMap<(u8, u32), SourceSpan>, tag: u8, id: StableId) -> SourceSpan {
    spans
        .get(&(tag, id.get()))
        .copied()
        .unwrap_or(SourceSpan::new(0, 0, 1, 1))
}
fn require_type(
    types: &BTreeMap<StableId, TypeKind>,
    id: StableId,
    span: SourceSpan,
    path: String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !types.contains_key(&id) {
        push(
            diagnostics,
            DiagnosticCode::UnknownReference,
            span,
            path,
            "declared type ID",
            text(id.get()),
            "declare the referenced type",
        );
    }
}
fn require_kind(
    types: &BTreeMap<StableId, TypeKind>,
    id: StableId,
    kind: TypeKind,
    span: SourceSpan,
    path: String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match types.get(&id) {
        Some(actual) if *actual == kind => {}
        Some(actual) => push(
            diagnostics,
            DiagnosticCode::InvalidDeclaration,
            span,
            path,
            format!("{kind:?} type"),
            format!("{actual:?}"),
            "reference a type with the required semantic role",
        ),
        None => require_type(types, id, span, path, diagnostics),
    }
}
fn duplicate_ids(
    values: &[StableId],
    span: SourceSpan,
    path: String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            push(
                diagnostics,
                DiagnosticCode::DuplicateId,
                span,
                path.clone(),
                "unique stable IDs",
                text(pair[0].get()),
                "remove the duplicate reference",
            );
        }
    }
}
fn formula_limit(
    diagnostics: &mut Vec<Diagnostic>,
    span: SourceSpan,
    path: String,
    actual: usize,
    limit: usize,
) {
    push(
        diagnostics,
        DiagnosticCode::ResourceLimit,
        span,
        path,
        text(limit),
        text(actual),
        "reduce formula nesting depth",
    );
}
fn push(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    span: SourceSpan,
    path: impl Into<Box<str>>,
    expected: impl Into<Box<str>>,
    actual: impl Into<Box<str>>,
    remediation: impl Into<Box<str>>,
) {
    diagnostics.push(Diagnostic::new(
        code,
        DiagnosticStage::Elaborate,
        AstPath::new(path),
        span,
        expected,
        actual,
        remediation,
    ));
}
