//! Shared source fixtures, never prewritten checker evidence.
use serde_json::{Value, json};

pub(crate) fn project(limit: i64) -> String {
    format!(
        r#"zeno 1;
project 1 scaled_cpmm;
namespace 2 core;
type 100 state Reserves;
type 101 command Swap;
type 102 context Environment;
type 105 int ReserveX in 0..={post_x};
type 106 int ReserveY in 0..={limit};
type 107 int Amount in 0..={limit};
field 110 100 x 105;
field 111 100 y 106;
field 120 101 amount 107;
reason 201 outside_scope precedence 0;
component 400 swap {{
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 256;
}}
merge [400];
law 500 nonnegative on commit, genesis = post.100.110 >= 0 && post.100.111 >= 0;
"#,
        post_x = 2 * limit
    )
}

pub(crate) fn policy(limit: i64, bps: i64, ceiling_output: bool) -> Value {
    let rounding = if ceiling_output {
        "mul_div_ceil"
    } else {
        "mul_div_floor"
    };
    json!({
        "schema":"zeno-fcis/template-declarative-policy/2","template":"scaled-cpmm",
        "roots":{"state":100,"command":101,"context":102},
        "leaf_bindings":{"102":["I128",0,0]},
        "variables":{
            "amount":"command.101.120", "x":"pre.100.110", "y":"pre.100.111",
            "fee":format!("mul_div_ceil(amount, {bps}, 10000)"),
            "net":"amount - fee", "den":"x + net",
            "safe_den":"choose(den == 0, 1, den)",
            "out":format!("{rounding}(net, y, safe_den)"),
        },
        "cases":[
            {"when":format!("x > {limit}"),"class":"Reject","reason":201,"post":{},"outbox":[]},
            {"when":true,"class":"Accept","reason":null,"post":{"110":"x + net","111":"y - out"},"outbox":[]},
        ],
        "genesis":{"110":0,"111":0},"law_kinds":{"500":"StateInvariant"},
    })
}

pub(crate) const RELATION: &str =
    "products_ge(post.100.110, post.100.111, pre.100.110, pre.100.111)";
