//! Fixed-size tables: exact expansion, stable IDs and diagnostics at the source.

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
        parse_project(source, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}")),
        ProjectLimits::default(),
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
}

fn bytes(source: &str) -> Vec<u8> {
    spec(source)
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("{error:?}"))
}

fn law(source: &str, id: u32) -> RelExpr {
    spec(source)
        .laws()
        .iter()
        .find(|law| law.id().get() == id)
        .unwrap_or_else(|| panic!("expected table fixture value"))
        .formula()
        .clone()
}

fn errors(source: &str) -> Vec<(DiagnosticCode, u32, String)> {
    parse_project(source, SourceLimits::default())
        .err()
        .unwrap_or_else(|| panic!("expected refusal"))
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
            ValueExpr::Select(c, a, b) => {
                let condition = holds(c, values);
                let then = value(a, values);
                let otherwise = value(b, values);
                if condition { then } else { otherwise }
            }
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
    let expansion =
        expand_tables(LIBRARY, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}"));
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
    let expansion =
        expand_tables(&source, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}"));
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
        .unwrap_or_else(|| panic!("expected copy index"));
    let range = index
        .range()
        .unwrap_or_else(|| panic!("expected table fixture value"));
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
    let at = |source: &str, needle: &str| {
        source
            .find(needle)
            .unwrap_or_else(|| panic!("expected table fixture value")) as u32
    };
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
    // A count too large to expand stops at the resource limit and says how
    // many clauses it would need: C(64, 32).
    let source = format!(
        "{HEADER}table 1000 100 slot 106 size 64;\nlaw 500 a on commit = count j in table 1000 {{ post.100.1000[j] == 1 }} <= 31;\n"
    );
    assert_eq!(
        errors(&source),
        [(
            DiagnosticCode::ResourceLimit,
            at(&source, "count"),
            "table.count".to_owned()
        )]
    );
    let refused = parse_project(&source, SourceLimits::default())
        .err()
        .unwrap_or_else(|| panic!("expected refusal"));
    assert_eq!(
        refused.diagnostics()[0].actual(),
        "1832624140942590534 clauses of 32 entries each, per expansion"
    );
}

#[test]
fn tables_refuse_entry_ids_that_meet_any_other_declaration() {
    let at = |source: &str, needle: &str| {
        source
            .find(needle)
            .unwrap_or_else(|| panic!("expected table fixture value")) as u32
    };
    // Entry IDs 110..=115 against a field, a type, a variant, a reason, a
    // law and another table: each is refused at the table.
    for (other, kind) in [
        ("field 113 100 spare 105;", "field 113"),
        ("type 112 int Spare in 0..=1;", "type 112"),
        ("variant 115 102 Spare none;", "variant 115"),
        ("reason 111 spare precedence 0;", "reason 111"),
        ("law 114 spare on commit = true;", "law 114"),
        (
            "reward 113 spare random {600} on post.100.110 { expectation >= 0/1; }",
            "reward 113",
        ),
        (
            "random 112 entropy on context.102.130 bits 1 independent uniform;",
            "random 112",
        ),
        (
            "distribution 114 outcomes on post.100.110 = uniform over {0, 1};",
            "distribution 114",
        ),
        ("table 115 100 other 105 size 2;", "field 115"),
    ] {
        let source = format!("{HEADER}table 110 100 slot 105 size 6;\n{other}\n");
        let refused = parse_project(&source, SourceLimits::default())
            .err()
            .unwrap_or_else(|| panic!("{other} must refuse"));
        let found: Vec<_> = refused
            .diagnostics()
            .iter()
            .filter(|d| d.code() == DiagnosticCode::DuplicateId)
            .map(|d| {
                (
                    d.span().start(),
                    d.path().as_str().to_owned(),
                    d.actual().to_owned(),
                )
            })
            .collect();
        assert!(
            found.contains(&(
                at(&source, "table 110"),
                "table.id".to_owned(),
                kind.to_owned()
            )),
            "{other}: {found:?}"
        );
    }
    // The IDs just outside the range, and the table's own entries, are free.
    let source = format!(
        "{HEADER}type 109 int Below in 0..=1;\ntable 110 100 slot 105 size 6;\nlaw 116 above on commit = true;\n"
    );
    assert!(parse_project(&source, SourceLimits::default()).is_ok());
}

