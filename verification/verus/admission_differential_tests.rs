use super::admission_baseline as oracle;
use super::evaluation::admission::{self, AdmissionFailure};
use super::evaluation::{Domain, Op};
use alloc::{vec, vec::Vec};
fn legacy<T>(r: Result<T, oracle::Error>) -> Result<T, &'static str> {
    r.map_err(|oracle::Error::Invalid(s)| s)
}
fn actual<T>(r: Result<T, AdmissionFailure>) -> Result<T, &'static str> {
    r.map_err(|e| match e {
        AdmissionFailure::Shape => "program-shape",
        AdmissionFailure::InputReference => "input-reference",
        AdmissionFailure::NodeReference => "node-reference",
        AdmissionFailure::TypeMismatch => "type-mismatch",
        AdmissionFailure::OutputType => "output-type",
    })
}
fn words<T: Clone>(alphabet: &[T], maximum: usize) -> Vec<Vec<T>> {
    let mut all = vec![vec![]];
    let mut layer = vec![vec![]];
    for _ in 0..maximum {
        let mut next = Vec::new();
        for prefix in &layer {
            for element in alphabet {
                let mut word = prefix.clone();
                word.push(element.clone());
                next.push(word);
            }
        }
        all.extend(next.iter().cloned());
        layer = next;
    }
    all
}
fn domains() -> [Domain; 4] {
    [
        Domain::Bool,
        Domain::Int { min: -2, max: 2 },
        Domain::Int { min: 1, max: 0 },
        Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        },
    ]
}
#[test]
fn all_ten_kinds_match_baseline_with_ordered_failures() {
    let refs = [0u16, 1, 2, 3, 4, u16::MAX];
    let mut ops = vec![
        Op::Int(i64::MIN),
        Op::Int(i64::MAX),
        Op::Bool(false),
        Op::Bool(true),
    ];
    for a in refs {
        ops.push(Op::Input(a));
        ops.push(Op::Not(a));
        for b in refs {
            ops.extend([
                Op::Add(a, b),
                Op::Sub(a, b),
                Op::Eq(a, b),
                Op::Lt(a, b),
                Op::And(a, b),
            ]);
            for c in refs {
                ops.push(Op::Select(a, b, c));
            }
        }
    }
    let inputs = words(&domains(), 3);
    let previous = words(&[false, true], 4);
    let mut cases = 0usize;
    for input in &inputs {
        for kinds in &previous {
            for op in &ops {
                assert_eq!(
                    actual(admission::kind(op, input, kinds)),
                    legacy(op.legacy_kind(input, kinds)),
                    "op={op:?}; inputs={input:?}; previous={kinds:?}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(
        admission::kind(&Op::Add(0, u16::MAX), &[], &[true]),
        Err(AdmissionFailure::TypeMismatch)
    );
    assert_eq!(
        admission::kind(&Op::And(0, u16::MAX), &[], &[false]),
        Err(AdmissionFailure::TypeMismatch)
    );
    assert_eq!(
        admission::kind(&Op::Eq(0, u16::MAX), &[], &[true]),
        Err(AdmissionFailure::NodeReference)
    );
    assert_eq!(
        admission::kind(&Op::Select(0, u16::MAX, u16::MAX), &[], &[false]),
        Err(AdmissionFailure::TypeMismatch)
    );
    let large = vec![Domain::Bool; usize::from(u16::MAX) + 1];
    assert_eq!(
        actual(admission::kind(&Op::Input(u16::MAX), &large, &[])),
        legacy(Op::Input(u16::MAX).legacy_kind(&large, &[]))
    );
    let large = vec![false; usize::from(u16::MAX) + 1];
    assert_eq!(
        actual(admission::kind(&Op::Add(u16::MAX, u16::MAX), &[], &large)),
        legacy(Op::Add(u16::MAX, u16::MAX).legacy_kind(&[], &large))
    );
    assert_eq!(cases, 1_085_620);
}
#[test]
fn shape_boundaries_and_invalid_domains_match_baseline() {
    let mut cases = 0usize;
    for ninput in [0usize, 1, 31, 32, 33] {
        for noutput in [0usize, 1, 15, 16, 17] {
            for nroot in [0usize, 1, 15, 16, 17] {
                for nnode in [0usize, 1, 255, 256, 257, usize::MAX] {
                    for variant in 0..7 {
                        let mut input = vec![Domain::Bool; ninput];
                        let mut output = vec![Domain::Bool; noutput];
                        let invalid = Domain::Int {
                            min: i64::MAX,
                            max: i64::MIN,
                        };
                        if variant == 1 {
                            input.fill(Domain::Int {
                                min: i64::MIN,
                                max: i64::MAX,
                            });
                            output.fill(Domain::Int {
                                min: i64::MIN,
                                max: i64::MAX,
                            });
                        }
                        if variant == 2 && !input.is_empty() {
                            input[0] = invalid;
                        }
                        if variant == 3 && !input.is_empty() {
                            input[ninput - 1] = invalid;
                        }
                        if variant == 4 && !output.is_empty() {
                            output[0] = invalid;
                        }
                        if variant == 5 && !output.is_empty() {
                            output[noutput - 1] = invalid;
                        }
                        if variant == 6 {
                            input.fill(Domain::Int { min: 0, max: 0 });
                            output.fill(Domain::Int { min: -1, max: -1 });
                        }
                        let roots = vec![u16::MAX; nroot];
                        assert_eq!(
                            actual(admission::validate_shape(&input, &output, nnode, &roots)),
                            legacy(oracle::legacy_validate_shape(
                                &input, &output, nnode, &roots
                            )),
                            "input={ninput}; output={noutput}; nodes={nnode}; roots={nroot}; variant={variant}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 5_250);
}
#[test]
fn standalone_root_zip_behavior_matches_baseline() {
    let outputs = words(&domains(), 3);
    let roots = words(&[0u16, 1, 2, 3, 4, u16::MAX], 3);
    let kinds = words(&[false, true], 4);
    let mut cases = 0usize;
    for output in &outputs {
        for root in &roots {
            for previous in &kinds {
                assert_eq!(
                    actual(admission::validate_roots(output, root, previous)),
                    legacy(oracle::legacy_validate_roots(output, root, previous)),
                    "outputs={output:?}; roots={root:?}; kinds={previous:?}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(
        admission::validate_roots(&[Domain::Bool, Domain::Bool], &[0], &[true]),
        Ok(())
    );
    assert_eq!(
        admission::validate_roots(&[Domain::Bool], &[0, u16::MAX], &[true]),
        Ok(())
    );
    assert_eq!(admission::validate_roots(&[], &[u16::MAX], &[]), Ok(()));
    assert_eq!(cases, 682_465);
}
#[test]
fn program_shape_prefix_and_root_precedence_match_baseline() {
    let alphabet = [
        Op::Int(0),
        Op::Bool(false),
        Op::Input(0),
        Op::Input(1),
        Op::Input(u16::MAX),
        Op::Add(0, 1),
        Op::And(0, 1),
        Op::Eq(0, 1),
        Op::Not(0),
        Op::Select(0, 1, 2),
    ];
    let programs = words(&alphabet, 3);
    let inputs = words(&domains(), 2);
    let outputs = [
        vec![],
        vec![Domain::Bool],
        vec![Domain::Int { min: -2, max: 2 }],
        vec![Domain::Int { min: 1, max: 0 }],
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool, Domain::Int { min: -2, max: 2 }],
    ];
    let roots = [
        vec![],
        vec![0u16],
        vec![1],
        vec![2],
        vec![u16::MAX],
        vec![0, 1],
        vec![1, 0],
    ];
    let mut cases = 0usize;
    for program in &programs {
        for input in &inputs {
            for output in &outputs {
                for root in &roots {
                    assert_eq!(
                        actual(admission::validate_program(input, output, program, root)),
                        legacy(oracle::legacy_program(input, output, program, root)),
                        "inputs={input:?}; outputs={output:?}; nodes={program:?}; roots={root:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    let input = vec![
        Domain::Int {
            min: i64::MIN,
            max: i64::MAX
        };
        32
    ];
    let output = vec![
        Domain::Int {
            min: i64::MIN,
            max: i64::MAX
        };
        16
    ];
    let program = vec![Op::Int(0); 256];
    let root = vec![255u16; 16];
    assert_eq!(
        actual(admission::validate_program(
            &input, &output, &program, &root
        )),
        legacy(oracle::legacy_program(&input, &output, &program, &root))
    );
    assert_eq!(
        admission::validate_program(&input, &output, &program, &root),
        Ok(())
    );
    assert_eq!(
        admission::validate_program(&[], &[], &[Op::Input(u16::MAX)], &[]),
        Err(AdmissionFailure::Shape)
    );
    assert_eq!(cases, 979_902);
}
