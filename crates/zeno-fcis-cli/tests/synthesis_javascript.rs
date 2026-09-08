//! Independent execution probes for the emitted module, including refuted programs.
#![allow(clippy::unwrap_used)]

use std::process::Command;
use zeno_fcis_synthesis::finite::emit::{JavaScriptEmitter, TargetEmitter};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

const I64: Domain = Domain::Int {
    min: i64::MIN,
    max: i64::MAX,
};

fn execute(
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    nodes: Vec<Op>,
    roots: Vec<u16>,
    checks: &str,
) {
    let program = Program::try_new(inputs, outputs, nodes, roots).unwrap();
    let mut source = JavaScriptEmitter.emit(&program).unwrap();
    source.push_str("\nif (process.versions.node.split('.')[0] !== '22') throw new Error('Node.js 22 is required');\nfunction expect(input, output) { if (transition(input) !== output) throw new Error('unexpected transition result'); }\n");
    source.push_str(checks);
    source.push_str("\nconsole.log('passed');\n");
    let node = std::env::var_os("NODE_BIN").unwrap_or_else(|| "node".into());
    let output = Command::new(node)
        .args(["--input-type=module", "--eval", &source])
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .output()
        .unwrap_or_else(|error| panic!("the runtime gate requires Node.js 22: {error}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"passed\n");
}

#[test]
#[ignore = "requires Node.js; the finite-synthesis ATDD scenario runs this explicitly"]
fn javascript_executes_precision_overflow_and_inert_input_boundaries() {
    execute(
        vec![I64],
        vec![I64],
        vec![Op::Input(0)],
        vec![0],
        r#"
        for (const input of ['0', '-1', '9007199254740993', '-9007199254740993',
                             '9223372036854775807', '-9223372036854775808']) expect(input, input);
        let touched = 0;
        const hostile = { [Symbol.toPrimitive]() { ++touched; throw new Error('coercion'); } };
        const proxy = new Proxy([], { get() { ++touched; throw new Error('get'); } });
        const revoked = Proxy.revocable({}, {}); revoked.revoke();
        for (const input of [null, undefined, true, false, 0, 0n, [], {}, new String('0'), hostile, proxy, revoked.proxy,
                             '', '+1', '-0', '00', '-01', '1e0', '0x1', '1.0', 'NaN', 'Infinity',
                             ' 1', '1 ', '1  2', '1\n', '1\r', '1\t', '\u0661',
                             '9223372036854775808', '-9223372036854775809', '1'.repeat(10000)]) {
            expect(input, null);
        }
        if (touched !== 0) throw new Error('input hooks executed');
    "#,
    );
    execute(
        vec![I64, I64],
        vec![I64, I64],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1), Op::Sub(0, 1)],
        vec![2, 3],
        r#"
        expect('9007199254740992 1', '9007199254740993 9007199254740991');
        expect('9223372036854775807 0', '9223372036854775807 9223372036854775807');
        expect('-9223372036854775808 0', '-9223372036854775808 -9223372036854775808');
        for (const input of ['9223372036854775807 1', '-9223372036854775808 1',
                             '-9223372036854775808 -1', '9223372036854775807 -1',
                             '0', '0 0 0', '0  0']) expect(input, null);
    "#,
    );
    execute(
        vec![],
        vec![I64],
        vec![
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Bool(false),
            Op::Int(7),
            Op::Select(3, 2, 4),
        ],
        vec![5],
        "expect('', null);",
    );
    execute(
        vec![],
        vec![I64],
        vec![Op::Int(i64::MIN), Op::Int(1), Op::Sub(0, 1), Op::Int(7)],
        vec![3],
        "expect('', null);",
    );
    execute(
        vec![],
        vec![I64],
        vec![Op::Int(i64::MIN)],
        vec![0],
        r#"
        expect('', '-9223372036854775808'); expect('0', null); expect(' ', null);
    "#,
    );
    execute(
        vec![I64; 16],
        vec![I64],
        vec![Op::Input(15)],
        vec![0],
        r#"
        expect(Array(16).fill('-9223372036854775808').join(' '), '-9223372036854775808');
        expect(Array(15).fill('0').join(' '), null);
        expect(Array(17).fill('0').join(' '), null);
    "#,
    );
    execute(
        vec![Domain::Bool, Domain::Bool, I64],
        vec![
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            I64,
        ],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::And(0, 1),
            Op::Not(0),
            Op::Eq(0, 1),
            Op::Bool(true),
            Op::Eq(3, 5),
            Op::Input(2),
            Op::Int(0),
            Op::Lt(7, 8),
            Op::Select(9, 8, 7),
        ],
        vec![2, 3, 4, 6, 9, 10],
        r#"
        expect('1 0 -5', '0 0 0 0 1 0'); expect('0 0 7', '0 1 1 1 0 7');
        expect('1 1 0', '1 0 1 0 0 0'); expect('2 0 7', null); expect('0 -1 7', null);
    "#,
    );
    execute(
        vec![Domain::Int { min: 0, max: 2 }],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![Op::Input(0)],
        vec![0],
        "expect('1', '1'); expect('2', null); expect('-1', null);",
    );
}
