//! Independent schema models and rejected authoring-to-schema boundaries.
#![forbid(unsafe_code)]

use zeno_fcis_bootstrap::{SchemaLoweringError, lower_schema};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_schema::{
    FieldDef, FieldId, Schema, SchemaLimits, SumVariantDef, TypeDef, TypeId, TypeKind, VariantId,
};
use zeno_fcis_spec::{
    ProjectLimits, ProjectSpec, SourceLimits, StableId, elaborate_project, parse_project,
};

const SOURCE: &str = "zeno 1; project 1 Lowered;
type 100 state State; type 101 command Command;
type 102 context Context; type 103 int Amount;
field 110 100 count 103;
variant 111 101 Increment none; variant 112 101 Fail none;
component 300 counter { owns 100; reads pre.100; writes post.100; }
merge [300];";

fn id(value: u32) -> StableId {
    StableId::new(value).unwrap_or_else(|| unreachable!())
}

fn project(source: &str) -> ProjectSpec {
    let parsed = parse_project(source, SourceLimits::default())
        .unwrap_or_else(|error| panic!("parse: {error:?}"));
    elaborate_project(parsed, ProjectLimits::default())
        .unwrap_or_else(|error| panic!("elaborate: {error:?}"))
}

fn leaves() -> Vec<(StableId, TypeKind)> {
    vec![
        (id(102), TypeKind::Bool),
        (id(103), TypeKind::I128 { min: 0, max: 4 }),
    ]
}

#[test]
fn authored_records_and_sums_lower_to_the_independently_constructed_schema() {
    let limits = SchemaLimits::default();
    let actual = lower_schema(&project(SOURCE), id(100), 7, leaves(), limits)
        .unwrap_or_else(|error| panic!("lower: {error}"));
    let definition = |id, name, kind| {
        TypeDef::try_new(TypeId::new(id), name, kind, limits)
            .unwrap_or_else(|error| panic!("type: {error}"))
    };
    let expected = Schema::try_new(
        "Lowered",
        7,
        TypeId::new(100),
        vec![
            definition(
                100,
                "State",
                TypeKind::Record {
                    fields: vec![
                        FieldDef::try_new(FieldId::new(110), "count", TypeId::new(103))
                            .unwrap_or_else(|error| panic!("field: {error}")),
                    ]
                    .into_boxed_slice(),
                },
            ),
            definition(
                101,
                "Command",
                TypeKind::Sum {
                    variants: vec![
                        SumVariantDef::try_new(VariantId::new(111), "Increment", None)
                            .unwrap_or_else(|error| panic!("variant: {error}")),
                        SumVariantDef::try_new(VariantId::new(112), "Fail", None)
                            .unwrap_or_else(|error| panic!("variant: {error}")),
                    ]
                    .into_boxed_slice(),
                },
            ),
            definition(102, "Context", TypeKind::Bool),
            definition(103, "Amount", TypeKind::I128 { min: 0, max: 4 }),
        ],
        limits,
    )
    .unwrap_or_else(|error| panic!("schema: {error}"));
    assert_eq!(actual.canonical_bytes(), expected.canonical_bytes());
}

#[test]
fn lowering_requires_exact_leaf_binding_coverage() {
    let spec = project(SOURCE);
    let lower = |bindings| lower_schema(&spec, id(100), 1, bindings, SchemaLimits::default());
    assert_eq!(
        lower(vec![(id(102), TypeKind::Bool)]),
        Err(SchemaLoweringError::MissingLeafBinding(id(103)))
    );
    let mut extra = leaves();
    extra.push((id(999), TypeKind::Unit));
    assert_eq!(
        lower(extra),
        Err(SchemaLoweringError::UnusedLeafBinding(id(999)))
    );
    let mut duplicate = leaves();
    duplicate.push((id(102), TypeKind::Bool));
    assert_eq!(
        lower(duplicate),
        Err(SchemaLoweringError::DuplicateLeafBinding(id(102)))
    );
    let mut overridden = leaves();
    overridden.push((id(100), TypeKind::Unit));
    assert_eq!(
        lower(overridden),
        Err(SchemaLoweringError::AuthoredShapeOverride(id(100)))
    );
}

