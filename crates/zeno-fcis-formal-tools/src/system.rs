//! SMT obligations that include a finite transition relation.
//!
//! [`crate::export_smt`] exports a claim over unconstrained observations, so
//! its results say nothing specific about a system. The obligations here
//! include the transition itself: an exact encoding of a `finite-i64/1`
//! program, its input and output domains, and its checked arithmetic. A
//! property is a closed Boolean relation over the transition's inputs followed
//! by its outputs: a synthesis [`Property`].
//!
//! Each property yields three scripts. Each script is unsatisfiable exactly
//! when the answer to its question is yes:
//!
//! - totality: does every admitted input yield a defined output inside the
//!   declared output domains? Output domains are checked here and never
//!   assumed, because the executed program fails on an out-of-domain output.
//! - property: does the property hold, and is it defined, on the transition's
//!   output for every admitted input? This script assumes totality, so its
//!   answer counts only after the totality script is unsatisfiable.
//! - domain only: does the property hold for every output tuple the declared
//!   domains admit, without the transition? If so, the property says nothing
//!   about the transition.
//!
//! Every script requests the values of its free variables: the inputs, and for
//! the domain-only control also the proposed outputs. A model is reported only
//! after its values are checked against the declared domains and the program
//! interpreter reproduces the failure it claims.

use core::fmt::Write as _;

use zeno_fcis_synthesis::finite::{Domain, Op, Program};
use zeno_fcis_synthesis::system::Property;

use crate::ExportError;

/// Which question one system obligation asks the solver.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemObligationKind {
    /// Every admitted input yields a defined output inside the output domains.
    Totality,
    /// The property holds on the transition's output for every admitted input,
    /// assuming totality.
    Property,
    /// The property holds for every admitted output tuple, without the
    /// transition.
    DomainOnly,
}

impl SystemObligationKind {
    /// All kinds, in the order their answers are combined.
    pub const ALL: [Self; 3] = [Self::Totality, Self::Property, Self::DomainOnly];

    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Totality => "totality",
            Self::Property => "property",
            Self::DomainOnly => "domain-only",
        }
    }
}

/// SMT-LIB scripts for one property of one finite transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemObligations {
    inputs: usize,
    outputs: usize,
    totality: Vec<u8>,
    property: Vec<u8>,
    domain_only: Vec<u8>,
}

impl SystemObligations {
    /// Returns the exact script for one obligation.
    #[must_use]
    pub fn source(&self, kind: SystemObligationKind) -> &[u8] {
        match kind {
            SystemObligationKind::Totality => &self.totality,
            SystemObligationKind::Property => &self.property,
            SystemObligationKind::DomainOnly => &self.domain_only,
        }
    }

    /// Returns the number of transition inputs whose values each script
    /// requests, named `in_0` onward.
    #[must_use]
    pub const fn inputs(&self) -> usize {
        self.inputs
    }

    /// Returns the number of proposed outputs whose values the domain-only
    /// script also requests, named `free_out_0` onward.
    #[must_use]
    pub const fn outputs(&self) -> usize {
        self.outputs
    }
}

