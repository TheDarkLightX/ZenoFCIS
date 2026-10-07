//! The rule base of `rules.txt`: parsed, checked for consistency and totality
//! over its finite features, and evaluated by priority.
//!
//! This evaluator is independent of the synthesized step in
//! `synthesized/transition.rs`. The tests compare the two on every input, and
//! the law checker evaluates the rule base against every decision at run time.

/// The rule base as shipped. `profile.rs` includes the same file in the
/// source identity, so changing a rule changes the policy identity.
pub const RULE_BASE: &str = include_str!("../rules.txt");

/// What a rule concludes about a transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Allow,
    Hold,
    Block,
}

impl Verdict {
    /// The code `synthesis.json` gives each verdict.
    #[must_use]
    pub fn code(self) -> i64 {
        match self {
            Self::Allow => 0,
            Self::Hold => 1,
            Self::Block => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// One finite feature of the working memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Feature {
    name: String,
    low: i64,
    high: i64,
    /// The values of an enumerated feature, in order; empty for an integer.
    values: Vec<String>,
}

impl Feature {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn low(&self) -> i64 {
        self.low
    }
    #[must_use]
    pub fn high(&self) -> i64 {
        self.high
    }
    #[must_use]
    pub fn values(&self) -> &[String] {
        &self.values
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Condition {
    feature: usize,
    op: Op,
    value: i64,
}

/// One rule: its conditions all hold, or it does not apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    name: String,
    priority: i64,
    conditions: Vec<Condition>,
    verdict: Verdict,
}

impl Rule {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn priority(&self) -> i64 {
        self.priority
    }
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }
}

/// The decision for one transfer: its verdict and the rule that decided.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fired {
    pub verdict: Verdict,
    /// The position of the rule in `rules.txt`.
    pub rule: usize,
}

/// Why a rule base is refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleBaseError {
    /// A line the format does not allow.
    Syntax { line: usize, problem: &'static str },
    /// Two rules of the same priority match the same input.
    Conflict {
        input: Vec<i64>,
        first: String,
        second: String,
    },
    /// An input that no rule matches.
    Uncovered { input: Vec<i64> },
    /// A rule that decides no input: its conditions never hold, or a rule
    /// of higher priority always holds with them.
    NeverFires { rule: String },
}

/// The most inputs a rule base may have; `check` visits every one.
pub const MAX_INPUTS: u64 = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleBase {
    features: Vec<Feature>,
    rules: Vec<Rule>,
}

impl RuleBase {
    /// Parses a rule base. `check` then decides whether it is usable.
    ///
    /// # Errors
    ///
    /// `RuleBaseError::Syntax` names the first line the format does not
    /// allow and what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, RuleBaseError> {
        let mut base = Self {
            features: Vec::new(),
            rules: Vec::new(),
        };
        let mut inputs: u64 = 1;
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let syntax = |problem| RuleBaseError::Syntax {
                line: line_number,
                problem,
            };
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.first().copied() {
                None => {}
                Some(word) if word.starts_with('#') => {}
                Some("feature") => {
                    if !base.rules.is_empty() {
                        return Err(syntax("features come before rules"));
                    }
                    let feature = parse_feature(&words[1..], &syntax)?;
                    if base.features.iter().any(|f| f.name == feature.name) {
                        return Err(syntax("a feature is declared twice"));
                    }
                    inputs = feature
                        .high
                        .checked_sub(feature.low)
                        .and_then(|width| width.checked_add(1))
                        .and_then(|size| u64::try_from(size).ok())
                        .and_then(|size| inputs.checked_mul(size))
                        .filter(|total| *total <= MAX_INPUTS)
                        .ok_or_else(|| syntax("the features have more than 65536 inputs"))?;
                    base.features.push(feature);
                }
                Some("rule") => {
                    let rule = parse_rule(&words[1..], &base.features, &syntax)?;
                    if base.rules.iter().any(|r| r.name == rule.name) {
                        return Err(syntax("a rule is declared twice"));
                    }
                    base.rules.push(rule);
                }
                Some(_) => return Err(syntax("a line must declare a feature or a rule")),
            }
        }
        Ok(base)
    }

    /// Refuses a rule base in which two rules of the same priority match one
    /// input, whatever they conclude; in which some input matches no rule;
    /// or in which some rule decides no input.
    ///
    /// # Errors
    ///
    /// `RuleBaseError::Conflict` names the input and the two rules,
    /// `RuleBaseError::Uncovered` names the input, and
    /// `RuleBaseError::NeverFires` names the rule.
    pub fn check(&self) -> Result<(), RuleBaseError> {
        let mut fires = vec![false; self.rules.len()];
        for input in self.inputs() {
            let matching: Vec<(usize, &Rule)> = self
                .rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| self.matches(rule, &input))
                .collect();
            let Some((decider, _)) = matching
                .iter()
                .max_by_key(|(_, rule)| rule.priority)
                .copied()
            else {
                return Err(RuleBaseError::Uncovered { input });
            };
            for (position, (_, first)) in matching.iter().enumerate() {
                if let Some((_, second)) = matching[position + 1..]
                    .iter()
                    .find(|(_, second)| second.priority == first.priority)
                {
                    return Err(RuleBaseError::Conflict {
                        input,
                        first: first.name.clone(),
                        second: second.name.clone(),
                    });
                }
            }
            fires[decider] = true;
        }
        match fires.iter().position(|fired| !fired) {
            Some(rule) => Err(RuleBaseError::NeverFires {
                rule: self.rules[rule].name.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Parses and checks.
    ///
    /// # Errors
    ///
    /// The errors of `parse`, then those of `check`.
    pub fn load(text: &str) -> Result<Self, RuleBaseError> {
        let base = Self::parse(text)?;
        base.check()?;
        Ok(base)
    }

    #[must_use]
    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Every input of the finite domain, in lexicographic order.
    #[must_use]
    pub fn inputs(&self) -> Vec<Vec<i64>> {
        let mut inputs = vec![Vec::new()];
        for feature in &self.features {
            inputs = inputs
                .iter()
                .flat_map(|prefix| {
                    (feature.low..=feature.high).map(move |value| {
                        let mut input = prefix.clone();
                        input.push(value);
                        input
                    })
                })
                .collect();
        }
        inputs
    }

    /// The decision for one input: the highest-priority rule that matches, or
    /// `None` when no rule does. After `check`, the highest priority is unique.
    #[must_use]
    pub fn decide(&self, input: &[i64]) -> Option<Fired> {
        let mut fired: Option<(usize, &Rule)> = None;
        for (index, rule) in self.rules.iter().enumerate() {
            if self.matches(rule, input)
                && fired.is_none_or(|(_, best)| rule.priority > best.priority)
            {
                fired = Some((index, rule));
            }
        }
        fired.map(|(rule, best)| Fired {
            verdict: best.verdict,
            rule,
        })
    }

    /// The position of a blocking rule among the blocking rules of the base.
    /// The committed-failure reasons of `project.zeno` follow this order.
    #[must_use]
    pub fn block_ordinal(&self, rule: usize) -> Option<usize> {
        let target = self.rules.get(rule)?;
        (target.verdict == Verdict::Block).then(|| {
            self.rules[..rule]
                .iter()
                .filter(|earlier| earlier.verdict == Verdict::Block)
                .count()
        })
    }

    fn matches(&self, rule: &Rule, input: &[i64]) -> bool {
        input.len() == self.features.len()
            && rule.conditions.iter().all(|condition| {
                let actual = input[condition.feature];
                match condition.op {
                    Op::Eq => actual == condition.value,
                    Op::Ne => actual != condition.value,
                    Op::Lt => actual < condition.value,
                    Op::Le => actual <= condition.value,
                    Op::Gt => actual > condition.value,
                    Op::Ge => actual >= condition.value,
                }
            })
    }
}

fn parse_feature(
    words: &[&str],
    syntax: &dyn Fn(&'static str) -> RuleBaseError,
) -> Result<Feature, RuleBaseError> {
    let [name, spec @ ..] = words else {
        return Err(syntax("a feature needs a name"));
    };
    if let [range] = spec
        && let Some((low, high)) = range.split_once("..")
    {
        let (low, high) = (
            low.parse::<i64>()
                .map_err(|_| syntax("a range bound must be an integer"))?,
            high.parse::<i64>()
                .map_err(|_| syntax("a range bound must be an integer"))?,
        );
        if low > high {
            return Err(syntax("a range must be MIN..MAX with MIN at most MAX"));
        }
        return Ok(Feature {
            name: (*name).to_owned(),
            low,
            high,
            values: Vec::new(),
        });
    }
    if spec.len() < 2 {
        return Err(syntax("an enumerated feature needs at least two values"));
    }
    for (position, value) in spec.iter().enumerate() {
        if spec[..position].contains(value) {
            return Err(syntax("an enumerated value is listed twice"));
        }
    }
    Ok(Feature {
        name: (*name).to_owned(),
        low: 0,
        high: i64::try_from(spec.len()).map_err(|_| syntax("too many values"))? - 1,
        values: spec.iter().map(|value| (*value).to_owned()).collect(),
    })
}

fn parse_rule(
    words: &[&str],
    features: &[Feature],
    syntax: &dyn Fn(&'static str) -> RuleBaseError,
) -> Result<Rule, RuleBaseError> {
    let [
        name,
        "priority",
        priority,
        "when",
        clause @ ..,
        "then",
        verdict,
    ] = words
    else {
        return Err(syntax(
            "a rule is `rule NAME priority N when CONDITIONS then VERDICT`",
        ));
    };
    let priority = priority
        .parse::<i64>()
        .map_err(|_| syntax("a priority must be an integer"))?;
    let verdict = match *verdict {
        "allow" => Verdict::Allow,
        "hold" => Verdict::Hold,
        "block" => Verdict::Block,
        _ => return Err(syntax("a verdict is allow, hold, or block")),
    };
    let mut conditions = Vec::new();
    if clause != ["always"] {
        let mut rest = clause;
        loop {
            let [feature, op, value, after @ ..] = rest else {
                return Err(syntax("a condition is `FEATURE OP VALUE`"));
            };
            conditions.push(parse_condition(feature, op, value, features, syntax)?);
            match after {
                [] => break,
                ["and", more @ ..] if !more.is_empty() => rest = more,
                _ => return Err(syntax("conditions are joined by `and`")),
            }
        }
    }
    Ok(Rule {
        name: (*name).to_owned(),
        priority,
        conditions,
        verdict,
    })
}

fn parse_condition(
    feature: &str,
    op: &str,
    value: &str,
    features: &[Feature],
    syntax: &dyn Fn(&'static str) -> RuleBaseError,
) -> Result<Condition, RuleBaseError> {
    let position = features
        .iter()
        .position(|f| f.name == feature)
        .ok_or_else(|| syntax("a condition names an undeclared feature"))?;
    let declared = &features[position];
    let op = match op {
        "==" => Op::Eq,
        "!=" => Op::Ne,
        "<" => Op::Lt,
        "<=" => Op::Le,
        ">" => Op::Gt,
        ">=" => Op::Ge,
        _ => return Err(syntax("an operator is ==, !=, <, <=, >, or >=")),
    };
    let value = if declared.values.is_empty() {
        let value = value
            .parse::<i64>()
            .map_err(|_| syntax("an integer feature is compared with an integer"))?;
        if value < declared.low || value > declared.high {
            return Err(syntax(
                "a condition names a value outside the feature's range",
            ));
        }
        value
    } else {
        if !matches!(op, Op::Eq | Op::Ne) {
            return Err(syntax("an enumerated feature accepts == and != only"));
        }
        let index = declared
            .values
            .iter()
            .position(|declared| declared == value)
            .ok_or_else(|| syntax("a condition names an undeclared value"))?;
        i64::try_from(index).map_err(|_| syntax("too many values"))?
    };
    Ok(Condition {
        feature: position,
        op,
        value,
    })
}
