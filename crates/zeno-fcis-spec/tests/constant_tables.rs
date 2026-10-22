//! Constant-table specialization and finite closure checks.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_spec::{
    CompareOp, ProjectLimits, RelExpr, SourceLimits, ValueExpr, elaborate_project, expand_tables,
    parse_project,
};
fn project(s: &str) -> zeno_fcis_spec::ProjectSpec {
    elaborate_project(
        parse_project(s, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap()
}
fn closure(parents: [usize; 3]) -> String {
    let mut s = format!(
        "zeno 1; project 1 hierarchy; type 100 state S; type 105 int Parent in 0..=3;
      const table 110 parents 105 size 3 [{},{},{}]; type 106 index Role of 110;
      derived table 700 ancestor[i][j] in table 110 rounds 9 {{
        when true then i == j || exists k in table 110 {{ parents[i] == k + 1 && ancestor[k][j] }};
        default false;
      }};",
        parents[0], parents[1], parents[2]
    );
    for i in 0..3 {
        for j in 0..3 {
            s += &format!("law {} pair_{i}_{j} = ancestor[{i}][{j}];", 500 + i * 3 + j);
        }
    }
    s
}
#[test]
fn constant_closure_becomes_literal_lookup_without_state_fields() {
    for p0 in 0..4 {
        for p1 in 0..4 {
            for p2 in 0..4 {
                let parents = [p0, p1, p2];
                let source = closure(parents);
                let p = project(&source);
                assert!(p.fields().is_empty(), "constant tables add no fields");
                let mut reach = [[false; 3]; 3];
                loop {
                    let next = std::array::from_fn(|i| {
                        std::array::from_fn(|j| {
                            i == j || (parents[i] > 0 && reach[parents[i] - 1][j])
                        })
                    });
                    if next == reach {
                        break;
                    }
                    reach = next;
                }
                for (i, row) in reach.iter().enumerate() {
                    for (j, expected) in row.iter().enumerate() {
                        assert_eq!(
                            p.laws()[i * 3 + j].formula(),
                            &RelExpr::Bool(*expected),
                            "parents={parents:?}, pair={i},{j}"
                        );
                    }
                }
                let expanded = expand_tables(&source, SourceLimits::default()).unwrap();
                assert!(!expanded.text.contains("const table"));
                assert_eq!(
                    p.canonical_bytes().unwrap(),
                    project(&expanded.text).canonical_bytes().unwrap()
                );
            }
        }
    }
}
#[test]
fn constant_words_and_prior_strata_fold_before_the_checked_project() {
    let s="zeno 1; project 1 words; type 100 state S; type 105 int Parent in 0..=3; type 106 bits Word width 3;
      const table 110 parents 105 size 3 [0,1,2]; const table 210 grants 106 size 3 [u128(1),u128(2),u128(4)];
      derived table 700 x[i] in table 110 bits 3 rounds 9 {when true then join(grants[i],join j in table 110 where parents[i] == j+1 {x[j]}); default u128(0);};
      law 500 all_permissions = x[2] == u128(7);";
    let p = project(s);
    assert!(p.fields().is_empty());
    assert_eq!(
        p.laws()[0].formula(),
        &RelExpr::Compare(CompareOp::Eq, ValueExpr::U128(7), ValueExpr::U128(7))
    );
    assert_eq!(
        p.canonical_bytes().unwrap(),
        project(&expand_tables(s, SourceLimits::default()).unwrap().text)
            .canonical_bytes()
            .unwrap()
    );
}
#[test]
fn constant_types_lengths_indices_names_and_ids_are_checked() {
    let good = closure([0, 1, 2]);
    for bad in [
        good.replace("size 3 [0,1,2]", "size 3 [0,1]"),
        good.replace("size 3 [0,1,2]", "size 3 [0,1,2,3]"),
        good.replace("[0,1,2]", "[0,1,4]"),
        good.replace("parents 105", "parents 999"),
        good.replace("const table 110", "const table 100"),
        good.replace("const table 110", "const table 1"),
        good.replace("derived table 700", "derived table 110"),
        good.replace("ancestor[0][0]", "parents[3] == 0"),
        good.replace("derived table 700 ancestor", "derived table 700 parents"),
    ] {
        assert!(
            parse_project(&bad, SourceLimits::default()).is_err(),
            "{bad}"
        );
    }
}
#[test]
fn boolean_and_extreme_integer_literals_keep_their_types() {
    let s = format!(
        "zeno 1; project 1 literals; type 100 state S; type 105 bool Flag; type 106 int Signed;
      const table 110 flags 105 size 2 [true,false]; const table 210 numbers 106 size 1 [{}];
      law 500 flag = flags[0] && !flags[1]; law 501 signed = numbers[0] == {};",
        i128::MIN,
        i128::MIN
    );
    let p = project(&s);
    let expanded = expand_tables(&s, SourceLimits::default()).unwrap();
    assert_eq!(
        p.canonical_bytes().unwrap(),
        project(&expanded.text).canonical_bytes().unwrap()
    );
}

#[test]
fn full_width_constant_guards_fold_without_signed_casts() {
    let s=format!("zeno 1; project 1 unsigned_guard; type 100 state S; type 105 bits Word width128; const table 110 words 105 size 1 [u128({})]; derived table 700 x[i] in table 110 rounds 1 {{when words[i] == u128({}) then true; default false;}}; law 500 query = x[0];",u128::MAX,u128::MAX).replace("width128","width 128");
    assert_eq!(project(&s).laws()[0].formula(), &RelExpr::Bool(true));
}
