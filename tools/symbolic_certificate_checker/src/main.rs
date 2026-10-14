//! Independent, complete root-contradiction checking for the declared Int/Bool fragment.
//!
//! Carcara is a named trusted checker, not a solver status or a Lean theorem.
//! Proof generation and elaboration are untrusted. No hole or unknown inference
//! grants acceptance, and a local contradiction does not establish root UNSAT.
#![forbid(unsafe_code)]
use carcara::{
    ast::{Constant, Operator, ProofCommand, Rc, Sort, Term},
    checker, parser,
};
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
    process::ExitCode,
};
const PIN: &str = "5f9f77bf83c52748dccc12829ce5e80903de0c99";
fn read(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > cap {
        return Err("input is not a bounded regular file".into());
    }
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("input changed to a nonregular file".into());
    }
    let mut bytes = Vec::new();
    file.take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > cap {
        return Err("input grew beyond cap".into());
    }
    Ok(bytes)
}

fn fragment(roots: impl IntoIterator<Item = Rc<Term>>) -> Result<(), String> {
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut work = 0usize;
    while let Some(term) = pending.pop() {
        work += 1;
        if work > 2_000_000 {
            return Err("fragment work limit exceeded".into());
        }
        match term.as_ref() {
            Term::Const(Constant::Integer(_)) => (),
            Term::Var(_, sort) if matches!(sort.as_ref(), Term::Sort(Sort::Int | Sort::Bool)) => (),
            Term::Op(op, arguments) => {
                match op {
                    Operator::True
                    | Operator::False
                    | Operator::Not
                    | Operator::Implies
                    | Operator::And
                    | Operator::Or
                    | Operator::Xor
                    | Operator::Equals
                    | Operator::Ite
                    | Operator::Add
                    | Operator::Sub
                    | Operator::LessThan
                    | Operator::GreaterThan
                    | Operator::LessEq
                    | Operator::GreaterEq => (),
                    Operator::Mult
                        if arguments.len() == 2
                            && arguments.iter().any(|arg| {
                                matches!(arg.as_ref(), Term::Const(Constant::Integer(_)))
                            }) =>
                    {
                        ()
                    }
                    _ => return Err("operator outside checked linear Int/Bool fragment".into()),
                }
                pending.extend(arguments.iter().cloned());
            }
            _ => return Err("term outside checked linear Int/Bool fragment".into()),
        }
    }
    Ok(())
}

fn single_query(query: &[u8]) -> Result<(), String> {
    // Admit only the flat command profile emitted by the scalar query builder.
    // Carcara ignores stateful SMT commands, so it must never receive them.
    let text = std::str::from_utf8(query).map_err(|_| "query is not UTF-8")?;
    let bytes = text.as_bytes();
    let mut position = 0usize;
    let mut depth = 0usize;
    let mut commands = Vec::new();
    let mut tokens = Vec::new();
    while position < bytes.len() {
        match bytes[position] {
            b';' => {
                while position < bytes.len() && bytes[position] != b'\n' {
                    position += 1;
                }
            }
            byte if byte.is_ascii_whitespace() => position += 1,
            b'(' => {
                depth += 1;
                if depth > 4096 {
                    return Err("query nesting limit exceeded".into());
                }
                tokens.push("(");
                position += 1;
            }
            b')' => {
                if depth == 0 {
                    return Err("unbalanced query".into());
                }
                tokens.push(")");
                depth -= 1;
                position += 1;
                if depth == 0 {
                    commands.push(std::mem::take(&mut tokens));
                }
            }
            b'"' | b'|' => return Err("quoted query tokens outside declared profile".into()),
            _ => {
                if depth == 0 {
                    return Err("token outside query command".into());
                }
                let start = position;
                while position < bytes.len()
                    && !bytes[position].is_ascii_whitespace()
                    && !b"();".contains(&bytes[position])
                {
                    if !bytes[position].is_ascii() || b"\"|".contains(&bytes[position]) {
                        return Err("query token outside declared profile".into());
                    }
                    position += 1;
                }
                tokens.push(&text[start..position]);
            }
        }
    }
    if depth != 0 || !tokens.is_empty() {
        return Err("incomplete query".into());
    }
    if commands.first().map(Vec::as_slice) != Some(&["(", "set-logic", "ALL", ")"][..])
        || commands.last().map(Vec::as_slice) != Some(&["(", "check-sat", ")"][..])
    {
        return Err("expected one final check-sat under ALL".into());
    }
    let mut option_seen = false;
    for command in commands
        .iter()
        .skip(1)
        .take(commands.len().saturating_sub(2))
    {
        match command.get(1).copied() {
            Some("set-option")
                if !option_seen
                    && command.as_slice()
                        == ["(", "set-option", ":produce-models", "true", ")"] =>
            {
                option_seen = true
            }
            Some("declare-const" | "define-fun" | "assert") => (),
            _ => return Err("command outside single-query profile".into()),
        }
    }
    Ok(())
}

