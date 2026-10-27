//! Materialized-source compilation and root-based lowering parity. The original
//! graph is used only after proposing, for this compiled candidate's ABI/pool.

use super::{common::*, layout::Layout};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    expr::{Ast, Binary},
    model::{Contract, ScalarDomain},
    program_bytes,
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_loop_doors::{
    expr::{self, Expr, Op, Vocabulary},
    lower,
};
use zeno_fcis_synthesis::finite::{Domain, V2Resource, v2_zero_limits};

pub(super) struct Compilation {
    pub(super) original: Vec<u8>,
    pub(super) lowered: Option<Vec<u8>>,
    pub(super) inventory: Value,
    pub(super) refusal: Option<String>,
}
fn convert(node: &Ast, names: &BTreeMap<String, String>) -> Result<Expr> {
    Ok(match node {
        Ast::Bool(v) => Expr::Int(i128::from(*v)),
        Ast::Int(v) => Expr::Int(*v),
        Ast::Name(name) => Expr::Var(
            names
                .get(name)
                .cloned()
                .ok_or_else(|| gate("lowering-root-name"))?,
        ),
        Ast::Not(a) => Expr::negation(convert(a, names)?),
        Ast::Neg(a) => Expr::bin(Op::Sub, Expr::Int(0), convert(a, names)?),
        Ast::Choose(c, a, b) => {
            Expr::select(convert(c, names)?, convert(a, names)?, convert(b, names)?)
        }
        Ast::Binary(op, a, b) => {
            let op = match op {
                Binary::Implies => Op::Implies,
                Binary::Or => Op::Or,
                Binary::And => Op::And,
                Binary::Eq => Op::Eq,
                Binary::Ne => Op::Ne,
                Binary::Lt => Op::Lt,
                Binary::Le => Op::Le,
                Binary::Gt => Op::Gt,
                Binary::Ge => Op::Ge,
                Binary::Add => Op::Add,
                Binary::Sub => Op::Sub,
                Binary::Mul => return fail(1, "lowering-profile-3-or-4-unsupported"),
            };
            Expr::bin(op, convert(a, names)?, convert(b, names)?)
        }
        _ => return fail(1, "lowering-profile-3-or-4-unsupported"),
    })
}
fn domain(domain: ScalarDomain) -> Result<Domain> {
    Ok(match domain {
        ScalarDomain::Bool => Domain::Bool,
        ScalarDomain::Int { min, max } => Domain::Int { min, max },
        ScalarDomain::Unsigned { .. } => return fail(1, "lowering-unsigned-unsupported"),
    })
}