/// Exports the three obligations for `property` against `transition`.
///
/// # Errors
///
/// Returns [`ExportError::InvalidFormula`] when the property's input or output
/// domains differ from the transition's.
pub fn export_system_smt(
    transition: &Program,
    property: &Property,
) -> Result<SystemObligations, ExportError> {
    let contract = property.contract();
    if contract.inputs() != transition.inputs() || contract.outputs() != transition.outputs() {
        return Err(ExportError::InvalidFormula);
    }
    let inputs = transition.inputs().len();
    let outputs = transition.outputs().len();
    let input_names: Vec<String> = (0..inputs).map(|index| format!("in_{index}")).collect();
    let mut model_names = input_names.clone();
    model_names.extend((0..outputs).map(|index| format!("free_out_{index}")));

    let transition_terms = encode("t", transition, |id| format!("in_{id}"));
    let transition_outputs: Vec<String> = transition
        .roots()
        .iter()
        .map(|root| format!("t_{root}"))
        .collect();
    let admitted_outputs = conjunction(
        transition
            .outputs()
            .iter()
            .zip(&transition_outputs)
            .map(|(domain, term)| in_domain(*domain, term)),
    );

    let with_transition = encode("p", property.relation(), |id| {
        if usize::from(id) < inputs {
            format!("in_{id}")
        } else {
            transition_outputs[usize::from(id) - inputs].clone()
        }
    });
    let without_transition = encode("p", property.relation(), |id| {
        if usize::from(id) < inputs {
            format!("in_{id}")
        } else {
            format!("free_out_{}", usize::from(id) - inputs)
        }
    });

    let mut totality = header(transition.inputs());
    totality.push_str(&transition_terms.definitions);
    let _ = writeln!(
        totality,
        "(assert (not (and {} {admitted_outputs})))",
        transition_terms.defined
    );
    finish(&mut totality, &input_names);

    let mut holds = header(transition.inputs());
    holds.push_str(&transition_terms.definitions);
    let _ = writeln!(holds, "(assert {})", transition_terms.defined);
    let _ = writeln!(holds, "(assert {admitted_outputs})");
    holds.push_str(&with_transition.definitions);
    let _ = writeln!(
        holds,
        "(assert (not (and {} (= {} 1))))",
        with_transition.defined, with_transition.root
    );
    finish(&mut holds, &input_names);

    let mut domain_only = header(transition.inputs());
    for (index, domain) in transition.outputs().iter().enumerate() {
        let name = format!("free_out_{index}");
        let _ = writeln!(domain_only, "(declare-const {name} Int)");
        let _ = writeln!(domain_only, "(assert {})", in_domain(*domain, &name));
    }
    domain_only.push_str(&without_transition.definitions);
    let _ = writeln!(
        domain_only,
        "(assert (not (and {} (= {} 1))))",
        without_transition.defined, without_transition.root
    );
    finish(&mut domain_only, &model_names);

    Ok(SystemObligations {
        inputs,
        outputs,
        totality: totality.into_bytes(),
        property: holds.into_bytes(),
        domain_only: domain_only.into_bytes(),
    })
}

/// A solver's answer to one system obligation.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemAnswer {
    /// Unsatisfiable: the obligation's question has the answer yes.
    Unsat,
    /// Satisfiable, with the model values the script requests.
    Sat {
        /// Transition input values, in order.
        input: Vec<i64>,
        /// Proposed output values, in order. Only the domain-only control
        /// requests them; for the other obligations this is empty.
        output: Vec<i64>,
    },
}

/// Outcome of the three system obligations, in the exhaustive checker's order.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemVerdict {
    /// The transition is total, the property holds on it, and the domains
    /// alone do not imply the property.
    SystemProperty,
    /// The property holds for every output tuple the domains admit, so it
    /// says nothing about the transition.
    DomainImplied,
    /// A replayed admitted input traps or leaves the output domains.
    NotTotal {
        /// The replayed input.
        input: Vec<i64>,
    },
    /// A replayed admitted input violates the property.
    Violated {
        /// The replayed input.
        input: Vec<i64>,
        /// The interpreter's output for that input.
        output: Vec<i64>,
    },
    /// The property traps on a replayed admitted input.
    Undefined {
        /// The replayed input.
        input: Vec<i64>,
        /// The interpreter's output for that input.
        output: Vec<i64>,
    },
}

impl SystemVerdict {
    /// Returns the stable machine-readable name. The names match those of the
    /// exhaustive checker's `SystemCheck`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SystemProperty => "system-property",
            Self::DomainImplied => "domain-implied",
            Self::NotTotal { .. } => "not-total",
            Self::Violated { .. } => "violated",
            Self::Undefined { .. } => "undefined",
        }
    }
}

/// Failure to reach a system verdict. None of these grants any evidence.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemSolveError {
    /// The obligations could not be exported.
    Export(ExportError),
    /// The solver gave no usable answer: unknown, timeout, or malformed output.
    Solver(SystemObligationKind),
    /// A model had the wrong shape, lay outside the declared domains, or did
    /// not reproduce its failure in the program interpreter.
    UnreplayableModel {
        /// The obligation that produced the model.
        kind: SystemObligationKind,
        /// The model's input values.
        input: Vec<i64>,
        /// The model's proposed output values, if any.
        output: Vec<i64>,
    },
}

/// Reads a solver's answer to one exported script: `unsat`, or `sat` followed
/// by the `(get-value ...)` model the script requests. `inputs` and `outputs`
/// are the transition's input and output counts; proposed outputs are read
/// only for the domain-only control.
///
/// # Errors
///
/// Returns [`SystemSolveError::Solver`] for `unknown`, missing, or malformed
/// output.
pub fn parse_system_answer(
    kind: SystemObligationKind,
    stdout: &[u8],
    inputs: usize,
    outputs: usize,
) -> Result<SystemAnswer, SystemSolveError> {
    let text = core::str::from_utf8(stdout).map_err(|_| SystemSolveError::Solver(kind))?;
    let values = |prefix: &str, count: usize| {
        (0..count)
            .map(|index| model_value(text, &format!("{prefix}{index}")))
            .collect::<Option<Vec<i64>>>()
    };
    let proposed = if kind == SystemObligationKind::DomainOnly {
        outputs
    } else {
        0
    };
    match text.lines().next().map(str::trim) {
        Some("unsat") => Ok(SystemAnswer::Unsat),
        Some("sat") => match (values("in_", inputs), values("free_out_", proposed)) {
            (Some(input), Some(output)) => Ok(SystemAnswer::Sat { input, output }),
            _ => Err(SystemSolveError::Solver(kind)),
        },
        _ => Err(SystemSolveError::Solver(kind)),
    }
}