fn checked(query: &[u8], certificate: &[u8]) -> Result<(), String> {
    single_query(query)?;
    // Pinned CVC5 emits one outer list after its UNSAT response. Keep the
    // original certificate as evidence and remove only this exact transport
    // envelope; the existing Carcara parser must consume its contents to EOF.
    let certificate = if let Some(body) = certificate.strip_prefix(b"unsat\n(\n") {
        body.strip_suffix(b"\n)\n")
            .ok_or("incomplete CVC5 proof envelope")?
    } else {
        certificate
    };
    let config = parser::Config {
        apply_function_defs: false,
        expand_lets: true,
        parse_hole_args: true,
        ..Default::default()
    };
    let (problem, proof, mut pool) =
        parser::parse_instance(Cursor::new(query), Cursor::new(certificate), config)
            .map_err(|e| e.to_string())?;
    fragment(problem.premises.iter().cloned())?;
    if problem
        .premises
        .iter()
        .any(|term| term.raw_sort() != Sort::Bool)
    {
        return Err("assertion is not Boolean".into());
    }
    fragment(
        proof
            .constant_definitions
            .iter()
            .map(|(_, term)| term.clone()),
    )?;
    for command in &proof.commands {
        match command {
            ProofCommand::Assume { term, .. } => {
                fragment([term.clone()])?;
                if term.raw_sort() != Sort::Bool {
                    return Err("assumption is not Boolean".into());
                }
            }
            ProofCommand::Step(step) => {
                if step.rule == "hole" || step.rule == "lia_generic" {
                    return Err("incomplete inference is outside checked fragment".into());
                }
                fragment(step.clause.iter().cloned())?;
                if step.clause.iter().any(|term| term.raw_sort() != Sort::Bool) {
                    return Err("clause literal is not Boolean".into());
                }
                fragment(step.args.iter().cloned())?;
            }
            ProofCommand::Subproof(_) => return Err("subproof is outside checked fragment".into()),
        }
    }
    // A local contradiction under an assumption cannot establish root UNSAT.
    if !matches!(proof.commands.last(), Some(ProofCommand::Step(step)) if step.clause.is_empty()) {
        return Err("missing final root-level empty clause".into());
    }
    let mut checker = checker::ProofChecker::new(&mut pool, checker::Config::default());
    match checker.check(&problem, &proof).map_err(|e| e.to_string())? {
        false => Ok(()),
        true => Err("incomplete certificate contains holes".into()),
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("zfcis-carcara-root-check 0.1.1 {PIN}");
        return Ok(());
    }
    if args.len() != 2 {
        return Err("usage: zfcis-carcara-root-check query.smt2 certificate.alethe".into());
    }
    let query = read(Path::new(&args[0]), 1_048_576)?;
    let certificate = read(Path::new(&args[1]), 33_554_432)?;
    checked(&query, &certificate)?;
    println!("checked-complete-root");
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("certificate refused: {e}");
            ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::checked;
    #[test]
    fn actual_root_contradiction_passes() {
        assert!(checked(b"(set-logic ALL)\n(assert false)\n(check-sat)\n", b"(assume a false)\n(step b (cl (not false)) :rule false)\n(step c (cl) :rule resolution :premises (a b))\n").is_ok());
    }
    #[test]
    fn stateful_multi_query_and_non_boolean_assertions_are_refused() {
        let proof = b"(assume a false)\n(step b (cl (not false)) :rule false)\n(step c (cl) :rule resolution :premises (a b))\n";
        for query in [
            b"(set-logic ALL)\n(push 1)\n(assert false)\n(pop 1)\n(check-sat)\n".as_slice(),
            b"(set-logic ALL)\n(assert false)\n(reset-assertions)\n(check-sat)\n".as_slice(),
            b"(set-logic ALL)\n(assert false)\n(check-sat)\n(check-sat)\n".as_slice(),
            b"(set-logic ALL)\n(assert 1)\n(assert false)\n(check-sat)\n".as_slice(),
        ] {
            assert!(checked(query, proof).is_err());
        }
    }
    #[test]
    fn hole_is_refused() {
        assert!(checked(
            b"(set-logic ALL)\n(assert true)\n(check-sat)\n",
            b"(step bad (cl) :rule hole)\n"
        )
        .is_err());
    }
    #[test]
    fn local_contradiction_is_refused() {
        assert!(checked(b"(set-logic ALL)\n(assert true)\n(check-sat)\n", b"(anchor :step s3)\n(assume s0 false)\n(step s1 (cl (not false)) :rule false)\n(step s2 (cl) :rule resolution :premises (s0 s1))\n(step s3 (cl (not false) false) :rule subproof :discharge (s0))\n").is_err());
    }
    #[test]
    fn truncated_and_unknown_proofs_are_refused() {
        let query = b"(set-logic ALL)\n(assert false)\n(check-sat)\n";
        for proof in [
            b"(step bad (cl) :rule invented)\n".as_slice(),
            b"(step bad (cl) :rule hole\n".as_slice(),
        ] {
            assert!(checked(query, proof).is_err());
        }
    }
    #[test]
    fn real_and_non_linear_terms_are_refused() {
        for query in [
            b"(set-logic ALL)\n(declare-const x Real)\n(assert (= x 1.0))\n(assert false)\n(check-sat)\n".as_slice(),
            b"(set-logic ALL)\n(declare-const x Int)\n(assert (= (* x x) 0))\n(assert false)\n(check-sat)\n".as_slice(),
        ] {
            assert!(checked(query, b"(assume a false)\n(step b (cl (not false)) :rule false)\n(step c (cl) :rule resolution :premises (a b))\n").is_err());
        }
    }
}
