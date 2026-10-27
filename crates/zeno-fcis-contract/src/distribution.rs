//! Exact finite distribution laws, conditional on uniform independent bits.
//!
//! Each non-random tuple is a separate fiber. One fold visits every random
//! assignment with exact weight `1/N`, retaining counts and preimages. An
//! accepted decision's outcome is its projected successor value; every
//! committed failure is the one outcome `committed-failure`. A reject has no
//! outcome, so its fiber is UNKNOWN. No sampling, conditioning on acceptance,
//! or retry normalization is performed.
//! The histogram fold is the future extension point for other weight algebras;
//! Reward measures reuse the same histograms with bounded signed rationals.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use zeno_fcis_spec::{
    DistributionDecl, DistributionOutcome, Probability, ProjectionPath, ProjectionRoot, Rational,
    RewardComparison, RewardDecl, RewardKind, RewardMeasure,
};

use crate::ContractError;
use crate::internals::{
    declarations::{Declarations, Kind},
    model::Contract,
    policy,
    review::{
        domain::{self, LeafDomain, Position},
        evaluate::{self, Fields, Framer, Outcome},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};

/// Every verdict retains this external premise; the checker cannot certify a
/// physical entropy source or its independence from the other inputs.
pub const PREMISE: &str =
    "given uniform independent random bits, independent of the fixed non-random inputs";
/// Default and hard tuple cap, matching the finite contract-review default.
pub const MAX_TUPLES: u64 = 1 << 20;
/// Default work cap: an invocation and each projection/fold charge one unit.
pub const MAX_WORK: u64 = 1 << 24;

/// Caller limits, bounded by the finite checker caps.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Complete input tuples, across random and non-random positions.
    pub max_tuples: u64,
    /// Invocations plus outcome projections/folds, including refused work.
    pub max_work: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tuples: MAX_TUPLES,
            max_work: MAX_WORK,
        }
    }
}

/// A finite law verdict; UNKNOWN admits no distribution claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every fiber and law compared exactly.
    Checked,
    /// One complete fiber differs from its target.
    Refuted,
    /// A limit, refusal or undefined projection prevented completion.
    Unknown,
}

impl Verdict {
    /// Stable packet spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Checked => "checked",
            Self::Refuted => "refuted",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Owned result with immutable access. It grants no runtime authority.
#[derive(Clone, Debug)]
pub struct CheckResult {
    verdict: Verdict,
    packet: Value,
}

impl CheckResult {
    /// The complete or incomplete verdict.
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }
    /// The result, including the premise, measured work and source identities.
    pub const fn packet(&self) -> &Value {
        &self.packet
    }
}

struct Progress {
    packet: Value,
    tuples: u64,
    work: u64,
    fibers: u64,
}

impl Progress {
    fn finish(mut self, verdict: Verdict, detail: Value) -> CheckResult {
        self.packet["status"] = json!(verdict.name());
        self.packet["claim"] = if verdict == Verdict::Checked {
            json!(
                "exactly checked over the declared finite domain, given uniform independent random bits"
            )
        } else {
            Value::Null
        };
        self.packet["measured"] =
            json!({"tuples":self.tuples,"work":self.work,"complete_fibers":self.fibers});
        self.packet["result"] = detail;
        CheckResult {
            verdict,
            packet: self.packet,
        }
    }
}

