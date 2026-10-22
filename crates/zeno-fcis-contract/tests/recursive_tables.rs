//! Native invocation metering and refusal checks for shared recursive formulas.

use serde_json::json;
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{domain, evaluate::Framer},
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{V2Resource, v2_composition as c, v2_laws as laws};

fn execute(recursive: bool, limit: Option<u64>) -> (bool, u64, bool) {
    let source = format!("zeno 1; project 1 recursive_meter;
        type 100 state S; type 101 command C; type 102 context X;
        type 105 int V in 0..=1; table 110 100 flags 105 size 4;
        field 120 101 flag 105;
        component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400];
        derived table 700 x[i] in table 110 rounds 4 {{
            when true then pre.100.110[i] == 1 || exists j in table 110 {{ x[j] }};
            default false;
        }};
        law 500 domain on commit, genesis = post.100.110 >= 0;
        law 501 query on commit = {};",if recursive { "x[0] || !x[0]" } else { "true" });
    let raw = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"recursive-meter",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},
        "cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":"pre.100.110","111":"pre.100.111","112":"pre.100.112","113":"pre.100.113"},"outbox":[]}],
        "genesis":{"110":0,"111":0,"112":0,"113":0},"law_kinds":{"500":"StateInvariant","501":"DebitCreditEffectEquality"}});
    let rules = Rules::read(&raw.to_string())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let declarations = Declarations::read(&source, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let mut contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    if let Some(limit) = limit {
        contract.budgets.step = limit;
    }
    let positions = domain::positions(&declarations)
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let framer = Framer::new(&positions, &contract);
    let tuple = vec![0; positions.len()];
    let original = tuple.clone();
    let frames = framer.frame(&tuple);
    let saved = frames.clone();
    let result = policy::with_authority(&contract, &schema, |authority| {
        let evaluation = authority.evaluate(c::Raw {
            state: &frames[0],
            command: &frames[1],
            context: &frames[2],
        });
        let budget = evaluation.diagnostics().iter().any(|d| {
            d.id == 501 && matches!(d.verdict, laws::Verdict::Refused(laws::Failure::Budget(_)))
        });
        (
            evaluation.result().is_ok(),
            evaluation.usage().used(V2Resource::Step),
            budget,
        )
    })
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(tuple, original);
    assert_eq!(frames, saved);
    result
}

#[test]
fn recursive_round_work_uses_the_actual_invocation_meter() {
    let (plain_ok, plain_work, _) = execute(false, None);
    let (recursive_ok, recursive_work, _) = execute(true, None);
    assert!(plain_ok && recursive_ok);
    assert!(
        recursive_work > plain_work,
        "recursive graph work must be charged"
    );
    let (ok, used, at_query) = execute(true, Some(plain_work));
    assert!(!ok, "the smaller Step limit must expose no candidate");
    assert!(
        at_query,
        "the actual recursive law must report a budget refusal"
    );
    assert!(
        used <= plain_work,
        "denied work must preserve consumed-work accounting"
    );
}