#[test]
fn lowering_preserves_primitive_roles_and_requires_a_state_root() {
    let spec = project(SOURCE);
    assert_eq!(
        lower_schema(&spec, id(101), 1, leaves(), SchemaLimits::default()),
        Err(SchemaLoweringError::WrongRoot(id(101)))
    );
    let bindings = vec![
        (id(102), TypeKind::Bool),
        (
            id(103),
            TypeKind::Text {
                min_len: 0,
                max_len: 4,
            },
        ),
    ];
    assert_eq!(
        lower_schema(&spec, id(100), 1, bindings, SchemaLimits::default()),
        Err(SchemaLoweringError::IncompatiblePrimitive(id(103)))
    );
}

#[test]
fn lowering_rejects_field_width_overflow_without_renumbering() {
    let source = SOURCE.replace("field 110", "field 65536");
    assert_eq!(
        lower_schema(
            &project(&source),
            id(100),
            1,
            leaves(),
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::IdOutOfRange(id(65536)))
    );
}

#[test]
fn source_and_leaf_order_do_not_change_schema_bytes() {
    let reordered = SOURCE.replace(
        "type 102 context Context; type 103 int Amount;",
        "type 103 int Amount; type 102 context Context;",
    );
    let mut reversed = leaves();
    reversed.reverse();
    assert_eq!(
        lower_schema(
            &project(SOURCE),
            id(100),
            1,
            leaves(),
            SchemaLimits::default()
        ),
        lower_schema(
            &project(&reordered),
            id(100),
            1,
            reversed,
            SchemaLimits::default()
        )
    );
}

#[test]
fn lowering_rejects_ambiguous_and_primitive_structures() {
    let ambiguous = SOURCE.replace(
        "merge [300];",
        "variant 130 100 Ambiguous none; merge [300];",
    );
    assert_eq!(
        lower_schema(
            &project(&ambiguous),
            id(100),
            1,
            leaves(),
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::AmbiguousShape(id(100)))
    );
    let primitive_record = SOURCE.replace(
        "merge [300];",
        "field 130 103 nested_value 102; merge [300];",
    );
    assert_eq!(
        lower_schema(
            &project(&primitive_record),
            id(100),
            1,
            vec![(id(102), TypeKind::Bool)],
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::IncompatiblePrimitive(id(103)))
    );
}

#[test]
fn lowering_rejects_hidden_compound_bindings_and_variant_overflow() {
    let bindings = vec![
        (
            id(102),
            TypeKind::Record {
                fields: Box::new([]),
            },
        ),
        (id(103), TypeKind::I128 { min: 0, max: 4 }),
    ];
    assert_eq!(
        lower_schema(
            &project(SOURCE),
            id(100),
            1,
            bindings,
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::UnsupportedLeafShape(id(102)))
    );
    let wide_variant = SOURCE.replace("variant 111", "variant 65536");
    assert_eq!(
        lower_schema(
            &project(&wide_variant),
            id(100),
            1,
            leaves(),
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::IdOutOfRange(id(65536)))
    );
}

#[test]
fn schema_checks_still_reject_cycles_invalid_bounds_names_and_resource_limits() {
    for source in [
        SOURCE.replace("count 103", "count 100"),
        SOURCE.replace("count 103", "counter-total 103"),
    ] {
        assert!(matches!(
            lower_schema(
                &project(&source),
                id(100),
                1,
                leaves(),
                SchemaLimits::default()
            ),
            Err(SchemaLoweringError::Schema(_))
        ));
    }
    let bounds = vec![
        (id(102), TypeKind::Bool),
        (id(103), TypeKind::I128 { min: 4, max: 0 }),
    ];
    assert!(matches!(
        lower_schema(
            &project(SOURCE),
            id(100),
            1,
            bounds,
            SchemaLimits::default()
        ),
        Err(SchemaLoweringError::Schema(_))
    ));
    for limits in [
        SchemaLimits {
            max_types: 3,
            ..SchemaLimits::default()
        },
        SchemaLimits {
            max_variants: 1,
            ..SchemaLimits::default()
        },
    ] {
        assert!(matches!(
            lower_schema(&project(SOURCE), id(100), 1, leaves(), limits),
            Err(SchemaLoweringError::Schema(_))
        ));
    }
}
