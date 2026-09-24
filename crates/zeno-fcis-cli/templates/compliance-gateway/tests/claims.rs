//! Checks that the claims in `project.zeno` are what the README says: each
//! claim assumes one law's formula verbatim, with the action it concerns and
//! the bounds before the decision, and concludes law 500's formula verbatim.
//!
//! A claim proves its formula for every assignment of the observations it
//! reads, with no model of this application. This test ties the hypotheses to
//! the laws; `tests/conformance.rs` ties them to the program by evaluating
//! them on every decision it commits.

use compliance_gateway::profile;
use zeno_fcis_spec::{
    BackendId, ClaimDecl, ClaimFormula, ClaimMode, LawDecl, ProjectSpec, RelExpr,
};

/// The state invariant every claim concludes.
const STRIKES_WITHIN_BOUNDS: u32 = 500;
/// The claims, each with the law whose formula it assumes.
const CLAIMS: [(u32, u32); 3] = [(600, 501), (601, 502), (602, 503)];

fn law(project: &ProjectSpec, id: u32) -> &LawDecl {
    project
        .laws()
        .iter()
        .find(|law| law.id().get() == id)
        .unwrap_or_else(|| panic!("law {id} is declared"))
}

fn claim(project: &ProjectSpec, id: u32) -> &ClaimDecl {
    project
        .claims()
        .iter()
        .find(|claim| claim.id().get() == id)
        .unwrap_or_else(|| panic!("claim {id} is declared"))
}

/// The conjuncts of an `&&` chain, in order.
fn conjuncts(formula: &RelExpr) -> Vec<&RelExpr> {
    match formula {
        RelExpr::And(left, right) => [conjuncts(left), conjuncts(right)].concat(),
        other => vec![other],
    }
}

#[test]
fn each_claim_assumes_one_law_verbatim_and_concludes_the_invariant() {
    let project = profile::project();
    let invariant = law(&project, STRIKES_WITHIN_BOUNDS).formula();
    assert_eq!(project.claims().len(), CLAIMS.len());
    for (claim_id, law_id) in CLAIMS {
        let claim = claim(&project, claim_id);
        assert_eq!(claim.mode(), ClaimMode::Relational, "claim {claim_id}");
        for backend in [BackendId::Cvc5, BackendId::Z3] {
            assert!(
                claim.backends().contains(&backend),
                "claim {claim_id} selects {backend:?}"
            );
        }
        let ClaimFormula::Relational(RelExpr::Implies(hypotheses, conclusion)) = claim.formula()
        else {
            panic!("claim {claim_id} is a relational implication");
        };
        assert_eq!(
            **conclusion, *invariant,
            "claim {claim_id} concludes law 500"
        );
        // A law that is itself an `&&` chain parses into the same nested
        // `And` nodes as the claim's chain, so compare conjunct by conjunct.
        let assumed = conjuncts(law(&project, law_id).formula());
        let stated = conjuncts(hypotheses);
        assert!(
            assumed.iter().all(|part| stated.contains(part)),
            "claim {claim_id} assumes law {law_id} verbatim: {stated:?} lacks part of {assumed:?}"
        );
    }
}
