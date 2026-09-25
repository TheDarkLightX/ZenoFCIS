//! Exhaustive system properties of a finite transition program.
//!
//! A property is a closed Boolean relation over a transition's inputs followed
//! by its outputs, with the same shape as a synthesis
//! [`Contract`](crate::finite::Contract). Checking it
//! against the transition, rather than on its own, is what makes a result say
//! something about the system. A property that holds for every output tuple the
//! declared domains admit says nothing about the transition, so it is reported
//! as domain-implied rather than as a system property.
//!
//! The transition's own output-domain check is part of its executed semantics:
//! an out-of-domain output is an execution failure. It is therefore reported as
//! a totality failure, never silently excluded from the relation.

use alloc::vec::Vec;

use crate::finite::{Contract, Domain, Error, Program};

/// A closed Boolean relation over a transition's inputs followed by its
/// outputs, kept together with the equivalent relational [`Contract`].
///
/// [`Contract`] does not expose its relation program, so this type retains the
/// program for encoders such as SMT exporters while [`check_system_property`]
/// evaluates the same relation through [`Property::contract`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Property {
    contract: Contract,
    relation: Program,
}

impl Property {
    /// Validates `relation` as a Boolean relation over `inputs` then `outputs`.
    ///
    /// Outputs must be nonempty and fit the [`Program`] output bound (16).
    /// Inputs and outputs together must fit its input bound (32). The public
    /// synthesis [`Contract::try_new`] retains its separate 16-input bound.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] with `contract-shape` for an excessive field
    /// count, empty outputs, mismatched relation domains, or a non-Boolean result.
    pub fn try_new(
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        relation: Program,
    ) -> Result<Self, Error> {
        let contract = Contract::try_new_system_property(inputs, outputs, relation.clone())?;
        Ok(Self { contract, relation })
    }

    /// Returns the equivalent relation for checking and canonical commitment.
    /// Its input count may exceed the synthesis constructor's 16-field limit.
    #[must_use]
    pub const fn contract(&self) -> &Contract {
        &self.contract
    }

    /// Returns the relation program: inputs, then outputs, to one Boolean.
    #[must_use]
    pub const fn relation(&self) -> &Program {
        &self.relation
    }
}

/// Deterministic enumeration limits for one check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemLimits {
    /// Maximum admitted input tuples.
    pub max_inputs: u64,
    /// Maximum input and output tuple pairs examined by the domain-only control.
    pub max_pairs: u64,
}

impl Default for SystemLimits {
    fn default() -> Self {
        Self {
            max_inputs: 65_536,
            max_pairs: 4_194_304,
        }
    }
}

/// Outcome of checking one property of a finite transition on every admitted
/// input.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemCheck {
    /// Every admitted input yields a defined output inside the declared output
    /// domains that satisfies the property, and some output tuple the domains
    /// admit would not. The result depends on the transition.
    SystemProperty {
        /// Admitted input tuples checked.
        inputs: u64,
    },
    /// Every admitted input satisfies the property, and so does every output
    /// tuple the declared domains admit. The result says nothing about the
    /// transition.
    DomainImplied {
        /// Admitted input tuples checked.
        inputs: u64,
        /// Input and output tuple pairs examined by the domain-only control.
        pairs: u64,
    },
    /// This admitted input traps or produces an output outside the declared
    /// domains, so the transition fails before any property applies.
    NotTotal {
        /// First admitted input, in canonical order, without an in-domain output.
        input: Vec<i64>,
    },
    /// The property is false for this admitted input and the transition's
    /// output.
    Violated {
        /// First violating admitted input in canonical order.
        input: Vec<i64>,
        /// The transition's exact output for that input.
        output: Vec<i64>,
    },
    /// The property traps on this admitted input and the transition's output.
    Undefined {
        /// First admitted input, in canonical order, on which the property traps.
        input: Vec<i64>,
        /// The transition's exact output for that input.
        output: Vec<i64>,
    },
}

impl SystemCheck {
    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SystemProperty { .. } => "system-property",
            Self::DomainImplied { .. } => "domain-implied",
            Self::NotTotal { .. } => "not-total",
            Self::Violated { .. } => "violated",
            Self::Undefined { .. } => "undefined",
        }
    }

    /// Returns true only for a property that holds and depends on the
    /// transition.
    #[must_use]
    pub const fn is_system_property(&self) -> bool {
        matches!(self, Self::SystemProperty { .. })
    }
}

