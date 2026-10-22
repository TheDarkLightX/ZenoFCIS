//! Small declared arithmetic policies, not prewritten judge evidence.
use serde_json::{Value, json};

pub(super) const FLOOR: &str = "mul_div_floor(amount, 3, 2)";
pub(super) const CEIL: &str = "mul_div_ceil(amount, 3, 2)";

pub(super) fn project() -> &'static str {
    r#"zeno 1;
project 1 arithmetic_judges;
namespace 2 core;
type 100 state Counter;
type 101 command Request;
type 102 context Environment;
type 105 int Value in 0..=3;
type 106 int Amount in 0..=2;
field 110 100 value 105;
field 120 101 amount 106;
reason 201 outside_scope precedence 0;
component 400 calculate {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 256;
}
merge [400];
law 500 nonnegative on commit, genesis = post.100.110 >= 0;
"#
}

pub(super) fn policy(successor: &str, condition: &str) -> Value {
    json!({
        "schema":"zeno-fcis/template-declarative-policy/2", "template":"arithmetic-judges",
        "roots":{"state":100,"command":101,"context":102}, "leaf_bindings":{"102":["I128",0,0]},
        "variables":{"amount":"command.101.120", "value":"pre.100.110"},
        "cases":[
            {"when":condition,"class":"Accept","reason":null,"post":{"110":successor},"outbox":[]},
            {"when":true,"class":"Reject","reason":201,"post":{},"outbox":[]},
        ],
        "genesis":{"110":0}, "law_kinds":{"500":"StateInvariant"},
    })
}