fn execute_bits(recursive: bool, limit: Option<u64>) -> (bool, u64, bool) {
    use zeno_fcis_synthesis::finite::{V2NumericAtom as Atom, canonical_v2::output};
    let source=format!("zeno 1; project 1 bits_meter; type 100 state S; type 101 command C; type 102 context X;
      type 105 bits Permissions width 2; type 106 int Zero in 0..=0; table 110 100 grants 105 size 2; field 120 101 query 106;
      component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 8192; }} merge [400];
      derived table 700 x[i] in table 110 bits 2 rounds 4 {{ when true then join(pre.100.110[i],join j in table 110 {{x[j]}}); default u128(0); }};
      law 500 domain on commit, genesis = post.100.110 >= u128(0); law 501 query on commit = {};",if recursive {"subset(2,x[0],x[0])"}else{"true"});
    let raw = json!({"schema":"zeno-fcis/template-declarative-policy/4","template":"bits-meter","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":"pre.100.110","111":"pre.100.111"},"outbox":[]}],"genesis":{"110":"u128(0)","111":"u128(0)"},"law_kinds":{"500":"StateInvariant","501":"DebitCreditEffectEquality"}});
    let rules = Rules::read(&raw.to_string())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let declarations = Declarations::read(&source, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    let mut contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    if let Some(limit) = limit {
        contract.budgets.step = limit;
    }
    let roots = [
        vec![
            c::Field {
                id: 110,
                value: Atom::U128(1),
            },
            c::Field {
                id: 111,
                value: Atom::U128(2),
            },
        ],
        vec![c::Field {
            id: 120,
            value: Atom::I128(0),
        }],
        vec![],
    ];
    let mut frames = [Vec::new(), Vec::new(), Vec::new()];
    for i in 0..3 {
        let max = contract.frame_bytes[i] as usize;
        let body = if i == 2 {
            output::encode_atom(Atom::Bool(false), max)
        } else {
            output::encode_record(&roots[i], max)
        }
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        frames[i] = output::encode_envelope(100 + i as u32, &contract.commitment, &body, max)
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    }
    let saved = frames.clone();
    let result = policy::with_numeric_authority(&contract, &schema, |a| {
        let e = a.evaluate(c::Raw {
            state: &frames[0],
            command: &frames[1],
            context: &frames[2],
        });
        (
            e.result().is_ok(),
            e.usage().used(V2Resource::Step),
            e.diagnostics().iter().any(|d| {
                d.id == 501 && matches!(d.verdict, laws::Verdict::Refused(laws::Failure::Budget(_)))
            }),
        )
    })
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(frames, saved);
    result
}

#[test]
fn atom_lift_work_is_metered_by_the_numeric_authority() {
    let (plain, baseline, _) = execute_bits(false, None);
    let (recursive, work, _) = execute_bits(true, None);
    assert!(plain && recursive);
    assert!(work > baseline, "lifted atoms consume actual work");
    let (ok, used, law_refusal) = execute_bits(true, Some(baseline));
    assert!(
        !ok && law_refusal,
        "low budget must refuse the recursive law without a candidate"
    );
    assert!(used <= baseline);
}

#[test]
fn constant_lookup_is_bound_into_policy_identity_without_extra_inputs() {
    fn identity(parent: u32) -> (Vec<u8>, Vec<u8>, usize) {
        let source=format!("zeno 1; project 1 constant_identity; type 100 state S; type 101 command C; type 102 context X;
          type 105 int Parent in 0..=2; type 106 int Flag in 0..=1;
          const table 110 parents 105 size 2 [0,{parent}]; field 210 100 flag 106; field 120 101 query 106;
          component 400 core {{owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096;}} merge [400];
          derived table 700 reach[i][j] in table 110 rounds 4 {{when true then i==j || exists k in table 110 {{parents[i]==k+1 && reach[k][j]}}; default false;}};
          law 500 domain on commit, genesis = post.100.210 >= 0; law 501 query on commit = reach[1][0] || pre.100.210==0;");
        let raw = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"constant-identity","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"210":"pre.100.210"},"outbox":[]}],"genesis":{"210":0},"law_kinds":{"500":"StateInvariant","501":"DebitCreditEffectEquality"}});
        let rules = Rules::read(&raw.to_string())
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        let d = Declarations::read(&source, &rules.leaf_bindings)
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        let schema = schema::encode(&d)
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        let contract = Contract::build(
            &d,
            &rules,
            schema_commitment(&schema)
                .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        let identity = policy::with_authority(&contract, &schema, |a| a.identity().to_vec())
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
        (schema, identity, contract.bindings.len())
    }
    let (schema_a, a, inputs_a) = identity(0);
    let (schema_b, b, inputs_b) = identity(1);
    assert_eq!(
        schema_a, schema_b,
        "changing policy data must not change stored/input schema"
    );
    assert_eq!((inputs_a, inputs_b), (3, 3));
    assert_ne!(
        a, b,
        "effective lookup changes must be bound into native policy identity"
    );
}
