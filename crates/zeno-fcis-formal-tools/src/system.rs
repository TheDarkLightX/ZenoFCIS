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
//! Every satisfiable script requests the values of its inputs, so a model can
//! be replayed through the program interpreter before it is reported.

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
    finish(&mut totality, inputs);

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
    finish(&mut holds, inputs);

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
    finish(&mut domain_only, inputs);

    Ok(SystemObligations {
        inputs,
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
    /// Satisfiable, with the requested input values in order.
    Sat(Vec<i64>),
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
    /// A model did not reproduce its failure in the program interpreter.
    UnreplayableModel {
        /// The obligation that produced the model.
        kind: SystemObligationKind,
        /// The model's input values.
        input: Vec<i64>,
    },
}

/// Reads a solver's answer to one exported script: `unsat`, or `sat` followed
/// by the `(get-value (in_0 ...))` model the script requests.
///
/// # Errors
///
/// Returns [`SystemSolveError::Solver`] for `unknown`, missing, or malformed
/// output.
pub fn parse_system_answer(
    kind: SystemObligationKind,
    stdout: &[u8],
    inputs: usize,
) -> Result<SystemAnswer, SystemSolveError> {
    let text = core::str::from_utf8(stdout).map_err(|_| SystemSolveError::Solver(kind))?;
    match text.lines().next().map(str::trim) {
        Some("unsat") => Ok(SystemAnswer::Unsat),
        Some("sat") => (0..inputs)
            .map(|index| model_value(text, index))
            .collect::<Option<Vec<i64>>>()
            .map(SystemAnswer::Sat)
            .ok_or(SystemSolveError::Solver(kind)),
        _ => Err(SystemSolveError::Solver(kind)),
    }
}

/// Reads `(in_k v)`, accepting a negative value printed as `(- n)`.
fn model_value(text: &str, index: usize) -> Option<i64> {
    let key = format!("(in_{index} ");
    let rest = &text[text.find(&key)? + key.len()..];
    let (negative, rest) = rest
        .strip_prefix("(- ")
        .map_or((false, rest), |rest| (true, rest));
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let magnitude: i128 = digits.parse().ok()?;
    i64::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// Exports the obligations for `property`, asks `solve` for each in order, and
/// combines the answers as the exhaustive checker does. A model is accepted
/// only after the program interpreter reproduces its failure.
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
    if let SystemAnswer::Sat(input) = ask(SystemObligationKind::Totality)? {
        return if transition.evaluate(&input).is_err() {
            Ok(SystemVerdict::NotTotal { input })
        } else {
            Err(SystemSolveError::UnreplayableModel {
                kind: SystemObligationKind::Totality,
                input,
            })
        };
    }
    if let SystemAnswer::Sat(input) = ask(SystemObligationKind::Property)? {
        let unreplayable = |input| SystemSolveError::UnreplayableModel {
            kind: SystemObligationKind::Property,
            input,
        };
        let Ok(output) = transition.evaluate(&input) else {
            return Err(unreplayable(input));
        };
        return match property.contract().holds(&input, &output) {
            Ok(false) => Ok(SystemVerdict::Violated { input, output }),
            Err(_) => Ok(SystemVerdict::Undefined { input, output }),
            Ok(true) => Err(unreplayable(input)),
        };
    }
    Ok(match ask(SystemObligationKind::DomainOnly)? {
        SystemAnswer::Unsat => SystemVerdict::DomainImplied,
        SystemAnswer::Sat(_) => SystemVerdict::SystemProperty,
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

fn finish(source: &mut String, inputs: usize) {
    source.push_str("(check-sat)\n");
    if inputs > 0 {
        let names: Vec<String> = (0..inputs).map(|index| format!("in_{index}")).collect();
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
        for kind in SystemObligationKind::ALL {
            assert!(text(&obligations, kind).ends_with("(check-sat)\n(get-value (in_0))\n"));
        }
        assert_eq!(obligations.inputs(), 1);
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

    /// Runs one script through a solver binary for the pinned differential.
    fn run_solver(
        solver: &Path,
        kind: SystemObligationKind,
        script: &[u8],
        inputs: usize,
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
        parse_system_answer(kind, &output.stdout, inputs)
    }

    #[test]
    fn system_answers_parse_models_and_fail_closed() {
        let kind = SystemObligationKind::Property;
        assert_eq!(
            parse_system_answer(kind, b"unsat\n", 2),
            Ok(SystemAnswer::Unsat)
        );
        assert_eq!(
            parse_system_answer(kind, b"sat\n((in_0 3) (in_1 (- 2)))\n", 2),
            Ok(SystemAnswer::Sat(vec![3, -2]))
        );
        for bad in [&b"unknown\n"[..], b"sat\n((in_0 3))\n", b"", b"\xff"] {
            assert_eq!(
                parse_system_answer(kind, bad, 2),
                Err(SystemSolveError::Solver(kind))
            );
        }
    }

    #[test]
    fn system_verdict_rejects_models_the_interpreter_cannot_replay() {
        // A solver that claims every script is satisfiable at input 0 lies
        // about totality: counter(3) is total at 0, so the model is refused.
        let lying = |_, _: &[u8]| Ok(SystemAnswer::Sat(vec![0]));
        assert_eq!(
            system_verdict(&counter(3), &monotone(), lying),
            Err(SystemSolveError::UnreplayableModel {
                kind: SystemObligationKind::Totality,
                input: vec![0]
            })
        );
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
                run_solver(&cvc5, kind, script, 1)
            })
            .unwrap_or_else(|error| panic!("solver verdict: {error:?}"));
            assert_eq!(verdict.code(), exhaustive.code());
            if let SystemCheck::NotTotal { input } = exhaustive {
                assert_eq!(verdict, SystemVerdict::NotTotal { input });
            }
        }
    }
}