/// Reads `(name v)`, accepting a negative value printed as `(- n)`.
fn model_value(text: &str, name: &str) -> Option<i64> {
    let key = format!("({name} ");
    let rest = &text[text.find(&key)? + key.len()..];
    let (negative, rest) = rest
        .strip_prefix("(- ")
        .map_or((false, rest), |rest| (true, rest));
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let magnitude: i128 = digits.parse().ok()?;
    i64::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// Exports the obligations for `property`, asks `solve` for each in order, and
/// combines the answers as the exhaustive checker does.
///
/// A model is accepted only when it has one value per variable, every value
/// lies inside its declared domain, and the program interpreter reproduces the
/// failure the model claims. That includes the domain-only control, whose
/// proposed input and output must make the property false or undefined.
///
/// # Errors
///
/// Returns [`SystemSolveError`] when export fails, the solver gives no usable
/// answer, or a model cannot be replayed.
pub fn system_verdict(
    transition: &Program,
    property: &Property,
    mut solve: impl FnMut(SystemObligationKind, &[u8]) -> Result<SystemAnswer, SystemSolveError>,
) -> Result<SystemVerdict, SystemSolveError> {
    let obligations = export_system_smt(transition, property).map_err(SystemSolveError::Export)?;
    let mut ask = |kind| solve(kind, obligations.source(kind));
    let refuse = |kind, input, output| SystemSolveError::UnreplayableModel {
        kind,
        input,
        output,
    };
    if let SystemAnswer::Sat { input, output } = ask(SystemObligationKind::Totality)? {
        // An inadmissible input would fail the interpreter's own domain check,
        // which is not a totality failure.
        if !output.is_empty()
            || !admitted(transition.inputs(), &input)
            || transition.evaluate(&input).is_ok()
        {
            return Err(refuse(SystemObligationKind::Totality, input, output));
        }
        return Ok(SystemVerdict::NotTotal { input });
    }
    if let SystemAnswer::Sat { input, output } = ask(SystemObligationKind::Property)? {
        let kind = SystemObligationKind::Property;
        if !output.is_empty() || !admitted(transition.inputs(), &input) {
            return Err(refuse(kind, input, output));
        }
        let Ok(result) = transition.evaluate(&input) else {
            return Err(refuse(kind, input, output));
        };
        return match property.contract().holds(&input, &result) {
            Ok(false) => Ok(SystemVerdict::Violated {
                input,
                output: result,
            }),
            Err(_) => Ok(SystemVerdict::Undefined {
                input,
                output: result,
            }),
            Ok(true) => Err(refuse(kind, input, output)),
        };
    }
    match ask(SystemObligationKind::DomainOnly)? {
        SystemAnswer::Unsat => Ok(SystemVerdict::DomainImplied),
        SystemAnswer::Sat { input, output } => {
            let violates = admitted(transition.inputs(), &input)
                && admitted(transition.outputs(), &output)
                && !matches!(property.contract().holds(&input, &output), Ok(true));
            if violates {
                Ok(SystemVerdict::SystemProperty)
            } else {
                Err(refuse(SystemObligationKind::DomainOnly, input, output))
            }
        }
    }
}

/// True when `values` has exactly one value per domain, each inside it.
fn admitted(domains: &[Domain], values: &[i64]) -> bool {
    values.len() == domains.len()
        && domains.iter().zip(values).all(|(domain, value)| {
            let (min, max) = domain.bounds();
            (min..=max).contains(value)
        })
}

/// SMT definitions for one program's nodes.
struct Encoded {
    definitions: String,
    /// True exactly when every checked addition and subtraction stays in i64.
    defined: String,
    /// Term naming the program's single result, when it has exactly one.
    root: String,
}

fn encode(prefix: &str, program: &Program, input: impl Fn(u16) -> String) -> Encoded {
    let mut definitions = String::new();
    let mut checked = Vec::new();
    let node = |id: u16| format!("{prefix}_{id}");
    for (index, op) in program.nodes().iter().enumerate() {
        let term = match *op {
            Op::Input(id) => input(id),
            Op::Int(value) => int(value),
            Op::Bool(value) => String::from(if value { "1" } else { "0" }),
            Op::Add(a, b) => format!("(+ {} {})", node(a), node(b)),
            Op::Sub(a, b) => format!("(- {} {})", node(a), node(b)),
            Op::Eq(a, b) => format!("(ite (= {} {}) 1 0)", node(a), node(b)),
            Op::Lt(a, b) => format!("(ite (< {} {}) 1 0)", node(a), node(b)),
            Op::And(a, b) => format!("(ite (and (= {} 1) (= {} 1)) 1 0)", node(a), node(b)),
            Op::Not(a) => format!("(ite (= {} 0) 1 0)", node(a)),
            Op::Select(c, a, b) => format!("(ite (= {} 1) {} {})", node(c), node(a), node(b)),
        };
        let name = format!("{prefix}_{index}");
        let _ = writeln!(definitions, "(define-fun {name} () Int {term})");
        if matches!(op, Op::Add(..) | Op::Sub(..)) {
            checked.push(format!("(<= {} {name} {})", int(i64::MIN), int(i64::MAX)));
        }
    }
    let root = program
        .roots()
        .first()
        .map_or_else(String::new, |root| node(*root));
    Encoded {
        definitions,
        defined: conjunction(checked.into_iter()),
        root,
    }
}

fn header(inputs: &[Domain]) -> String {
    let mut source = String::from(
        "; zeno-fcis/system-obligation/1\n(set-logic ALL)\n(set-option :produce-models true)\n",
    );
    for (index, domain) in inputs.iter().enumerate() {
        let name = format!("in_{index}");
        let _ = writeln!(source, "(declare-const {name} Int)");
        let _ = writeln!(source, "(assert {})", in_domain(*domain, &name));
    }
    source
}

fn finish(source: &mut String, names: &[String]) {
    source.push_str("(check-sat)\n");
    if !names.is_empty() {
        let _ = writeln!(source, "(get-value ({}))", names.join(" "));
    }
}

fn in_domain(domain: Domain, term: &str) -> String {
    let (min, max) = domain.bounds();
    format!("(<= {} {term} {})", int(min), int(max))
}

fn conjunction(mut terms: impl Iterator<Item = String>) -> String {
    let Some(first) = terms.next() else {
        return String::from("true");
    };
    let rest: Vec<String> = terms.collect();
    if rest.is_empty() {
        first
    } else {
        format!("(and {first} {})", rest.join(" "))
    }
}

fn int(value: i64) -> String {
    if value < 0 {
        format!("(- {})", value.unsigned_abs())
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use std::collections::BTreeMap;

    use zeno_fcis_synthesis::finite::Program;
    use zeno_fcis_synthesis::system::{SystemCheck, SystemLimits, check_system_property};

    use super::*;

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

    fn property(nodes: Vec<Op>) -> Property {
        let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| unreachable!());
        let relation = Program::try_new(vec![COUNT, COUNT], vec![Domain::Bool], nodes, vec![root])
            .unwrap_or_else(|error| panic!("property program: {error:?}"));
        Property::try_new(vec![COUNT], vec![COUNT], relation)
            .unwrap_or_else(|error| panic!("property: {error:?}"))
    }

    fn monotone() -> Property {
        property(vec![Op::Input(1), Op::Input(0), Op::Lt(0, 1), Op::Not(2)])
    }

    fn bounded() -> Property {
        property(vec![Op::Int(3), Op::Input(1), Op::Lt(0, 1), Op::Not(2)])
    }

    fn unchanged() -> Property {
        property(vec![Op::Input(1), Op::Input(0), Op::Eq(0, 1)])
    }

    fn text(obligations: &SystemObligations, kind: SystemObligationKind) -> String {
        String::from_utf8(obligations.source(kind).to_vec()).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn system_obligations_never_assume_output_domains_for_totality() {
        let obligations = export_system_smt(&counter(3), &monotone())
            .unwrap_or_else(|error| panic!("export: {error:?}"));
        let totality = text(&obligations, SystemObligationKind::Totality);
        // The output domain appears only inside the negated goal.
        assert!(!totality.contains("(assert (<= 0 t_5 3))"));
        assert!(totality.contains(
            "(assert (not (and (<= (- 9223372036854775808) t_4 9223372036854775807) (<= 0 t_5 3))))"
        ));
        // The property script assumes totality, which is proved separately.
        let property = text(&obligations, SystemObligationKind::Property);
        assert!(property.contains("(assert (<= 0 t_5 3))"));
        // The domain-only control removes the transition entirely.
        let domain_only = text(&obligations, SystemObligationKind::DomainOnly);
        assert!(!domain_only.contains("define-fun t_"));
        assert!(!domain_only.contains(" t_"));
        assert!(domain_only.contains("(declare-const free_out_0 Int)"));
        for kind in [
            SystemObligationKind::Totality,
            SystemObligationKind::Property,
        ] {
            assert!(text(&obligations, kind).ends_with("(check-sat)\n(get-value (in_0))\n"));
        }
        // The domain-only control also requests its proposed outputs, so its
        // model can be replayed.
        assert!(domain_only.ends_with("(check-sat)\n(get-value (in_0 free_out_0))\n"));
        assert_eq!((obligations.inputs(), obligations.outputs()), (1, 1));
    }

    #[test]
    fn system_obligations_are_deterministic_and_shape_checked() {
        let first = export_system_smt(&counter(3), &monotone());
        assert_eq!(first, export_system_smt(&counter(3), &monotone()));
        let wide = Program::try_new(
            vec![Domain::Int { min: 0, max: 7 }],
            vec![COUNT],
            vec![Op::Int(0)],
            vec![0],
        )
        .unwrap_or_else(|error| panic!("wide program: {error:?}"));
        assert_eq!(
            export_system_smt(&wide, &monotone()),
            Err(ExportError::InvalidFormula)
        );
        assert_eq!(SystemObligationKind::DomainOnly.code(), "domain-only");
    }

    /// Runs one script through a solver binary for the pinned differentials.
    fn run_solver(
        solver: &Path,
        kind: SystemObligationKind,
        script: &[u8],
        inputs: usize,
        outputs: usize,
    ) -> Result<SystemAnswer, SystemSolveError> {
        let mut child = Command::new(solver)
            .args(["--lang", "smt2"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("start solver: {error}"));
        child
            .stdin
            .take()
            .unwrap_or_else(|| unreachable!())
            .write_all(script)
            .unwrap_or_else(|error| panic!("write script: {error}"));
        let output = child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("solver output: {error}"));
        parse_system_answer(kind, &output.stdout, inputs, outputs)
    }

    #[test]
    fn system_answers_parse_models_and_fail_closed() {
        let kind = SystemObligationKind::Property;
        assert_eq!(
            parse_system_answer(kind, b"unsat\n", 2, 1),
            Ok(SystemAnswer::Unsat)
        );
        // Only the domain-only control reads proposed outputs.
        let model = b"sat\n((in_0 3) (in_1 (- 2)) (free_out_0 (- 4)))\n";
        assert_eq!(
            parse_system_answer(kind, model, 2, 1),
            Ok(SystemAnswer::Sat {
                input: vec![3, -2],
                output: vec![]
            })
        );
        assert_eq!(
            parse_system_answer(SystemObligationKind::DomainOnly, model, 2, 1),
            Ok(SystemAnswer::Sat {
                input: vec![3, -2],
                output: vec![-4]
            })
        );
        assert_eq!(
            parse_system_answer(
                SystemObligationKind::DomainOnly,
                b"sat\n((in_0 3) (in_1 1))\n",
                2,
                1
            ),
            Err(SystemSolveError::Solver(SystemObligationKind::DomainOnly))
        );
        for bad in [&b"unknown\n"[..], b"sat\n((in_0 3))\n", b"", b"\xff"] {
            assert_eq!(
                parse_system_answer(kind, bad, 2, 1),
                Err(SystemSolveError::Solver(kind))
            );
        }
    }

    #[test]
    fn system_verdict_rejects_models_the_interpreter_cannot_replay() {
        // A solver that claims every script is satisfiable at input 0 lies
        // about totality: counter(3) is total at 0, so the model is refused.
        let lying = |_, _: &[u8]| {
            Ok(SystemAnswer::Sat {
                input: vec![0],
                output: vec![],
            })
        };
        assert_eq!(
            system_verdict(&counter(3), &monotone(), lying),
            Err(SystemSolveError::UnreplayableModel {
                kind: SystemObligationKind::Totality,
                input: vec![0],
                output: vec![]
            })
        );
    }

    const BIT: Domain = Domain::Int { min: 0, max: 1 };

    fn identity_over_bits() -> Program {
        Program::try_new(vec![BIT], vec![BIT], vec![Op::Input(0)], vec![0])
            .unwrap_or_else(|error| panic!("identity: {error:?}"))
    }

    fn bit_property(nodes: Vec<Op>) -> Property {
        let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| unreachable!());
        let relation = Program::try_new(vec![BIT, BIT], vec![Domain::Bool], nodes, vec![root])
            .unwrap_or_else(|error| panic!("bit relation: {error:?}"));
        Property::try_new(vec![BIT], vec![BIT], relation)
            .unwrap_or_else(|error| panic!("bit property: {error:?}"))
    }

    /// Answers with `answer` for `kind` and `unsat` for every other script.
    fn answering(
        kind: SystemObligationKind,
        answer: SystemAnswer,
    ) -> impl FnMut(SystemObligationKind, &[u8]) -> Result<SystemAnswer, SystemSolveError> {
        move |asked, _| {
            Ok(if asked == kind {
                answer.clone()
            } else {
                SystemAnswer::Unsat
            })
        }
    }

    fn sat(input: Vec<i64>, output: Vec<i64>) -> SystemAnswer {
        SystemAnswer::Sat { input, output }
    }

    #[test]
    fn solver_models_must_be_admitted_before_they_are_replayed() {
        // Regression from the independent review of 521b768: the identity
        // over {0, 1} is total, yet the interpreter rejects input 2 at its own
        // domain check, which was reported as a totality failure.
        let identity = identity_over_bits();
        let always = bit_property(vec![Op::Bool(true)]);
        let refused = |kind, input: Vec<i64>, output: Vec<i64>| {
            Err(SystemSolveError::UnreplayableModel {
                kind,
                input,
                output,
            })
        };
        let totality = SystemObligationKind::Totality;
        for (input, output) in [
            (vec![2], vec![]),
            (vec![-1], vec![]),
            (vec![0, 0], vec![]),
            (vec![], vec![]),
            (vec![0], vec![0]),
        ] {
            assert_eq!(
                system_verdict(
                    &identity,
                    &always,
                    answering(totality, sat(input.clone(), output.clone()))
                ),
                refused(totality, input, output)
            );
        }
        // A property model must also be admitted, and must fail on replay.
        let property = SystemObligationKind::Property;
        for input in [vec![5], vec![0]] {
            assert_eq!(
                system_verdict(
                    &identity,
                    &always,
                    answering(property, sat(input.clone(), vec![]))
                ),
                refused(property, input, vec![])
            );
        }
    }

    #[test]
    fn domain_only_models_are_replayed_with_their_proposed_outputs() {
        let identity = identity_over_bits();
        let domain_only = SystemObligationKind::DomainOnly;
        // Regression from the independent review of 521b768: a satisfiable
        // domain-only answer for a constant-true property was accepted.
        let always = bit_property(vec![Op::Bool(true)]);
        assert_eq!(
            system_verdict(
                &identity,
                &always,
                answering(domain_only, sat(vec![0], vec![0]))
            ),
            Err(SystemSolveError::UnreplayableModel {
                kind: domain_only,
                input: vec![0],
                output: vec![0]
            })
        );
        // `post >= pre` is violated by the admitted pair (1, 0), so a model
        // proposing it shows the property depends on the transition.
        let monotone = bit_property(vec![Op::Input(1), Op::Input(0), Op::Lt(0, 1), Op::Not(2)]);
        assert_eq!(
            system_verdict(
                &identity,
                &monotone,
                answering(domain_only, sat(vec![1], vec![0]))
            ),
            Ok(SystemVerdict::SystemProperty)
        );
        // An output outside its domain, a missing output, or a pair that
        // satisfies the property is refused.
        for (input, output) in [
            (vec![1], vec![-1]),
            (vec![1], vec![]),
            (vec![0], vec![1]),
            (vec![2], vec![0]),
        ] {
            assert!(
                system_verdict(
                    &identity,
                    &monotone,
                    answering(domain_only, sat(input.clone(), output.clone()))
                )
                .is_err(),
                "{input:?} -> {output:?} must be refused"
            );
        }
    }

    const SMALL: Domain = Domain::Int { min: 0, max: 2 };

    /// One-input transitions over {0, 1, 2}. Several leave the output domain on
    /// some inputs and violate a property on others.
    fn small_transitions() -> Vec<(&'static str, Program)> {
        let program = |nodes: Vec<Op>| {
            let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| unreachable!());
            Program::try_new(vec![SMALL], vec![SMALL], nodes, vec![root])
                .unwrap_or_else(|error| panic!("small transition: {error:?}"))
        };
        vec![
            ("x", program(vec![Op::Input(0)])),
            (
                "x + 1",
                program(vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)]),
            ),
            (
                "x - 1",
                program(vec![Op::Input(0), Op::Int(1), Op::Sub(0, 1)]),
            ),
            (
                "2 - x",
                program(vec![Op::Int(2), Op::Input(0), Op::Sub(0, 1)]),
            ),
            ("x + x", program(vec![Op::Input(0), Op::Add(0, 0)])),
            (
                "x < 2 ? x + 1 : x",
                program(vec![
                    Op::Input(0),
                    Op::Int(2),
                    Op::Lt(0, 1),
                    Op::Int(1),
                    Op::Add(0, 3),
                    Op::Select(2, 4, 0),
                ]),
            ),
            (
                "x == 0 ? 2 : x - 1",
                program(vec![
                    Op::Input(0),
                    Op::Int(0),
                    Op::Eq(0, 1),
                    Op::Int(2),
                    Op::Int(1),
                    Op::Sub(0, 4),
                    Op::Select(2, 3, 5),
                ]),
            ),
            (
                "x < 1 ? x : x + 1",
                program(vec![
                    Op::Input(0),
                    Op::Int(1),
                    Op::Lt(0, 1),
                    Op::Add(0, 1),
                    Op::Select(2, 0, 3),
                ]),
            ),
            ("0", program(vec![Op::Int(0)])),
            ("2", program(vec![Op::Int(2)])),
        ]
    }

    /// Relations over (pre, post) on {0, 1, 2}.
    fn small_properties() -> Vec<(&'static str, Property)> {
        let property = |nodes: Vec<Op>| {
            let root = u16::try_from(nodes.len() - 1).unwrap_or_else(|_| unreachable!());
            let relation =
                Program::try_new(vec![SMALL, SMALL], vec![Domain::Bool], nodes, vec![root])
                    .unwrap_or_else(|error| panic!("small relation: {error:?}"));
            Property::try_new(vec![SMALL], vec![SMALL], relation)
                .unwrap_or_else(|error| panic!("small property: {error:?}"))
        };
        vec![
            ("true", property(vec![Op::Bool(true)])),
            (
                "post == pre",
                property(vec![Op::Input(1), Op::Input(0), Op::Eq(0, 1)]),
            ),
            (
                "post >= pre",
                property(vec![Op::Input(1), Op::Input(0), Op::Lt(0, 1), Op::Not(2)]),
            ),
            (
                "post == 1",
                property(vec![Op::Input(1), Op::Int(1), Op::Eq(0, 1)]),
            ),
            (
                "post <= 2",
                property(vec![Op::Int(2), Op::Input(1), Op::Lt(0, 1), Op::Not(2)]),
            ),
            (
                "post != pre",
                property(vec![Op::Input(1), Op::Input(0), Op::Eq(0, 1), Op::Not(2)]),
            ),
            (
                "pre + post <= 2",
                property(vec![
                    Op::Input(0),
                    Op::Input(1),
                    Op::Add(0, 1),
                    Op::Int(2),
                    Op::Lt(3, 2),
                    Op::Not(4),
                ]),
            ),
            // Traps whenever post is at least 1, because i64::MAX + post
            // overflows.
            (
                "0 < post + i64::MAX",
                property(vec![
                    Op::Input(1),
                    Op::Int(i64::MAX),
                    Op::Add(0, 1),
                    Op::Int(0),
                    Op::Lt(3, 2),
                ]),
            ),
        ]
    }

    /// Every tuple the domains admit, in the checkers' lexicographic order.
    fn tuples(domains: &[Domain]) -> Vec<Vec<i64>> {
        domains.iter().fold(vec![Vec::new()], |prefixes, domain| {
            let (min, max) = domain.bounds();
            prefixes
                .iter()
                .flat_map(|prefix| {
                    (min..=max).map(move |value| {
                        let mut tuple = prefix.clone();
                        tuple.push(value);
                        tuple
                    })
                })
                .collect()
        })
    }

    /// Answers each obligation by enumerating the question it asks. It shares
    /// no code with the SMT encoding, so agreement checks how answers are
    /// validated, replayed, and combined.
    fn reference_answer(
        transition: &Program,
        property: &Property,
        kind: SystemObligationKind,
    ) -> SystemAnswer {
        let fails = |input: &[i64], output: &[i64]| {
            !matches!(property.contract().holds(input, output), Ok(true))
        };
        let inputs = tuples(transition.inputs());
        let found = match kind {
            SystemObligationKind::Totality => inputs
                .into_iter()
                .find(|input| transition.evaluate(input).is_err())
                .map(|input| (input, Vec::new())),
            SystemObligationKind::Property => inputs
                .into_iter()
                .find(|input| {
                    transition
                        .evaluate(input)
                        .is_ok_and(|output| fails(input, &output))
                })
                .map(|input| (input, Vec::new())),
            SystemObligationKind::DomainOnly => inputs
                .iter()
                .flat_map(|input| {
                    tuples(transition.outputs())
                        .into_iter()
                        .map(move |output| (input.clone(), output))
                })
                .find(|(input, output)| fails(input, output)),
        };
        found.map_or(SystemAnswer::Unsat, |(input, output)| SystemAnswer::Sat {
            input,
            output,
        })
    }

    fn verdict_of(check: SystemCheck) -> SystemVerdict {
        match check {
            SystemCheck::SystemProperty { .. } => SystemVerdict::SystemProperty,
            SystemCheck::DomainImplied { .. } => SystemVerdict::DomainImplied,
            SystemCheck::NotTotal { input } => SystemVerdict::NotTotal { input },
            SystemCheck::Violated { input, output } => SystemVerdict::Violated { input, output },
            SystemCheck::Undefined { input, output } => SystemVerdict::Undefined { input, output },
            other => panic!("unexpected check {other:?}"),
        }
    }

    /// The stage a result belongs to. Two correct routes can report different
    /// witnesses, and so a violated rather than an undefined property.
    fn stage(code: &str) -> &str {
        match code {
            "violated" | "undefined" => "property",
            other => other,
        }
    }

    #[test]
    fn solver_route_agrees_with_exhaustive_route_on_small_programs() {
        let mut seen = BTreeMap::new();
        for (transition_name, transition) in small_transitions() {
            for (property_name, property) in small_properties() {
                let exhaustive = check_system_property(
                    &transition,
                    property.contract(),
                    SystemLimits::default(),
                )
                .unwrap_or_else(|error| panic!("exhaustive: {error:?}"));
                *seen.entry(exhaustive.code()).or_insert(0_u32) += 1;
                let verdict = system_verdict(&transition, &property, |kind, _| {
                    Ok(reference_answer(&transition, &property, kind))
                })
                .unwrap_or_else(|error| {
                    panic!("{transition_name} with {property_name}: {error:?}")
                });
                assert_eq!(
                    verdict,
                    verdict_of(exhaustive),
                    "{transition_name} with {property_name}"
                );
            }
        }
        // Every verdict occurs, including transitions that fail totality on
        // some inputs and the property on others.
        assert_eq!(seen.len(), 5, "{seen:?}");
        assert_eq!(seen.values().sum::<u32>(), 80);
    }

    #[test]
    #[ignore = "requires the workflow-pinned CVC5 executable"]
    fn pinned_system_smt_agrees_with_exhaustive_check() {
        let cvc5 = std::path::PathBuf::from(
            std::env::var_os("ZENO_FCIS_CVC5").unwrap_or_else(|| unreachable!()),
        );
        let cases = [
            (counter(3), monotone(), "system-property"),
            (counter(3), bounded(), "domain-implied"),
            (counter(3), unchanged(), "violated"),
            (counter(4), monotone(), "not-total"),
        ];
        for (transition, property, expected) in &cases {
            let exhaustive =
                check_system_property(transition, property.contract(), SystemLimits::default())
                    .unwrap_or_else(|error| panic!("exhaustive: {error:?}"));
            assert_eq!(exhaustive.code(), *expected);
            let verdict = system_verdict(transition, property, |kind, script| {
                run_solver(&cvc5, kind, script, 1, 1)
            })
            .unwrap_or_else(|error| panic!("solver verdict: {error:?}"));
            assert_eq!(verdict.code(), exhaustive.code());
            if let SystemCheck::NotTotal { input } = exhaustive {
                assert_eq!(verdict, SystemVerdict::NotTotal { input });
            }
        }
    }

    #[test]
    #[ignore = "requires the workflow-pinned CVC5 executable"]
    fn pinned_solver_route_agrees_with_exhaustive_route_on_small_programs() {
        let cvc5 = std::path::PathBuf::from(
            std::env::var_os("ZENO_FCIS_CVC5").unwrap_or_else(|| unreachable!()),
        );
        for (transition_name, transition) in small_transitions() {
            for (property_name, property) in small_properties() {
                let exhaustive = check_system_property(
                    &transition,
                    property.contract(),
                    SystemLimits::default(),
                )
                .unwrap_or_else(|error| panic!("exhaustive: {error:?}"));
                let verdict = system_verdict(&transition, &property, |kind, script| {
                    run_solver(&cvc5, kind, script, 1, 1)
                })
                .unwrap_or_else(|error| {
                    panic!("{transition_name} with {property_name}: {error:?}")
                });
                assert_eq!(
                    stage(verdict.code()),
                    stage(exhaustive.code()),
                    "{transition_name} with {property_name}"
                );
            }
        }
    }
}