/// Checks `property` against `transition` on every admitted input.
///
/// The check has three stages. Each stage covers every admitted input before
/// the next begins, and results are reported in this order:
///
/// 1. Totality: every admitted input must yield a defined output inside the
///    declared output domains.
/// 2. Property: the property must hold, and be defined, for each input and the
///    transition's output.
/// 3. Domain-only control: the property is evaluated for every output tuple
///    the declared domains admit, without the transition. If it still holds
///    everywhere, the result is [`SystemCheck::DomainImplied`].
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the property's input or output domains
/// differ from the transition's, and [`Error::Limit`] when enumeration would
/// exceed `limits`.
pub fn check_system_property(
    transition: &Program,
    property: &Contract,
    limits: SystemLimits,
) -> Result<SystemCheck, Error> {
    if property.inputs() != transition.inputs() || property.outputs() != transition.outputs() {
        return Err(Error::Invalid("property-shape"));
    }
    let inputs = property.input_space()?.cardinality();
    if inputs > limits.max_inputs {
        return Err(Error::Limit("system-inputs"));
    }
    // Totality is decided on every admitted input before any property result,
    // so a property failure never hides a later totality failure.
    for input in property.input_space()? {
        if transition.evaluate(&input).is_err() {
            return Ok(SystemCheck::NotTotal { input });
        }
    }
    for input in property.input_space()? {
        let Ok(output) = transition.evaluate(&input) else {
            return Ok(SystemCheck::NotTotal { input });
        };
        match property.holds(&input, &output) {
            Ok(true) => {}
            Ok(false) => return Ok(SystemCheck::Violated { input, output }),
            Err(_) => return Ok(SystemCheck::Undefined { input, output }),
        }
    }

    let pairs = cardinality(transition.outputs())?
        .checked_mul(inputs)
        .filter(|pairs| *pairs <= limits.max_pairs)
        .ok_or(Error::Limit("system-pairs"))?;
    for input in property.input_space()? {
        let mut output: Vec<i64> = transition.outputs().iter().map(|d| d.bounds().0).collect();
        loop {
            // A violating or undefined tuple that the domains admit shows that
            // the property depends on what the transition produces.
            if !matches!(property.holds(&input, &output), Ok(true)) {
                return Ok(SystemCheck::SystemProperty { inputs });
            }
            if !advance(transition.outputs(), &mut output) {
                break;
            }
        }
    }
    Ok(SystemCheck::DomainImplied { inputs, pairs })
}

fn cardinality(domains: &[Domain]) -> Result<u64, Error> {
    domains.iter().try_fold(1u64, |count, domain| {
        let (min, max) = domain.bounds();
        let width = u64::try_from(i128::from(max) - i128::from(min) + 1)
            .map_err(|_| Error::Limit("system-space"))?;
        count.checked_mul(width).ok_or(Error::Limit("system-space"))
    })
}