#[test]
fn tables_report_each_count_with_its_clauses() {
    let expansion =
        expand_tables(LIBRARY, SourceLimits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    let line = LIBRARY[..LIBRARY
        .find("count j")
        .unwrap_or_else(|| panic!("expected table fixture value"))]
        .lines()
        .count() as u32;
    assert_eq!(expansion.counts.len(), 1);
    let count = &expansion.counts[0];
    // C(6, 3) clauses, once per copy of the enclosing forall.
    assert_eq!((count.line, count.clauses, count.expansions), (line, 20, 6));
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

#[test]
fn tables_include_the_maximum_stable_id_in_collision_checks() {
    for size in 1..=64 {
        let first = u32::MAX - (size - 1);
        let table = format!("table {first} 100 slot 105 size {size};\n");
        assert!(parse_project(&format!("{HEADER}{table}"), SourceLimits::default()).is_ok());
        for before in [false, true] {
            let other = format!("type {} int Spare in 0..=1;\n", u32::MAX);
            let source = if before {
                format!("{HEADER}{other}{table}")
            } else {
                format!("{HEADER}{table}{other}")
            };
            let refused = parse_project(&source, SourceLimits::default())
                .err()
                .unwrap_or_else(|| panic!("upper endpoint must collide"));
            assert!(
                refused
                    .diagnostics()
                    .iter()
                    .any(|d| d.code() == DiagnosticCode::DuplicateId
                        && d.path().as_str() == "table.id"
                        && d.actual() == format!("type {}", u32::MAX)),
                "size={size} before={before}: {refused:?}"
            );
        }
    }
}

#[test]
fn table_value_counts_are_linear_and_match_conditional_sums() {
    for n in [6, 16, 64] {
        let prefix = format!("{HEADER}table 1000 100 slot 105 size {n};\n");
        let counted = format!(
            "{prefix}law 500 limit on commit = count j in table 1000 {{ post.100.1000[j] == command.101.120 }} <= command.101.121;\n"
        );
        let summed = format!(
            "{prefix}law 500 limit on commit = sum j in table 1000 {{ choose(post.100.1000[j] == command.101.120, 1, 0) }} <= command.101.121;\n"
        );
        let filtered = format!(
            "{prefix}law 500 limit on commit = sum j in table 1000 where post.100.1000[j] == command.101.120 {{ 1 }} <= command.101.121;\n"
        );
        assert_eq!(bytes(&counted), bytes(&summed));
        assert_eq!(bytes(&counted), bytes(&filtered));
        let expansion = expand_tables(&counted, SourceLimits::default()).unwrap();
        assert_eq!(expansion.text.matches("choose(").count(), n);
        assert_eq!(bytes(&counted), bytes(&expansion.text));
        assert!(expansion.counts[0].linear);
        assert_eq!(expansion.counts[0].clauses, n as u128);
        eprintln!(
            "N={n}: linear terms={n}, expanded bytes={}",
            expansion.text.len()
        );
    }
}

#[test]
fn table_value_count_limit_includes_the_boundary_and_refuses_three_under_two() {
    let source = format!(
        "{HEADER}table 110 100 slot 105 size 6;\nlaw 500 limit on commit = count j in table 110 {{ post.100.110[j] == command.101.120 }} <= command.101.121;\n"
    );
    let formula = law(&source, 500);
    for held in 0..=6 {
        let mut values = BTreeMap::from([
            ("Command.101.120".to_owned(), 1),
            ("Command.101.121".to_owned(), 2),
        ]);
        for k in 0..6 {
            values.insert(format!("Post.100.{}", 110 + k), i128::from(k < held));
        }
        assert_eq!(holds(&formula, &values), held <= 2, "held={held} limit=2");
    }
    // A count can occur on either side and within arithmetic.
    for formula in [
        "command.101.121 >= count j in table 110 { post.100.110[j] == command.101.120 }",
        "count j in table 110 { post.100.110[j] == command.101.120 } + 1 <= command.101.121 + 1",
    ] {
        let source = format!(
            "{HEADER}table 110 100 slot 105 size 6;\nlaw 500 limit on commit = {formula};\n"
        );
        assert_eq!(
            bytes(&source),
            bytes(
                &expand_tables(&source, SourceLimits::default())
                    .unwrap()
                    .text
            )
        );
    }
}
