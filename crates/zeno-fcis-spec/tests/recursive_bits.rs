//! Finite algebra boundary checks and atom-lifting equivalence.
use zeno_fcis_spec::{
    CompareOp, ProjectLimits, RelExpr, SourceLimits, ValueExpr, elaborate_project, expand_tables,
    parse_formula, parse_project,
};

fn project(source: &str) -> zeno_fcis_spec::ProjectSpec {
    elaborate_project(
        parse_project(source, SourceLimits::default())
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        ProjectLimits::default(),
    )
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
}
fn value(v: &ValueExpr, fields: &[u128]) -> u128 {
    match v {
        ValueExpr::Int(n) => {
            u128::try_from(*n).unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
        }
        ValueExpr::U128(n) => *n,
        ValueExpr::Projection(p) => {
            let id = p
                .segments()
                .last()
                .unwrap_or_else(|| panic!("projection must have a field"))
                .get();
            fields[if id >= 210 {
                3 + (id - 210) as usize
            } else {
                (id - 110) as usize
            }]
        }
        ValueExpr::Select(c, a, b) => {
            let (a, b) = (value(a, fields), value(b, fields));
            if holds(c, fields) { a } else { b }
        }
        ValueExpr::Add(a, b) => value(a, fields)
            .checked_add(value(b, fields))
            .unwrap_or_else(|| panic!("reference arithmetic must be defined")),
        ValueExpr::Sub(a, b) => value(a, fields)
            .checked_sub(value(b, fields))
            .unwrap_or_else(|| panic!("reference arithmetic must be defined")),
        ValueExpr::Mul(a, b) => value(a, fields)
            .checked_mul(value(b, fields))
            .unwrap_or_else(|| panic!("reference arithmetic must be defined")),
        ValueExpr::Div(_, a, b) => value(a, fields) / value(b, fields),
        _ => panic!("unexpected value {v:?}"),
    }
}
fn holds(f: &RelExpr, fields: &[u128]) -> bool {
    match f {
        RelExpr::Bool(b) => *b,
        RelExpr::Circuit(c) => c
            .evaluate(|f| Some(holds(f, fields)))
            .unwrap_or_else(|| panic!("circuit reference must evaluate")),
        RelExpr::Not(a) => !holds(a, fields),
        RelExpr::And(a, b) => holds(a, fields) & holds(b, fields),
        RelExpr::Or(a, b) => holds(a, fields) | holds(b, fields),
        RelExpr::Implies(a, b) => !holds(a, fields) | holds(b, fields),
        RelExpr::Compare(op, a, b) => {
            let (a, b) = (value(a, fields), value(b, fields));
            match op {
                CompareOp::Eq => a == b,
                CompareOp::NotEq => a != b,
                CompareOp::Less => a < b,
                CompareOp::LessEq => a <= b,
                CompareOp::Greater => a > b,
                CompareOp::GreaterEq => a >= b,
            }
        }
        _ => panic!("unexpected formula {f:?}"),
    }
}
fn source(width: u8) -> String {
    let mut s=format!("zeno 1; project 1 bits_inheritance; type 100 state S; type 101 command C; type 102 context X;
        type 105 int Index in 0..=2; type 106 bits Permissions width {width};
        table 110 100 parents 105 size 3; table 210 100 grants 106 size 3;
        derived table 700 permissions[i] in table 110 bits {width} rounds {} {{
          when true then join(pre.100.210[i], join j in table 110 where pre.100.110[i] == j {{ permissions[j] }});
          default u128(0);
        }};",3*u32::from(width));
    for i in 0..3 {
        for k in 0..width {
            s += &format!(
                "law {} atom_{i}_{k} = subset({width}, u128({}), permissions[{i}]);",
                500 + i * 128 + u32::from(k),
                1u128 << k
            );
        }
    }
    s
}
#[test]
fn small_algebras_match_independent_boolean_lifts_exhaustively() {
    for width in [2, 3] {
        let p = project(&source(width));
        let base = 1usize << width;
        for parents in 0..27usize {
            let parent = [parents % 3, parents / 3 % 3, parents / 9];
            for grants in 0..base.pow(3) {
                let grant = [grants % base, grants / base % base, grants / (base * base)];
                let fields = [
                    parent[0] as u128,
                    parent[1] as u128,
                    parent[2] as u128,
                    grant[0] as u128,
                    grant[1] as u128,
                    grant[2] as u128,
                ];
                for k in 0..width {
                    let mut bits = [false; 3];
                    loop {
                        let next =
                            std::array::from_fn(|i| grant[i] & (1 << k) != 0 || bits[parent[i]]);
                        if next == bits {
                            break;
                        }
                        bits = next;
                    }
                    for (i, expected) in bits.into_iter().enumerate() {
                        assert_eq!(
                            holds(p.laws()[i * width as usize + k as usize].formula(), &fields),
                            expected,
                            "width={width}, parents={parent:?}, grants={grant:?}, entry={i}, atom={k}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn full_width_constants_complement_and_round_bound_are_exact() {
    let all = u128::MAX;
    let s=format!("zeno 1; project 1 full_width; type 100 state S; type 105 int V in 0..=0; table 110 100 slots 105 size 1;
       type 106 bits Permissions width 128;
       derived table 700 x[i] in table 110 bits 128 rounds 128 {{ when true then join(u128({all}),x[i]); default u128(0); }};
       law 500 ones = x[0] == u128({all}); law 501 zero = complement(128, x[0]) == u128(0);
       law 502 meet = meet(128, x[0],u128(1)) == u128(1);
       law 503 ordering = !subset(128,u128(2),u128(1));");
    let p = project(&s);
    assert!(p.laws().iter().all(|l| holds(l.formula(), &[])));
    let expanded = expand_tables(&s, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(
        p.canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        project(&expanded.text)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
    );
    assert!(
        parse_project(
            &s.replace("rounds 128", "rounds 127"),
            SourceLimits::default()
        )
        .is_err()
    );
    assert!(
        parse_project(
            &s.replace("width 128", "width 129"),
            SourceLimits::default()
        )
        .is_err()
    );
}
#[test]
fn self_complement_is_refused() {
    let s="zeno 1; project 1 self_complement; type 100 state S; type 105 int V in 0..=0; table 110 100 slots 105 size 1;
      derived table 700 x[i] in table 110 bits 2 rounds 2 { when true then complement(x[i]); default u128(0); };";
    let error = parse_project(s, SourceLimits::default())
        .expect_err("same-stratum complement must be refused");
    assert!(error.to_string().contains("complement"), "{error}");
}
#[test]
fn algebra_widths_guards_and_strata_are_checked() {
    let good = source(2);
    let p = project(&good);
    let expansion = expand_tables(&good, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(
        p.canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        project(&expansion.text)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
    );
    assert!(
        parse_formula(
            &good,
            "subset(2, u128(1), permissions[0])",
            SourceLimits::default()
        )
        .is_ok()
    );
    for bad in [
        good.replace("when true then", "when permissions[i] == u128(0) then"),
        good.replace("default u128(0)", "default u128(4)"),
        good.replace("permissions[j]", "complement(permissions[j])"),
        good.replace("subset(2, u128(1)", "subset(3, u128(1)"),
    ] {
        assert!(parse_project(&bad, SourceLimits::default()).is_err());
    }
}

#[test]
fn nested_algebra_expansion_preserves_binding_and_negation() {
    let mut s = source(2);
    s += "law 950 nested = choose(subset(2, u128(1), permissions[0]), complement(2, permissions[1]), meet(2, permissions[1],permissions[2])) == join(2,permissions[0],u128(0));";
    let p = project(&s);
    let expansion = expand_tables(&s, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert!(
        p.canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
            == project(&expansion.text)
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        "nested algebra must reparse identically"
    );
    for invalid in [
        "permissions[0]; law 900 injected = true;",
        "permissions[3]",
        "subset(2,u128(1),permissions[0]); type 900 data Injected;",
    ] {
        assert!(zeno_fcis_spec::parse_formula(&s, invalid, SourceLimits::default()).is_err());
    }
}

#[test]
fn full_width_variable_bits_do_not_overflow_at_the_top_atom() {
    let s="zeno 1; project 1 top_atom; type 100 state S; type 105 bits Word width 128; table 110 100 words 105 size 1;
      law 500 top = subset(128,u128(170141183460469231731687303715884105728),pre.100.110[0]);
      law 501 complement = join(128,pre.100.110[0],complement(pre.100.110[0])) == u128(340282366920938463463374607431768211455);";
    // Within an explicit-width helper, inner algebra operations inherit that width.
    let p = project(s);
    let expansion = expand_tables(s, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert!(
        p.canonical_bytes()
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
            == project(&expansion.text)
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
    );
    for n in [0, 1, 1u128 << 63, 1u128 << 127, u128::MAX] {
        let fields = [n];
        assert_eq!(
            holds(p.laws()[0].formula(), &fields),
            n & (1u128 << 127) != 0
        );
        assert!(holds(p.laws()[1].formula(), &fields));
    }
}