/// Steps a tuple to its lexicographic successor; false once it wraps.
fn advance(domains: &[Domain], tuple: &mut [i64]) -> bool {
    for (value, domain) in tuple.iter_mut().zip(domains).rev() {
        let (min, max) = domain.bounds();
        if *value < max {
            *value += 1;
            return true;
        }
        *value = min;
    }
    false
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::finite::Op;

    const COUNT: Domain = Domain::Int { min: 0, max: 3 };

    /// Increments a counter while it is below `cap`, else keeps it.
    fn counter(cap: i64) -> Program {
        Program::try_new(
            vec![COUNT],
            vec![COUNT],
            vec![
                Op::Input(0),
                Op::Int(cap),
                Op::Lt(0, 1),
                Op::Int(1),
                Op::Add(0, 3),
                Op::Select(2, 4, 0),
            ],
            vec![5],
        )
        .unwrap_or_else(|error| panic!("counter program: {error:?}"))
    }

    /// A relation over (pre, post) built from `nodes` whose last node is the verdict.
    fn property(nodes: Vec<Op>) -> Contract {
        let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| unreachable!());
        let relation = Program::try_new(vec![COUNT, COUNT], vec![Domain::Bool], nodes, vec![root])
            .unwrap_or_else(|error| panic!("property program: {error:?}"));
        Contract::try_new(vec![COUNT], vec![COUNT], relation)
            .unwrap_or_else(|error| panic!("property contract: {error:?}"))
    }

    /// post >= pre
    fn monotone() -> Contract {
        property(vec![Op::Input(1), Op::Input(0), Op::Lt(0, 1), Op::Not(2)])
    }

    /// post <= 3, which the output domain already guarantees.
    fn bounded() -> Contract {
        property(vec![Op::Int(3), Op::Input(1), Op::Lt(0, 1), Op::Not(2)])
    }

    /// post == pre
    fn unchanged() -> Contract {
        property(vec![Op::Input(1), Op::Input(0), Op::Eq(0, 1)])
    }

    #[test]
    fn transition_dependent_property_is_a_system_property() {
        assert_eq!(
            check_system_property(&counter(3), &monotone(), SystemLimits::default()),
            Ok(SystemCheck::SystemProperty { inputs: 4 })
        );
    }

    #[test]
    fn domain_only_control_reports_properties_the_domains_already_imply() {
        assert_eq!(
            check_system_property(&counter(3), &bounded(), SystemLimits::default()),
            Ok(SystemCheck::DomainImplied {
                inputs: 4,
                pairs: 16
            })
        );
    }

    #[test]
    fn out_of_domain_output_is_a_totality_failure_not_an_exclusion() {
        // Planted bug: the guard admits pre = 3, so post = 4 leaves the output
        // domain. Folding the domain into the relation would hide this.
        assert_eq!(
            check_system_property(&counter(4), &monotone(), SystemLimits::default()),
            Ok(SystemCheck::NotTotal { input: vec![3] })
        );
    }

    #[test]
    fn totality_is_decided_on_every_input_before_any_property_result() {
        // Doubling over {0, 1}: input 0 violates `post == 1`, and input 1
        // leaves the output domain. The totality failure is reported even
        // though the property fails at an earlier input.
        let bit = Domain::Int { min: 0, max: 1 };
        let double = Program::try_new(
            vec![bit],
            vec![bit],
            vec![Op::Input(0), Op::Add(0, 0)],
            vec![1],
        )
        .unwrap_or_else(|error| panic!("double program: {error:?}"));
        let is_one = Contract::try_new(
            vec![bit],
            vec![bit],
            Program::try_new(
                vec![bit, bit],
                vec![Domain::Bool],
                vec![Op::Input(1), Op::Int(1), Op::Eq(0, 1)],
                vec![2],
            )
            .unwrap_or_else(|error| panic!("relation: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("contract: {error:?}"));
        assert_eq!(
            check_system_property(&double, &is_one, SystemLimits::default()),
            Ok(SystemCheck::NotTotal { input: vec![1] })
        );
    }

    #[test]
    fn violations_report_the_first_input_and_exact_output() {
        assert_eq!(
            check_system_property(&counter(3), &unchanged(), SystemLimits::default()),
            Ok(SystemCheck::Violated {
                input: vec![0],
                output: vec![1]
            })
        );
    }

    #[test]
    fn property_shape_and_limits_fail_closed() {
        let other = Contract::try_new(
            vec![Domain::Bool],
            vec![COUNT],
            Program::try_new(
                vec![Domain::Bool, COUNT],
                vec![Domain::Bool],
                vec![Op::Bool(true)],
                vec![0],
            )
            .unwrap_or_else(|error| panic!("other relation: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("other contract: {error:?}"));
        assert_eq!(
            check_system_property(&counter(3), &other, SystemLimits::default()),
            Err(Error::Invalid("property-shape"))
        );
        let tight = SystemLimits {
            max_inputs: 3,
            max_pairs: 16,
        };
        assert_eq!(
            check_system_property(&counter(3), &monotone(), tight),
            Err(Error::Limit("system-inputs"))
        );
        let no_pairs = SystemLimits {
            max_inputs: 4,
            max_pairs: 15,
        };
        assert_eq!(
            check_system_property(&counter(3), &bounded(), no_pairs),
            Err(Error::Limit("system-pairs"))
        );
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(
            SystemCheck::SystemProperty { inputs: 1 }.code(),
            "system-property"
        );
        assert_eq!(
            SystemCheck::DomainImplied {
                inputs: 1,
                pairs: 1
            }
            .code(),
            "domain-implied"
        );
        assert_eq!(SystemCheck::NotTotal { input: vec![] }.code(), "not-total");
        assert!(SystemCheck::SystemProperty { inputs: 1 }.is_system_property());
        assert!(!SystemCheck::NotTotal { input: vec![] }.is_system_property());
    }
}
