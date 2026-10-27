//! Export exact admitted Program data beside the production RustEmitter bytes.
use std::{fmt::Write, fs, path::PathBuf};
use zeno_fcis_synthesis::finite::emit::{RustEmitter, TargetEmitter};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

fn domains(values: &[Domain]) -> String {
    let mut text = String::from("[");
    for (i, d) in values.iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        let (min, max) = d.bounds();
        write!(
            text,
            "{{\"min\":\"{min}\",\"max\":\"{max}\",\"boolean\":{}}}",
            matches!(d, Domain::Bool)
        )
        .unwrap();
    }
    text.push(']');
    text
}

fn operation_program(name: &str) -> Program {
    let integer = Domain::Int {
        min: i128::MIN,
        max: i128::MAX,
    };
    let (inputs, op, output) = match name {
        "input" => (vec![integer], Op::Input(0), integer),
        "int" => (vec![], Op::Int(42), integer),
        "bool" => (vec![], Op::Bool(true), Domain::Bool),
        "add" => (vec![integer; 2], Op::Add(0, 1), integer),
        "sub" => (vec![integer; 2], Op::Sub(0, 1), integer),
        "mul" => (vec![integer; 2], Op::Mul(0, 1), integer),
        "div_floor" => (vec![integer; 2], Op::DivFloor(0, 1), integer),
        "div_ceil" => (vec![integer; 2], Op::DivCeil(0, 1), integer),
        "mul_div_floor" => (vec![integer; 3], Op::MulDivFloor(0, 1, 2), integer),
        "mul_div_ceil" => (vec![integer; 3], Op::MulDivCeil(0, 1, 2), integer),
        "signed_mul_div_floor" => (vec![integer; 3], Op::SignedMulDivFloor(0, 1, 2), integer),
        "products_ge" => (
            vec![integer; 6],
            Op::ProductsGe([0, 1, 2], [3, 4, 5]),
            Domain::Bool,
        ),
        "eq" => (vec![integer; 2], Op::Eq(0, 1), Domain::Bool),
        "lt" => (vec![integer; 2], Op::Lt(0, 1), Domain::Bool),
        "and" => (vec![Domain::Bool; 2], Op::And(0, 1), Domain::Bool),
        "not" => (vec![Domain::Bool], Op::Not(0), Domain::Bool),
        "select" => (
            vec![Domain::Bool, integer, integer],
            Op::Select(0, 1, 2),
            integer,
        ),
        "eager_select" => {
            return Program::try_new(
                vec![integer; 2],
                vec![integer],
                vec![
                    Op::Input(0),
                    Op::Input(1),
                    Op::Bool(false),
                    Op::DivFloor(0, 1),
                    Op::Int(0),
                    Op::Select(2, 3, 4),
                ],
                vec![5],
            )
            .unwrap();
        }
        _ => panic!("unrecognized arithmetic probe operation"),
    };
    let mut nodes: Vec<Op> = (0..inputs.len()).map(|i| Op::Input(i as u16)).collect();
    // Input itself is already present for its one-input probe.
    if name != "input" {
        nodes.push(op);
    }
    let root = nodes.len() as u16 - 1;
    Program::try_new(inputs, vec![output], nodes, vec![root]).unwrap()
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 3);
    let program = match args[1].to_str().unwrap().strip_prefix("op:") {
        Some(name) => operation_program(name),
        None => import_program(&fs::read(&args[1]).unwrap()).unwrap(),
    };
    let out = PathBuf::from(&args[2]);
    fs::create_dir_all(&out).unwrap();
    let mut text = format!(
        "{{\"inputs\":{},\"outputs\":{},\"nodes\":[",
        domains(program.inputs()),
        domains(program.outputs())
    );
    for (i, op) in program.nodes().iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        match *op {
            Op::Input(a) => write!(text, "[\"input\",{a}]"),
            Op::Int(a) => write!(text, "[\"int\",\"{a}\"]"),
            Op::Bool(a) => write!(text, "[\"bool\",{a}]"),
            Op::Add(a, b) => write!(text, "[\"add\",{a},{b}]"),
            Op::Sub(a, b) => write!(text, "[\"sub\",{a},{b}]"),
            Op::Mul(a, b) => write!(text, "[\"mul\",{a},{b}]"),
            Op::DivFloor(a, b) => write!(text, "[\"div_floor\",{a},{b}]"),
            Op::DivCeil(a, b) => write!(text, "[\"div_ceil\",{a},{b}]"),
            Op::MulDivFloor(a, b, d) => write!(text, "[\"mul_div_floor\",{a},{b},{d}]"),
            Op::MulDivCeil(a, b, d) => write!(text, "[\"mul_div_ceil\",{a},{b},{d}]"),
            Op::SignedMulDivFloor(a, b, d) => {
                write!(text, "[\"signed_mul_div_floor\",{a},{b},{d}]")
            }
            Op::ProductsGe(a, b) => write!(
                text,
                "[\"products_ge\",{},{},{},{},{},{}]",
                a[0], a[1], a[2], b[0], b[1], b[2]
            ),
            Op::Eq(a, b) => write!(text, "[\"eq\",{a},{b}]"),
            Op::Lt(a, b) => write!(text, "[\"lt\",{a},{b}]"),
            Op::And(a, b) => write!(text, "[\"and\",{a},{b}]"),
            Op::Not(a) => write!(text, "[\"not\",{a}]"),
            Op::Select(c, a, b) => write!(text, "[\"select\",{c},{a},{b}]"),
            _ => panic!("instruction outside the reviewed arithmetic inventory"),
        }
        .unwrap();
    }
    text.push_str("],\"roots\":[");
    for (i, r) in program.roots().iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        write!(text, "{r}").unwrap();
    }
    text.push_str("]}\n");
    fs::write(out.join("program.json"), text).unwrap();
    fs::write(
        out.join("transition.rs"),
        RustEmitter.emit(&program).unwrap(),
    )
    .unwrap();
}