/// Checks every declared distribution law on the contract's bound Authority.
/// Invalid syntax, random widths or targets are errors. Execution or resource
/// incompleteness is UNKNOWN, with its premise and consumed work retained.
pub fn check(
    project: &str,
    policy_source: &str,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    let invalid = |reason| ContractError::new("distribution laws", reason);
    let spec = zeno_fcis_spec::parse_project(project, Default::default())
        .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
        .map_err(|e| invalid(e.to_string()))?;
    if spec.random_inputs().is_empty()
        || (spec.distributions().is_empty() && spec.rewards().is_empty())
    {
        return Err(invalid(
            "declare random inputs and at least one distribution or reward law".to_owned(),
        ));
    }
    let rules = Rules::read_for_project(project, policy_source)?;
    rules.refuse_unsigned("finite distribution binding")?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let positions = domain::positions(&declarations)?;
    let mut random = BTreeMap::new();
    for input in spec.random_inputs() {
        let name = path_name(input.input());
        let index = positions
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| invalid(format!("{name} is not a scalar input")))?;
        if !matches!(
            input.input().root(),
            ProjectionRoot::Command | ProjectionRoot::Context
        ) {
            return Err(invalid(
                "random bits must be command or context inputs".to_owned(),
            ));
        }
        let max = 1u128
            .checked_shl(u32::from(input.bits()))
            .and_then(|n| i128::try_from(n - 1).ok());
        let width_matches = match positions[index].domain {
            LeafDomain::Bool => input.bits() == 1,
            LeafDomain::Int { min, max: upper } => min == 0 && Some(upper) == max,
            LeafDomain::Sum { .. } => false,
        };
        if !width_matches {
            return Err(invalid(format!(
                "{name}: random bit width does not match the complete input domain"
            )));
        }
        if random.insert(index, input.bits()).is_some() {
            return Err(invalid(format!("{name}: duplicate random input")));
        }
    }
    let mut projections = Vec::new();
    let mut reward_indices = Vec::new();
    for (path, is_reward) in spec
        .distributions()
        .iter()
        .map(|v| (v.outcome(), false))
        .chain(spec.rewards().iter().map(|v| (v.outcome(), true)))
    {
        // The supported scalar contract shape uses one state record field.
        if path.root() != ProjectionRoot::Post
            || path.segments().len() != 2
            || path.segments()[0].get() != 100
        {
            return Err(invalid(
                "outcome must be a scalar post.100.FIELD projection".to_owned(),
            ));
        }
        let field = u16::try_from(path.segments()[1].get())
            .map_err(|_| invalid("outcome field is too wide".to_owned()))?;
        if is_reward {
            let declared = declarations
                .state_fields()?
                .iter()
                .find(|v| v.id == field)
                .ok_or_else(|| invalid("reward field is absent".to_owned()))?;
            if !matches!(declarations.kind(declared.type_id)?, Kind::I128 { .. }) {
                return Err(invalid(
                    "reward outcome must be an integer field".to_owned(),
                ));
            }
            let index = match projections.iter().position(|v| *v == field) {
                Some(index) => index,
                None => {
                    projections.push(field);
                    projections.len() - 1
                }
            };
            reward_indices.push(index);
        } else {
            projections.push(field);
        }
    }
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let framer = Framer::new(&positions, &contract);
    let random_indices: Vec<usize> = random.keys().copied().collect();
    let fixed_indices: Vec<usize> = (0..positions.len())
        .filter(|i| !random.contains_key(i))
        .collect();
    let random_size = random_indices
        .iter()
        .try_fold(1u128, |n, i| n.checked_mul(positions[*i].domain.size()?));
    let size = domain::domain_size(&positions);
    let sha = |s: &str| evaluate::hex(&evaluate::digest(s.as_bytes()));
    let mut progress = Progress {
        packet: json!({
            "schema":"zeno-fcis/distribution-laws/1", "authority":"none",
            "premise": PREMISE,
            "sources":{"project_sha256":sha(project),"rules_sha256":sha(policy_source)},
            "checker":crate::checked_composition::identity::Identity::current().json(),
            "limits":{"max_tuples":limits.max_tuples.min(MAX_TUPLES),"max_work":limits.max_work.min(MAX_WORK)},
            "domain_size":size.map(|v|v.to_string()), "random_domain_size":random_size.map(|v|v.to_string()),
            "random_inputs":random_indices.iter().map(|i|json!({"input":positions[*i].name,"bits":random[i],"uniform":true,"independent":true})).collect::<Vec<_>>(),
            "non_random_inputs":fixed_indices.iter().map(|i|positions[*i].name.clone()).collect::<Vec<_>>(),
            "laws":spec.distributions().iter().map(|law|json!({"id":law.id().get(),"name":law.name().as_str(),"outcome":path_name(law.outcome()),"expected":distribution_json(law.target().iter().copied())})).collect::<Vec<_>>(),
            "rewards":spec.rewards().iter().map(|law|json!({"id":law.id().get(),"name":law.name().as_str(),"random":law.random().iter().map(|v| v.get()).collect::<Vec<_>>(),"outcome":path_name(law.outcome()),"failure_reward":law.failure().map(|v|v.to_string()),"measures":law.measures().iter().map(|m| measure_json(*m)).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "work_unit":"one Authority invocation, each projection/fold, each reward moment bin and comparison, and each witness bin"
        }),
        tuples: 0,
        work: 0,
        fibers: 0,
    };
    if size.is_none_or(|n| n == 0 || n > u128::from(limits.max_tuples.min(MAX_TUPLES))) {
        return Ok(progress.finish(
            Verdict::Unknown,
            json!({"reason":"domain budget exceeded or empty"}),
        ));
    }
    let random_size =
        u64::try_from(random_size.ok_or_else(|| invalid("random domain overflow".to_owned()))?)
            .map_err(|_| invalid("random domain overflow".to_owned()))?;
    let weight = Probability::try_new(1, u128::from(random_size))
        .ok_or_else(|| invalid("empty random domain".to_owned()))?;
    let mut tuple: Vec<i128> = positions
        .iter()
        .map(|p| p.domain.least().unwrap_or(0))
        .collect();
    let work_cap = limits.max_work.min(MAX_WORK);
    policy::with_authority(&contract, &schema, |authority| {
        // Committed failures in the current fiber.
        let mut failures = 0u64;
        loop {
            let mut bins: Vec<BTreeMap<DistributionOutcome, Bin>> =
                projections.iter().map(|_| BTreeMap::new()).collect();
            let mut visited = 0u64;
            loop {
                if progress.work >= work_cap {
                    return progress.finish(
                        Verdict::Unknown,
                        json!({"reason":"work budget exceeded","partial_random_values":visited}),
                    );
                }
                progress.work += 1;
                progress.tuples += 1;
                let decision = match evaluate::evaluate(authority, &framer, &tuple) {
                    Outcome::Decision(d) => d,
                    Outcome::Refused(r) => return progress.finish(Verdict::Unknown, json!({"reason":"Authority refused","refusal":r.to_string(),"input":input_json(&positions,&tuple)})),
                };
                failures += u64::from(decision.class == Class::CommittedFailure);
                for (field, histogram) in projections.iter().zip(&mut bins) {
                    if progress.work >= work_cap {
                        return progress.finish(Verdict::Unknown, json!({"reason":"work budget exceeded","partial_random_values":visited}));
                    }
                    progress.work += 1;
                    let outcome = match decision.class {
                        Class::Accept => projected(&decision.post, *field),
                        Class::CommittedFailure => Some(DistributionOutcome::CommittedFailure),
                        Class::Reject => None,
                    };
                    let Some(outcome) = outcome else {
                        return progress.finish(Verdict::Unknown, json!({"reason":"undefined outcome projection","decision_kind":decision.class.name(),"input":input_json(&positions,&tuple)}));
                    };
                    let bin = histogram.entry(outcome).or_insert_with(Bin::zero);
                    let Some(mass) = bin.mass.checked_add(weight) else {
                        return progress.finish(
                            Verdict::Unknown,
                            json!({"reason":"exact weight arithmetic bound"}),
                        );
                    };
                    bin.mass = mass;
                    bin.preimages
                        .push(random_indices.iter().map(|i| tuple[*i]).collect());
                }
                visited += 1;
                if !advance(&positions, &random_indices, &mut tuple) {
                    break;
                }
            }
            if visited != random_size {
                return progress.finish(
                    Verdict::Unknown,
                    json!({"reason":"incomplete random domain","visited":visited}),
                );
            }
            progress.fibers += 1;
            for (law, histogram) in spec.distributions().iter().zip(&bins) {
                let actual: Vec<_> = histogram
                    .iter()
                    .map(|(value, bin)| (*value, bin.mass))
                    .collect();
                if actual != law.target() {
                    let fixed: Vec<_> = fixed_indices
                        .iter()
                        .map(|i| json!({"input":positions[*i].name,"value":tuple[*i].to_string()}))
                        .collect();
                    return progress.finish(
                        Verdict::Refuted,
                        json!({
                            "law":law.id().get(), "non_random_input":fixed,
                            "expected":distribution_json(law.target().iter().copied()),
                            "actual":distribution_json(actual.into_iter()),
                            "witness":minimal_witness(law,histogram,random_size),
                            "decision_mass":decision_mass(failures,random_size),
                        }),
                    );
                }
            }
            for (law, index) in spec.rewards().iter().zip(&reward_indices) {
                let histogram = &bins[*index];
                let fixed = || {
                    fixed_indices
                        .iter()
                        .map(|i| json!({"input":positions[*i].name,"value":tuple[*i].to_string()}))
                        .collect::<Vec<_>>()
                };
                if law.failure().is_none()
                    && let Some(bin) = histogram.get(&DistributionOutcome::CommittedFailure)
                {
                    if let Err(reason) = charge(&mut progress.work, work_cap) {
                        return progress.finish(
                            Verdict::Unknown,
                            json!({"reason":reason,"law":law.id().get()}),
                        );
                    }
                    return progress.finish(Verdict::Refuted, json!({"law":law.id().get(),"reason":"committed failure has no reward","measure":"reward-defined","actual":null,"bound":null,"non_random_input":fixed(),"outcomes":[{"outcome":"committed-failure","mass":probability_json(bin.mass),"random_values":preimages_json(bin)}],"decision_mass":decision_mass(failures,random_size)}));
                }
                for measure in law.measures() {
                    let (actual, mean) = match reward_actual(law, *measure, histogram, &mut progress.work, work_cap) {
                        Ok(values) => values,
                        Err(reason) => return progress.finish(Verdict::Unknown, json!({"reason":reason,"law":law.id().get(),"non_random_input":fixed()})),
                    };
                    if !measure.comparison().holds(actual, measure.bound()) {
                        let outcomes = match reward_witness(law, *measure, histogram, actual, mean, &mut progress.work, work_cap) {
                            Ok(outcomes) => outcomes,
                            Err(reason) => return progress.finish(Verdict::Unknown, json!({"reason":reason,"law":law.id().get(),"non_random_input":fixed()})),
                        };
                        return progress.finish(Verdict::Refuted, json!({"law":law.id().get(),"non_random_input":fixed(),"measure":measure_json(*measure),"actual":rational_json(actual),"bound":rational_json(measure.bound()),"outcomes":outcomes,"witness_kind":"all outcomes ranked by signed contribution to actual minus bound, largest violation first","decision_mass":decision_mass(failures,random_size)}));
                    }
                }
            }
            if !advance(&positions, &fixed_indices, &mut tuple) {
                break;
            }
            failures = 0;
        }
        if Some(u128::from(progress.tuples)) != size {
            return progress.finish(
                Verdict::Unknown,
                json!({"reason":"incomplete input domain"}),
            );
        }
        // Reward bounds need not fix failure mass across fibers. State the
        // scope explicitly rather than generalizing the last fiber's mass.
        progress.finish(
            Verdict::Checked,
            json!({"all_fibers_compared":true,"decision_mass":decision_mass(failures,random_size),"decision_mass_scope":"last complete fiber"}),
        )
    })
}

fn charge(work: &mut u64, cap: u64) -> Result<(), &'static str> {
    if *work >= cap {
        return Err("work budget exceeded");
    }
    *work += 1;
    Ok(())
}
fn integer(value: i128) -> Option<Rational> {
    Rational::try_new(value, 1)
}
fn reward(law: &RewardDecl, outcome: DistributionOutcome) -> Option<i128> {
    match outcome {
        DistributionOutcome::Value(value) => Some(value),
        DistributionOutcome::CommittedFailure => law.failure(),
    }
}
fn reward_term(kind: RewardKind, value: i128) -> Option<Rational> {
    match kind {
        RewardKind::Expectation => integer(value),
        RewardKind::Variance => integer(value)?.checked_mul(integer(value)?),
        RewardKind::Threshold(t) => integer(i128::from(value >= t)),
        RewardKind::Excess(b) => integer(value.checked_sub(b)?.max(0)),
    }
}
fn weighted(term: Rational, bin: &Bin) -> Option<Rational> {
    let weight = Rational::try_new(
        i128::from(bin.mass.numerator()),
        u128::from(bin.mass.denominator()),
    )?;
    term.checked_mul(weight)
}
fn reward_moment(
    law: &RewardDecl,
    kind: RewardKind,
    histogram: &BTreeMap<DistributionOutcome, Bin>,
    work: &mut u64,
    cap: u64,
) -> Result<Rational, &'static str> {
    let mut sum = Rational::ZERO;
    for (outcome, bin) in histogram {
        charge(work, cap)?;
        sum = (|| sum.checked_add(weighted(reward_term(kind, reward(law, *outcome)?)?, bin)?))()
            .ok_or("reward arithmetic bound")?;
    }
    Ok(sum)
}
fn reward_actual(
    law: &RewardDecl,
    measure: RewardMeasure,
    histogram: &BTreeMap<DistributionOutcome, Bin>,
    work: &mut u64,
    cap: u64,
) -> Result<(Rational, Rational), &'static str> {
    let moment = reward_moment(law, measure.kind(), histogram, work, cap)?;
    let mean = if measure.kind() == RewardKind::Variance {
        reward_moment(law, RewardKind::Expectation, histogram, work, cap)?
    } else {
        Rational::ZERO
    };
    charge(work, cap)?;
    let actual = if measure.kind() == RewardKind::Variance {
        moment
            .checked_sub(mean.checked_mul(mean).ok_or("reward arithmetic bound")?)
            .ok_or("reward arithmetic bound")?
    } else {
        moment
    };
    Ok((actual, mean))
}
fn reward_witness(
    law: &RewardDecl,
    measure: RewardMeasure,
    histogram: &BTreeMap<DistributionOutcome, Bin>,
    actual: Rational,
    mean: Rational,
    work: &mut u64,
    cap: u64,
) -> Result<Value, &'static str> {
    let mut rows = Vec::new();
    for (outcome, bin) in histogram {
        charge(work, cap)?;
        let value = reward(law, *outcome).ok_or("undefined reward")?;
        let contribution = (|| {
            let term = if measure.kind() == RewardKind::Variance {
                let centered = integer(value)?.checked_sub(mean)?;
                centered.checked_mul(centered)?
            } else {
                reward_term(measure.kind(), value)?
            };
            weighted(term.checked_sub(measure.bound())?, bin)
        })()
        .ok_or("reward witness arithmetic bound")?;
        rows.push((contribution, *outcome, json!({"outcome":outcome_name(*outcome),"reward":value.to_string(),"mass":probability_json(bin.mass),"difference_contribution":rational_json(contribution),"random_values":preimages_json(bin)})));
    }
    rows.sort_by(|a, b| {
        let order = if actual > measure.bound() {
            b.0.cmp(&a.0)
        } else {
            a.0.cmp(&b.0)
        };
        order.then(a.1.cmp(&b.1))
    });
    Ok(json!(
        rows.into_iter().map(|(_, _, row)| row).collect::<Vec<_>>()
    ))
}
fn rational_json(value: Rational) -> Value {
    json!({"numerator":value.numerator().to_string(),"denominator":value.denominator().to_string()})
}
fn probability_json(value: Probability) -> Value {
    json!({"numerator":value.numerator().to_string(),"denominator":value.denominator().to_string()})
}
fn preimages_json(bin: &Bin) -> Value {
    json!(
        bin.preimages
            .iter()
            .map(|v| v.iter().map(ToString::to_string).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}
fn measure_json(measure: RewardMeasure) -> Value {
    let (kind, parameter) = match measure.kind() {
        RewardKind::Expectation => ("expectation", None),
        RewardKind::Variance => ("variance", None),
        RewardKind::Threshold(t) => ("threshold", Some(t.to_string())),
        RewardKind::Excess(b) => ("excess", Some(b.to_string())),
    };
    let comparison = match measure.comparison() {
        RewardComparison::AtMost => "<=",
        RewardComparison::AtLeast => ">=",
        RewardComparison::Equal => "=",
    };
    json!({"kind":kind,"parameter":parameter,"comparison":comparison,"bound":rational_json(measure.bound())})
}

struct Bin {
    mass: Probability,
    preimages: Vec<Vec<i128>>,
}
impl Bin {
    fn zero() -> Self {
        Self {
            mass: Probability::ZERO,
            preimages: Vec::new(),
        }
    }
}

// A cardinality-minimal positive witness: m assignments producing one outcome
// certify excess when m/N > p. Thus m=floor(N*p)+1. Minimize m over excess
// outcomes, breaking ties by outcome then by enumeration order of assignments.
fn minimal_witness(
    law: &DistributionDecl,
    bins: &BTreeMap<DistributionOutcome, Bin>,
    n: u64,
) -> Value {
    let witness = bins
        .iter()
        .filter_map(|(value, bin)| {
            let p = law
                .target()
                .iter()
                .find(|(v, _)| v == value)
                .map_or(Probability::ZERO, |(_, p)| *p);
            let needed =
                u128::from(n) * u128::from(p.numerator()) / u128::from(p.denominator()) + 1;
            let needed = usize::try_from(needed).ok()?;
            (bin.preimages.len() >= needed).then_some((needed, *value, bin))
        })
        .min_by_key(|(needed, value, _)| (*needed, *value));
    match witness {
        Some((needed, value, bin)) => {
            json!({"kind":"minimal-single-outcome-excess","outcome":outcome_name(value),"size":needed,"random_values":bin.preimages[..needed].iter().map(|v|v.iter().map(ToString::to_string).collect::<Vec<_>>()).collect::<Vec<_>>(),"random_domain_size":n})
        }
        None => json!({"kind":"no-positive-witness"}),
    }
}

fn path_name(path: &ProjectionPath) -> String {
    let root = match path.root() {
        ProjectionRoot::Pre => "pre",
        ProjectionRoot::Post => "post",
        ProjectionRoot::Command => "command",
        ProjectionRoot::Context => "context",
        _ => "unsupported",
    };
    format!(
        "{root}.{}",
        path.segments()
            .iter()
            .map(|s| s.get().to_string())
            .collect::<Vec<_>>()
            .join(".")
    )
}
fn projected(post: &Fields, field: u16) -> Option<DistributionOutcome> {
    post.iter()
        .find(|(id, _)| *id == field)
        .and_then(|(_, v)| v.number())
        .map(DistributionOutcome::Value)
}
fn outcome_name(outcome: DistributionOutcome) -> String {
    match outcome {
        DistributionOutcome::Value(value) => value.to_string(),
        DistributionOutcome::CommittedFailure => "committed-failure".to_owned(),
    }
}
fn distribution_json(values: impl Iterator<Item = (DistributionOutcome, Probability)>) -> Value {
    json!(values.map(|(value,p)|json!({"value":outcome_name(value),"numerator":p.numerator().to_string(),"denominator":p.denominator().to_string()})).collect::<Vec<_>>())
}
// The mass of each decision kind at one complete fiber of n assignments. A
// complete fiber has no reject, which has no outcome and stops the fold.
fn decision_mass(failures: u64, n: u64) -> Value {
    json!(Class::ALL.iter().map(|class| {
        let count = match class {
            Class::Accept => n - failures,
            Class::CommittedFailure => failures,
            Class::Reject => 0,
        };
        let p = Probability::try_new(count.into(), n.into()).unwrap_or(Probability::ZERO);
        json!({"kind":class.name(),"numerator":p.numerator().to_string(),"denominator":p.denominator().to_string()})
    }).collect::<Vec<_>>())
}
fn input_json(positions: &[Position], tuple: &[i128]) -> Value {
    json!(
        positions
            .iter()
            .zip(tuple)
            .map(|(p, v)| (p.name.clone(), json!(v.to_string())))
            .collect::<BTreeMap<_, _>>()
    )
}
fn advance(positions: &[Position], indices: &[usize], tuple: &mut [i128]) -> bool {
    for index in indices.iter().rev().copied() {
        let next = match &positions[index].domain {
            LeafDomain::Bool => (tuple[index] == 0).then_some(1),
            LeafDomain::Int { max, .. } => (tuple[index] < *max).then(|| tuple[index] + 1),
            LeafDomain::Sum { variants, .. } => variants
                .iter()
                .position(|v| i128::from(*v) == tuple[index])
                .and_then(|i| variants.get(i + 1))
                .map(|v| i128::from(*v)),
        };
        if let Some(next) = next {
            tuple[index] = next;
            return true;
        }
        tuple[index] = positions[index].domain.least().unwrap_or(0);
    }
    false
}