pub(super) fn compile(layout: Layout, project: &str, policy: &str) -> Result<Compilation> {
    let rules = Rules::read(policy)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let (contract, roots) = Contract::build_with_roots(&declarations, &rules, commitment)?;
    let original = contract.program()?;
    let original_bytes = program_bytes(&original)?;
    let max_outputs = if layout == Layout::T { 8 } else { 9 };
    let mut inventory = json!({"project_sha256":sha(project.as_bytes()),"policy_sha256":sha(policy.as_bytes()),"project_bytes":project.len(),"policy_bytes":policy.len(),
        "compiled_sha256":sha(&original_bytes),"compiled_bytes":original_bytes.len(),"nodes":original.nodes().len(),"inputs":original.inputs().len(),"outputs":original.outputs().len(),
        "profile":original.profile(),"root_count":roots.len(),
        "laws":contract.laws.iter().map(|l|json!({"id":l.id,"kind":l.kind.name(),"nodes":l.nodes.len(),"genesis":l.genesis})).collect::<Vec<_>>(),
        "authority_limits":{"read":contract.budgets.read,"write":contract.budgets.write,"effect":contract.budgets.effect,"byte":contract.budgets.byte,"step":contract.budgets.step},
        "root_expression_nodes":[],"root_expression_bytes":[]});
    let mut result = Compilation {
        original: original_bytes,
        lowered: None,
        inventory: Value::Null,
        refusal: None,
    };
    let lowered = (|| -> Result<Vec<u8>> {
        if original.nodes().len() > 224
            || original.inputs().len() > 6
            || original.outputs().len() > max_outputs
            || result.original.len() > SOURCE_LIMIT
        {
            return fail(1, "candidate-scalar-admission-limit");
        }
        if !original.profile().ends_with("/1") {
            return fail(1, "candidate-scalar-profile-unsupported");
        }
        if original.outputs().len() != roots.len()
            || roots
                .iter()
                .map(|r| domain(r.domain))
                .collect::<Result<Vec<_>>>()?
                != original.outputs()
        {
            return fail(1, "compiler-root-domain-mismatch");
        }
        let inputs = declarations.inputs()?;
        let map = inputs
            .iter()
            .enumerate()
            .map(|(j, input)| (input.name.clone(), format!("in{j}")))
            .collect();
        let names = (0..inputs.len()).map(|j| format!("in{j}")).collect();
        let vocab = Vocabulary::new(
            names,
            original.inputs().to_vec(),
            original.outputs().to_vec(),
            &BTreeMap::new(),
        )
        .map_err(|e| gate(e.to_string()))?;
        let mut expressions = Vec::new();
        let mut sizes = Vec::new();
        let mut bytes = Vec::new();
        for root in &roots {
            let expression = convert(&root.expression, &map)?;
            if expr::depth(&expression) > expr::MAX_DEPTH {
                return fail(1, "expanded-root-depth-limit");
            }
            let size = expr::size(&expression);
            let text = expr::print_symbolic(&expression);
            sizes.push(size);
            bytes.push(text.len());
            if size > expr::MAX_NODES || text.len() > expr::MAX_TEXT || !text.is_ascii() {
                inventory["root_expression_nodes"] = json!(sizes);
                inventory["root_expression_bytes"] = json!(bytes);
                return fail(1, "expanded-root-parser-limit");
            }
            if expr::has_holes(&expression) {
                return fail(1, "lowering-unfilled-root");
            }
            expressions.push(expression);
        }
        inventory["root_expression_nodes"] = json!(sizes);
        inventory["root_expression_bytes"] = json!(bytes);
        let lowered = lower::lower_outputs(&expressions, &vocab, &original)
            .map_err(|e| gate(e.to_string()))?;
        let bytes = program_bytes(&lowered)?;
        if bytes.len() > SOURCE_LIMIT
            || lowered.nodes().len() > 256
            || lowered.inputs().len() > 32
            || lowered.outputs().len() > 16
        {
            return fail(1, "lowered-profile-limit");
        }
        let mut expected_inputs: Vec<_> = layout
            .bounds()
            .into_iter()
            .map(|max| Domain::Int { min: 0, max })
            .collect();
        expected_inputs.extend([
            Domain::Int { min: 0, max: 15 },
            Domain::Int { min: 0, max: 0 },
        ]);
        if original.inputs() != expected_inputs {
            return fail(1, "candidate-input-domain-or-order");
        }
        let expected_names: Vec<_> = (0..layout.cells() + 1)
            .map(|j| format!("pre.100.{}", 110 + j))
            .chain(["command.101.120".to_string(), "context.102.130".to_string()])
            .collect();
        if inputs.iter().map(|i| i.name.as_str()).collect::<Vec<_>>()
            != expected_names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        {
            return fail(1, "candidate-input-binding-order");
        }
        // Existing transform equivalence also permits equal failures. The pilot
        // additionally forbids traps, so inspect both actual library outcomes
        // on every raw tuple. These are additional measured evaluations.
        let mut evaluations = 0u64;
        let mut steps = 0u128;
        for s in 0..layout.raw_count() {
            for q in 0..16 {
                let mut tuple = layout.raw(s);
                tuple.extend([q, 0]);
                for program in [&original, &lowered] {
                    let outcome = program
                        .execute_v2(&tuple, v2_zero_limits().with_limit(V2Resource::Step, 256));
                    let (value, usage) = outcome.into_parts();
                    evaluations += 1;
                    steps += u128::from(usage.used(V2Resource::Step));
                    inventory["scalar_totality"] = json!({"evaluations":evaluations,"steps":steps.to_string(),"complete":false});
                    if value.is_err() {
                        return fail(1, format!("scalar-trap:{s}/{q}"));
                    }
                }
            }
        }
        inventory["scalar_totality"] =
            json!({"evaluations":evaluations,"steps":steps.to_string(),"complete":true});
        inventory["lowered_sha256"] = json!(sha(&bytes));
        inventory["lowered_nodes"] = json!(lowered.nodes().len());
        inventory["lowered_bytes"] = json!(bytes.len());
        inventory["lowering"] = json!(if bytes == result.original {
            "lowering-parity/no-change"
        } else {
            "lowering-parity/reordered-graph"
        });
        Ok(bytes)
    })();
    match lowered {
        Ok(bytes) => result.lowered = Some(bytes),
        Err(error) => result.refusal = Some(error.reason),
    }
    result.inventory = inventory;
    Ok(result)
}
