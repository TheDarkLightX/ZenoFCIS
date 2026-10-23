//! An existential recovery witness for every law-consistent held state.
//! Unlike a game capability sample, this enumerates the entire declared state
//! carrier. It supplies one honest refund request and checks the actual Authority.
#![forbid(unsafe_code)]
use std::{fs, path::Path};
use zeno_fcis_contract::internals::{
    declarations::{Declarations, Source},
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, LawSet, Outcome},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("recovery check: {error:?}"))
}

#[test]
fn every_law_consistent_treasury_hold_has_an_agent_only_refund() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-cli/templates/agent-treasury-guard");
    let project = ok(fs::read_to_string(root.join("project.zeno")));
    let policy_text = ok(fs::read_to_string(root.join("v2/policy.json")));
    let rules = ok(Rules::read(&policy_text));
    let declarations = ok(Declarations::read(&project, &rules.leaf_bindings));
    let positions = ok(domain::positions(&declarations));
    let encoded = ok(schema::encode(&declarations));
    let commitment = ok(schema_commitment(&encoded));
    let contract = ok(Contract::build(&declarations, &rules, commitment));
    let framer = Framer::new(&positions, &contract);
    let width = positions
        .iter()
        .take_while(|p| p.source == Source::State)
        .count();
    let values: Vec<Vec<i128>> = positions[..width]
        .iter()
        .map(|p| {
            p.domain
                .values_within(100)
                .unwrap_or_else(|| panic!("state range changed"))
        })
        .collect();
    let count = values.iter().map(Vec::len).product::<usize>();
    assert!(count <= 5_000_000, "explicit exhaustive check cap");
    let index = |name: &str| {
        positions
            .iter()
            .position(|p| p.name == name)
            .unwrap_or_else(|| panic!("missing position {name}"))
    };
    let pending = index("pre.100.134");
    let held = index("pre.100.135");
    let seen = index("pre.100.133");
    let quote = index("pre.100.130");
    let base = index("pre.100.131");
    ok(policy::with_authority(&contract, &encoded, |authority| {
        let ids = evaluate::state_laws(authority.descriptor());
        assert_eq!(ids, [500, 510, 514]);
        let laws = LawSet::new(authority.descriptor(), &ids);
        let mut tuple: Vec<i128> = positions
            .iter()
            .map(|p| p.domain.least().unwrap_or_else(|| panic!("empty domain")))
            .collect();
        for (name, value) in [
            ("command.101.140", 185),
            ("context.102.150", 178),
            ("context.102.151", 11),
            ("context.102.154", 182),
        ] {
            tuple[index(name)] = value;
        }
        let mut checked = 0;
        for ordinal in 0..count {
            let mut residual = ordinal;
            for i in (0..width).rev() {
                tuple[i] = values[i][residual % values[i].len()];
                residual /= values[i].len();
            }
            if tuple[pending] == 170 || !laws.on_state(&framer, &tuple) {
                continue;
            }
            tuple[index("command.101.144")] = tuple[seen];
            let outcome = evaluate::evaluate(authority, &framer, &tuple);
            let Outcome::Decision(decision) = &outcome else {
                panic!("held state has no recovery: {tuple:?}: {outcome:?}")
            };
            assert_eq!(decision.class, Class::Accept, "{tuple:?}: {outcome:?}");
            let field = |id| {
                decision
                    .post
                    .iter()
                    .find(|(field, _)| *field == id)
                    .and_then(|(_, value)| value.number())
                    .unwrap_or_else(|| panic!("missing result"))
            };
            assert_eq!((field(134), field(135), field(136)), (170, 0, 0));
            assert_eq!(
                field(130),
                tuple[quote]
                    + if tuple[pending] == 171 {
                        tuple[held]
                    } else {
                        0
                    }
            );
            assert_eq!(
                field(131),
                tuple[base]
                    + if tuple[pending] == 172 {
                        tuple[held]
                    } else {
                        0
                    }
            );
            assert!(decision.outbox.is_empty());
            checked += 1;
        }
        assert!(
            checked > 100_000,
            "state coverage unexpectedly narrowed: {checked}"
        );
        eprintln!(
            "treasury recovery: declared_states={count}; law_consistent_held_states={checked}; stuck=0; caller=Agent; now=11; unlisted model; stale price; one step"
        );
    }));
}
