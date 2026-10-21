//! Fixed-size tables: exact expansion, stable IDs and diagnostics at the source.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use zeno_fcis_spec::{
    CompareOp, DiagnosticCode, ProjectLimits, ProjectSpec, RelExpr, SourceLimits, ValueExpr,
    elaborate_project, expand_tables, parse_project,
};

const LIBRARY: &str = include_str!("../../../examples/contract-tables/library-loans/project.zeno");

const HEADER: &str = "zeno 1;\nproject 1 tables;\nnamespace 2 tables;\ntype 100 state S;\n\
type 101 command C;\ntype 102 context X;\ntype 105 int V in 0..=3;\ntype 106 int W in 0..=1;\n\
field 120 101 pick 105;\nfield 121 101 other 105;\nvariant 130 102 Desk none;\n\
component 400 c { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; }\n\
merge [400];\n";

fn spec(source: &str) -> ProjectSpec {
    elaborate_project(
        parse_project(source, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap()
}

fn bytes(source: &str) -> Vec<u8> {
    spec(source).canonical_bytes().unwrap()
}

fn law(source: &str, id: u32) -> RelExpr {
    spec(source)
        .laws()
        .iter()
        .find(|law| law.id().get() == id)
        .unwrap()
        .formula()
        .clone()
}

fn errors(source: &str) -> Vec<(DiagnosticCode, u32, String)> {
    parse_project(source, SourceLimits::default())
        .expect_err("refused")
        .diagnostics()
        .iter()
        .map(|d| (d.code(), d.span().start(), d.path().as_str().to_owned()))
        .collect()
}

#[test]
fn tables_expand_every_entry() {
    let tabled = format!("{HEADER}table 110 100 slot 105 size 6;\n");
    let fields: String = (0..6)
        .map(|k| format!("field {} 100 slot_{k} 105;\n", 110 + k))
        .collect();
    assert_eq!(bytes(&tabled), bytes(&format!("{HEADER}{fields}")));
    let project = spec(&tabled);
    let entries: Vec<_> = project
        .fields()
        .iter()
        .filter(|field| field.owner().get() == 100)
        .map(|field| (field.id().get(), field.name().as_str().to_owned()))
        .collect();
    assert_eq!(
        entries,
        (0..6)
            .map(|k| (110 + k, format!("slot_{k}")))
            .collect::<Vec<_>>()
    );
    // Sizes 1 and 64 are the boundaries.
    for size in [1, 64] {
        let project = spec(&format!("{HEADER}table 1000 100 big 106 size {size};\n"));
        assert_eq!(project.fields().len(), 2 + size as usize);
    }
}

#[test]
fn tables_forall_covers_every_entry_exactly() {
    let tabled = format!(
        "{HEADER}table 110 100 slot 105 size 4;\n\
law 500 frame on commit = forall i in table 110 where i != command.101.120 {{ post.100.110[i] == pre.100.110[i] }};\n"
    );
    let entries: Vec<String> = (0..4)
        .map(|k| {
            format!(
                "({k} != command.101.120 -> post.100.{0} == pre.100.{0})",
                110 + k
            )
        })
        .collect();
    let written = format!(
        "{HEADER}table 110 100 slot 105 size 4;\nlaw 500 frame on commit = {};\n",
        entries.join(" && ")
    );
    assert_eq!(law(&tabled, 500), law(&written, 500));
    assert_eq!(bytes(&tabled), bytes(&written));
    // Without a condition, and with exists.
    let every = format!(
        "{HEADER}table 110 100 slot 105 size 3;\nlaw 500 shelf on commit, genesis = forall i in table 110 {{ post.100.110[i] == 0 }};\n\
law 501 any on commit = exists i in table 110 where i == command.101.120 {{ post.100.110[i] != 0 }};\n"
    );
    let written = format!(
        "{HEADER}table 110 100 slot 105 size 3;\nlaw 500 shelf on commit, genesis = post.100.110 == 0 && post.100.111 == 0 && post.100.112 == 0;\n\
law 501 any on commit = (0 == command.101.120 && post.100.110 != 0) || (1 == command.101.120 && post.100.111 != 0) || (2 == command.101.120 && post.100.112 != 0);\n"
    );
    assert_eq!(bytes(&every), bytes(&written));
}

/// The value of an expanded formula, with each state field `post.100.F` read
/// from `post` and every other projection from `inputs`.
fn holds(formula: &RelExpr, values: &BTreeMap<String, i128>) -> bool {
    fn value(v: &ValueExpr, values: &BTreeMap<String, i128>) -> i128 {
        match v {
            ValueExpr::Int(n) => *n,
            ValueExpr::Projection(path) => {
                let segments: Vec<String> = path
                    .segments()
                    .iter()
                    .map(|s| s.get().to_string())
                    .collect();
                values[&format!("{:?}.{}", path.root(), segments.join("."))]
            }
            ValueExpr::Add(a, b) => value(a, values) + value(b, values),
            ValueExpr::Sub(a, b) => value(a, values) - value(b, values),
            other => panic!("unexpected {other:?}"),
        }
    }
    match formula {
        RelExpr::Bool(b) => *b,
        RelExpr::Not(a) => !holds(a, values),
        RelExpr::And(a, b) => holds(a, values) && holds(b, values),
        RelExpr::Or(a, b) => holds(a, values) || holds(b, values),
        RelExpr::Implies(a, b) => !holds(a, values) || holds(b, values),
        RelExpr::Compare(op, a, b) => {
            let (a, b) = (value(a, values), value(b, values));
            match op {
                CompareOp::Eq => a == b,
                CompareOp::NotEq => a != b,
                CompareOp::Less => a < b,
                CompareOp::LessEq => a <= b,
                CompareOp::Greater => a > b,
                CompareOp::GreaterEq => a >= b,
            }
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn tables_count_matches_direct_counting_on_every_assignment() {
    // Four entries over 0..=3 and a command value: every assignment, every
    // operator and every bound from -1 to 5 against a direct count.
    let operators = [
        ("<=", CompareOp::LessEq),
        ("<", CompareOp::Less),
        (">=", CompareOp::GreaterEq),
        (">", CompareOp::Greater),
        ("==", CompareOp::Eq),
        ("!=", CompareOp::NotEq),
    ];
    for (written, op) in operators {
        for bound in -1..=5i128 {
            let source = format!(
                "{HEADER}table 110 100 slot 105 size 4;\n\
law 500 limit on commit = count j in table 110 {{ post.100.110[j] == command.101.120 }} {written} {bound};\n"
            );
            let formula = law(&source, 500);
            for assignment in 0..4i128.pow(5) {
                let mut values = BTreeMap::new();
                let digits: Vec<i128> = (0..5).map(|d| assignment / 4i128.pow(d) % 4).collect();
                for k in 0..4 {
                    values.insert(format!("Post.100.{}", 110 + k), digits[k as usize]);
                }
                values.insert("Command.101.120".to_owned(), digits[4]);
                let count = (0..4).filter(|k| digits[*k] == digits[4]).count() as i128;
                let expected = match op {
                    CompareOp::LessEq => count <= bound,
                    CompareOp::Less => count < bound,
                    CompareOp::GreaterEq => count >= bound,
                    CompareOp::Greater => count > bound,
                    CompareOp::Eq => count == bound,
                    CompareOp::NotEq => count != bound,
                };
                assert_eq!(
                    holds(&formula, &values),
                    expected,
                    "{written} {bound} {digits:?}"
                );
            }
        }
    }
}

#[test]
fn tables_count_expands_to_the_documented_clauses() {
    let tabled = format!(
        "{HEADER}table 110 100 slot 105 size 4;\n\
law 500 limit on commit = count j in table 110 {{ post.100.110[j] == 1 }} <= 2;\n"
    );
    // Every three-entry set, in order, combined left half first.
    let c = |k: u32| format!("post.100.{} == 1", 110 + k);
    let clause = |a, b, d| format!("!({} && {} && {})", c(a), c(b), c(d));
    let written = format!(
        "{HEADER}table 110 100 slot 105 size 4;\nlaw 500 limit on commit = {} && {} && ({} && {});\n",
        clause(0, 1, 2),
        clause(0, 1, 3),
        clause(0, 2, 3),
        clause(1, 2, 3)
    );
    assert_eq!(bytes(&tabled), bytes(&written));
}

#[test]
fn tables_entry_ids_do_not_depend_on_declaration_order() {
    let a = "table 110 100 slot 105 size 3;\n";
    let b = "table 140 100 flag 106 size 2;\n";
    let first = bytes(&format!("{HEADER}{a}{b}"));
    let second = bytes(&format!("{HEADER}{b}{a}"));
    assert_eq!(first, second);
    let ids: Vec<u32> = spec(&format!("{HEADER}{b}{a}"))
        .fields()
        .iter()
        .filter(|field| field.owner().get() == 100)
        .map(|field| field.id().get())
        .collect();
    assert_eq!(ids, [110, 111, 112, 140, 141]);
}

#[test]
fn tables_expansion_text_reparses_to_the_same_project() {
    let expansion = expand_tables(LIBRARY, SourceLimits::default()).unwrap();
    assert_eq!(bytes(&expansion.text), bytes(LIBRARY));
    for form in ["table 110", "index", "[i]", "[j]", "forall", "count"] {
        assert!(!expansion.text.contains(form), "{form}");
    }
    assert!(expansion.text.contains("field 115 100 copies_5 105;"));
    assert!(expansion.text.contains("type 106 int CopyNumber in 0..=5;"));
    // Comments and untouched declarations are kept.
    assert!(
        expansion
            .text
            .contains("// Only the chosen copy changes hands")
    );
    assert!(
        expansion
            .text
            .contains("law 501 librarian_only on commit = context.102 == 130;")
    );
    assert_eq!(expansion.tables.len(), 1);
    assert_eq!(expansion.tables[0].size, 6);
    // Comparisons holding entries or sums are rewritten whole.
    let source = format!(
        "{HEADER}table 110 100 slot 105 size 3;\n\
law 500 total on commit, genesis = 2 * sum i in table 110 {{ post.100.110[i] }} <= 9 && post.100.110[2] >= -1;\n\
law 501 tail on commit, genesis = sum i in table 110 where i > 0 {{ post.100.110[i] }} - post.100.110[0] <= 3;\n"
    );
    let expansion = expand_tables(&source, SourceLimits::default()).unwrap();
    assert!(
        expansion.text.contains(
            "2 * (post.100.110 + post.100.111 + post.100.112) <= 9 && post.100.112 >= -1;"
        )
    );
    assert!(
        expansion
            .text
            .contains("post.100.111 + post.100.112 - post.100.110 <= 3;")
    );
    assert_eq!(bytes(&expansion.text), bytes(&source));
}

#[test]
fn tables_index_types_take_the_table_range() {
    let project = spec(LIBRARY);
    let index = project
        .types()
        .iter()
        .find(|t| t.id().get() == 106)
        .unwrap();
    let range = index.range().unwrap();
    assert_eq!((range.min(), range.max()), (0, 5));
    assert_eq!(
        project
            .fields()
            .iter()
            .filter(|f| f.owner().get() == 100)
            .count(),
        6
    );
}

#[test]
fn tables_report_errors_at_the_table_source() {
    let at = |source: &str, needle: &str| source.find(needle).unwrap() as u32;
    // Too many entries is the resource limit, at the table declaration.
    let source = format!("{HEADER}table 110 100 slot 105 size 65;\n");
    assert_eq!(
        errors(&source),
        [(
            DiagnosticCode::ResourceLimit,
            at(&source, "table 110"),
            "table.size".to_owned()
        )]
    );
    // An unknown table, an entry out of range and a stray index.
    let source = format!(
        "{HEADER}table 110 100 slot 105 size 3;\n\
law 500 a on commit = forall i in table 999 {{ post.100.110[i] == 0 }};\n\
law 501 b on commit = post.100.110[3] == 0;\n\
law 502 c on commit = post.100.120[0] == 0;\n"
    );
    let found = errors(&source);
    assert!(found.contains(&(
        DiagnosticCode::UnknownReference,
        at(&source, "forall"),
        "table.reference".to_owned()
    )));
    assert!(found.contains(&(
        DiagnosticCode::InvalidDeclaration,
        at(&source, "[3]"),
        "table.entry".to_owned()
    )));
    assert!(found.contains(&(
        DiagnosticCode::InvalidDeclaration,
        at(&source, "[0] == 0;\n"),
        "table.entry".to_owned()
    )));
    // One error inside a body is reported once, not once per entry.
    let source = format!(
        "{HEADER}table 110 100 slot 105 size 5;\nlaw 500 a on commit = forall i in table 110 {{ post.100.110[i] == }};\n"
    );
    assert_eq!(errors(&source).len(), 1);
    // A duplicate entry ID is reported at the table that declares it.
    let source = format!("{HEADER}field 111 100 spare 105;\ntable 110 100 slot 105 size 3;\n");
    let refused = elaborate_project(
        parse_project(&source, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap_err();
    assert!(
        refused
            .diagnostics()
            .iter()
            .any(|d| d.code() == DiagnosticCode::DuplicateId
                && d.span().start() == at(&source, "table 110"))
    );
    // A count too large to expand stops at the resource limit.
    let source = format!(
        "{HEADER}table 110 100 slot 106 size 64;\nlaw 500 a on commit = count j in table 110 {{ post.100.110[j] == 1 }} <= 31;\n"
    );
    assert_eq!(
        errors(&source),
        [(
            DiagnosticCode::ResourceLimit,
            at(&source, "count"),
            "table.expansion".to_owned()
        )]
    );
}

#[test]
fn tables_leave_existing_names_and_quantifiers_alone() {
    // `count`, `table` and `index` stay ordinary names outside table forms,
    // and quantifier variables shadow nothing.
    let source = format!(
        "{HEADER}field 110 100 v 105;\n\
claim 600 c lean relational = forall count in 0..3 {{ count + table <= index }};\n"
    );
    let claim = spec(&source).claims()[0].clone();
    let text = format!("{:?}", claim.formula());
    assert!(text.contains("Var(Identifier(\"count\"))"));
    assert!(text.contains("Var(Identifier(\"table\"))"));
}
