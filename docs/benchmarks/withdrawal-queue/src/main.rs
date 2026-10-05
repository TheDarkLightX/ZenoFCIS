//! Offline source-derived experiment; creates no application authority.
use std::{env, fs, path::Path};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::{
    finite::{
        Domain, Op, Program, V2ExecutionFailure as Failure, V2Resource as Resource, v2_zero_limits,
    },
    finite_runtime::import_program,
};

#[allow(dead_code)]
#[path = "../../../../crates/zeno-fcis-cli/templates/withdrawal-queue/src/v2_contract.rs"]
mod production;
#[path = "../../../../crates/zeno-fcis-cli/templates/withdrawal-queue/synthesized/transition.rs"]
mod retained_controller;

type Checked<T> = Result<T, Box<dyn std::error::Error>>;

fn operands(op: &Op) -> Vec<u16> {
    match *op {
        Op::Input(_) | Op::Int(_) | Op::Bool(_) => vec![],
        Op::Not(a) => vec![a],
        Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => vec![a, b],
        Op::Select(c, a, b) => vec![c, a, b],
        _ => panic!("unsupported new instruction; update the offline experiment"),
    }
}

fn remap(op: &Op, ids: &[u16]) -> Op {
    match *op {
        Op::Input(a) => Op::Input(a),
        Op::Int(a) => Op::Int(a),
        Op::Bool(a) => Op::Bool(a),
        Op::Not(a) => Op::Not(ids[a as usize]),
        Op::Add(a, b) => Op::Add(ids[a as usize], ids[b as usize]),
        Op::Sub(a, b) => Op::Sub(ids[a as usize], ids[b as usize]),
        Op::Eq(a, b) => Op::Eq(ids[a as usize], ids[b as usize]),
        Op::Lt(a, b) => Op::Lt(ids[a as usize], ids[b as usize]),
        Op::And(a, b) => Op::And(ids[a as usize], ids[b as usize]),
        Op::Select(c, a, b) => Op::Select(ids[c as usize], ids[a as usize], ids[b as usize]),
        _ => panic!("unsupported new instruction; update the offline experiment"),
    }
}

fn rebuild(p: &Program, nodes: Vec<Op>, roots: Vec<u16>) -> Checked<Program> {
    Ok(Program::try_new(
        p.inputs().to_vec(),
        p.outputs().to_vec(),
        nodes,
        roots,
    )?)
}

/// Only contiguous four-node ORs whose three intermediates have no other users.
/// Does not prune arithmetic, constants, inputs, roots or unrelated instructions.
fn propose(p: &Program) -> Checked<(Program, Program, usize)> {
    let mut reduced = Vec::new();
    let mut padded = p.nodes().to_vec();
    let mut ids = vec![0; p.nodes().len()];
    let mut i = 0;
    let mut replacements = 0;
    while i < p.nodes().len() {
        let pattern = match p.nodes().get(i..i + 4) {
            Some([Op::Not(a), Op::Not(b), Op::And(x, y), Op::Not(z)])
                if (*a as usize) < i
                    && (*b as usize) < i
                    && *x as usize == i
                    && *y as usize == i + 1
                    && *z as usize == i + 2 =>
            {
                Some((*a, *b))
            }
            _ => None,
        };
        let private = !(0..3).any(|offset| {
            let id = (i + offset) as u16;
            p.roots().contains(&id)
                || p.nodes()
                    .iter()
                    .enumerate()
                    .any(|(j, op)| !(i..i + 4).contains(&j) && operands(op).contains(&id))
        });
        if let Some((a, b)) = pattern.filter(|_| private) {
            // Preserve exact instruction positions in the padded control.
            padded[i..i + 3].fill(Op::Bool(false));
            padded[i + 3] = Op::Select(a, a, b);
            ids[i + 3] = reduced.len() as u16;
            reduced.push(Op::Select(
                ids[a as usize],
                ids[a as usize],
                ids[b as usize],
            ));
            i += 4;
            replacements += 1;
        } else {
            ids[i] = reduced.len() as u16;
            reduced.push(remap(&p.nodes()[i], &ids));
            i += 1;
        }
    }
    let roots = p.roots().iter().map(|id| ids[*id as usize]).collect();
    Ok((
        rebuild(p, reduced, roots)?,
        rebuild(p, padded, p.roots().to_vec())?,
        replacements,
    ))
}

fn observe(p: &Program, input: &[i64], budget: u64) -> (Result<Vec<i64>, Failure>, [u64; 8]) {
    let (result, used) = p
        .execute_v2(input, v2_zero_limits().with_limit(Resource::Step, budget))
        .into_parts();
    let resources = [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ];
    (result, resources.map(|r| used.used(r)))
}

fn cardinality(domains: &[Domain]) -> usize {
    domains
        .iter()
        .map(|d| {
            let (lo, hi) = d.bounds();
            usize::try_from(i128::from(hi) - i128::from(lo) + 1).unwrap()
        })
        .try_fold(1usize, |n, width| n.checked_mul(width))
        .unwrap()
}

fn row(domains: &[Domain], mut ordinal: usize) -> Vec<i64> {
    assert!(ordinal < cardinality(domains));
    let mut result = vec![0; domains.len()];
    for (i, domain) in domains.iter().enumerate().rev() {
        let (lo, hi) = domain.bounds();
        let width = usize::try_from(i128::from(hi) - i128::from(lo) + 1).unwrap();
        result[i] = lo + (ordinal % width) as i64;
        ordinal /= width;
    }
    assert_eq!(ordinal, 0);
    result
}

fn ordinal(domains: &[Domain], input: &[i64]) -> usize {
    assert_eq!(domains.len(), input.len());
    domains.iter().zip(input).fold(0, |n, (d, x)| {
        let (lo, hi) = d.bounds();
        assert!(d.contains(*x));
        let width = usize::try_from(i128::from(hi) - i128::from(lo) + 1).unwrap();
        n * width + usize::try_from(i128::from(*x) - i128::from(lo)).unwrap()
    })
}

// Direct domain-rule oracle, independently written without IR interpretation.
fn controller_rules(x: &[i64]) -> Vec<i64> {
    let a = x[0] == 1 || x[5] == 1;
    let b = x[1] == 1 || x[6] == 1;
    let paused = x[2] > 0;
    let honored = !paused && x[7] == 1 && x[3] == 0;
    let pay = if paused || honored {
        0
    } else if a && b {
        if x[4] == 1 { 2 } else { 1 }
    } else if a {
        1
    } else if b {
        2
    } else {
        0
    };
    let pause = if paused {
        x[2] - 1
    } else if honored {
        2
    } else {
        0
    };
    let serve = if paused {
        x[2] == 1 && (a || b)
    } else {
        !honored && x[3] == 1 && pay == 0 && (a || b)
    };
    let priority = if pay == 1 {
        1
    } else if pay == 2 {
        0
    } else {
        x[4]
    };
    vec![
        pay,
        i64::from(a && pay != 1),
        i64::from(b && pay != 2),
        pause,
        i64::from(serve),
        priority,
    ]
}

// These nine outputs are the scalar ABI, not a complete publication.
fn production_rules(x: &[i64]) -> Vec<i64> {
    let [
        balance,
        lane_a,
        amount_a,
        lane_b,
        amount_b,
        pause,
        serve,
        priority,
        action,
        lane,
        amount,
        caller,
        alarm,
    ] = <[i64; 13]>::try_from(x).unwrap();
    let a = lane_a != 180;
    let b = lane_b != 180;
    let paused = pause > 0;
    let honored = pause == 0 && alarm == 1 && serve == 0;
    let pay = if paused || honored {
        0
    } else if a && b {
        if priority == 171 { 2 } else { 1 }
    } else if a {
        1
    } else if b {
        2
    } else {
        0
    };
    let sender = if action == 160 {
        190
    } else if action == 161 {
        if lane == 170 { 191 } else { 192 }
    } else {
        193
    };
    let branch = if caller != sender {
        0
    } else if action == 161 && (if lane == 170 { a } else { b }) {
        1
    } else if action == 161 && amount > balance - amount_a - amount_b {
        2
    } else if action == 160 && balance + amount > 4 {
        3
    } else if action == 160 {
        4
    } else if action == 161 && lane == 170 {
        5
    } else if action == 161 {
        6
    } else if pay == 0 {
        7
    } else if pay == 1 {
        8
    } else {
        9
    };
    let next_pause = if paused {
        pause - 1
    } else if honored {
        2
    } else {
        0
    };
    let next_serve = if paused {
        pause == 1 && (a || b)
    } else {
        !honored && serve == 1 && pay == 0 && (a || b)
    };
    let next_priority = if pay == 1 {
        171
    } else if pay == 2 {
        170
    } else {
        priority
    };
    vec![
        branch,
        balance + amount,
        if a && pay != 1 { 182 } else { 180 },
        if b && pay != 2 { 182 } else { 180 },
        next_pause,
        i64::from(next_serve),
        next_priority,
        balance - amount_a,
        balance - amount_b,
    ]
}

fn kernel(source: &Program) -> Checked<Program> {
    // Extract the actual retained controller's first three ORs. Pin the source
    // layout rather than accepting a guessed kernel after a template change.
    let wires = [0, 1, 5, 6];
    let mut ids = vec![u16::MAX; source.nodes().len()];
    let mut nodes = Vec::new();
    for (i, old) in wires.iter().enumerate() {
        assert_eq!(source.inputs()[*old], Domain::Bool);
        assert_eq!(source.nodes()[*old], Op::Input(*old as u16));
        ids[*old] = i as u16;
        nodes.push(Op::Input(i as u16));
    }
    for old in 8..20 {
        assert!(matches!(source.nodes()[old], Op::Not(_) | Op::And(_, _)));
        assert!(
            operands(&source.nodes()[old])
                .iter()
                .all(|id| ids[*id as usize] != u16::MAX)
        );
        ids[old] = nodes.len() as u16;
        nodes.push(remap(&source.nodes()[old], &ids));
    }
    Ok(Program::try_new(
        vec![Domain::Bool; 4],
        vec![Domain::Bool; 3],
        nodes,
        [11, 15, 19].iter().map(|id| ids[*id]).collect(),
    )?)
}

fn check_case(
    name: &str,
    original: &Program,
    expected: fn(&[i64]) -> Vec<i64>,
    complete: bool,
    every_budget: bool,
    directory: &Path,
) -> Checked<String> {
    let (candidate, padded, replacements) = propose(original)?;
    assert!(replacements > 0);
    assert_eq!(candidate.inputs(), original.inputs());
    assert_eq!(candidate.outputs(), original.outputs());
    assert_eq!(padded.nodes().len(), original.nodes().len());
    let count = cardinality(original.inputs());
    let rows = if complete { count } else { 1 };
    let high = original.nodes().len() as u64 + 1;
    let boundary = candidate.nodes().len() as u64;
    let budgets: Vec<u64> = if every_budget {
        (0..=high).collect()
    } else {
        vec![boundary]
    };
    let mut visited = vec![false; count];
    let mut padded_comparisons = 0;
    for i in 0..rows {
        let input = row(original.inputs(), i);
        let recovered = ordinal(original.inputs(), &input);
        assert_eq!(recovered, i);
        assert!(!visited[recovered], "duplicate input row");
        visited[recovered] = true;
        let wanted = expected(&input);
        for p in [original, &candidate, &padded] {
            let (result, usage) = observe(p, &input, high);
            assert_eq!(result, Ok(wanted.clone()), "{name} ordinal {i}: {input:?}");
            assert_eq!(usage, [0, 0, 0, 0, 0, 0, 0, p.nodes().len() as u64]);
        }
        if name == "retained-controller" {
            assert_eq!(
                retained_controller::transition(&input).map(|x| x.to_vec()),
                Some(wanted)
            );
        }
        for budget in &budgets {
            assert_eq!(
                observe(original, &input, *budget),
                observe(&padded, &input, *budget)
            );
            padded_comparisons += 1;
        }
        let (refusal, usage) = observe(original, &input, boundary);
        let Err(Failure::Budget(error)) = refusal else {
            panic!("original must refuse at reduced budget")
        };
        assert_eq!(
            (error.resource, error.limit, error.attempted, error.overflow),
            (Resource::Step, boundary, boundary + 1, false)
        );
        assert_eq!(usage[7], boundary);
        assert!(observe(&candidate, &input, boundary).0.is_ok());
    }
    if complete {
        assert!(visited.iter().all(|x| *x), "omitted input row");
    }
    let mut malformed = vec![
        vec![],
        row(original.inputs(), 0)[..original.inputs().len() - 1].to_vec(),
    ];
    let mut longer = row(original.inputs(), 0);
    longer.push(0);
    malformed.push(longer);
    for (i, domain) in original.inputs().iter().enumerate() {
        let (lo, hi) = domain.bounds();
        for value in [lo.checked_sub(1).unwrap(), hi.checked_add(1).unwrap()] {
            let mut input = row(original.inputs(), 0);
            input[i] = value;
            malformed.push(input);
        }
    }
    for input in &malformed {
        for p in [original, &candidate, &padded] {
            assert_eq!(observe(p, input, 0), (Err(Failure::InputDomain), [0; 8]));
        }
    }
    let mut sizes = Vec::new();
    for (tag, p) in [
        ("original", original),
        ("candidate", &candidate),
        ("padded", &padded),
    ] {
        let bytes = p.value()?.canonical_bytes()?;
        assert_eq!(import_program(&bytes)?, *p);
        sizes.push(bytes.len());
        fs::write(directory.join(format!("{name}-{tag}.zcve")), bytes)?;
    }
    // Correctly typed but wrong candidates must be observed as different.
    let mut wrong = candidate.nodes().to_vec();
    let i = wrong
        .iter()
        .position(|op| matches!(op, Op::Select(a, b, _) if a == b))
        .unwrap();
    let Op::Select(a, _, b) = wrong[i] else {
        unreachable!()
    };
    wrong[i] = Op::And(a, b);
    let wrong = rebuild(&candidate, wrong, candidate.roots().to_vec())?;
    let witness = (0..count)
        .find_map(|i| {
            let x = row(original.inputs(), i);
            (observe(original, &x, high).0 != observe(&wrong, &x, high).0).then_some(x)
        })
        .ok_or("wrong OR mutant survived")?;
    let mut swapped = candidate.roots().to_vec();
    swapped.swap(0, 1);
    // Kernel/controller first two roots have different kinds. Swap like-kinded ones.
    if original.outputs()[0] != original.outputs()[1] {
        swapped = candidate.roots().to_vec();
        swapped.swap(1, 2);
    }
    let swapped = rebuild(&candidate, candidate.nodes().to_vec(), swapped)?;
    assert!(
        (0..count).any(|i| {
            let x = row(original.inputs(), i);
            observe(original, &x, high).0 != observe(&swapped, &x, high).0
        }),
        "output-order mutant survived"
    );
    Ok(format!(
        "{{\"id\":\"{name}\",\"declared_rows\":{count},\"checked_rows\":{rows},\"domain_complete\":{complete},\"nodes\":[{},{},{}],\"canonical_bytes\":{sizes:?},\"or_replacements\":{replacements},\"padded_budgets\":{budgets:?},\"padded_comparisons\":{padded_comparisons},\"malformed_inputs\":{},\"resource_refinement\":\"NOT_QUALIFIED\",\"application_promotion\":false,\"wrong_or_witness\":{witness:?}}}",
        original.nodes().len(),
        candidate.nodes().len(),
        padded.nodes().len(),
        malformed.len()
    ))
}

fn controls(kernel: &Program) -> Checked<()> {
    // Structural admission rejects dead bad references/types, not just used roots.
    assert!(
        Program::try_new(
            vec![Domain::Bool],
            vec![Domain::Bool],
            vec![Op::Input(0), Op::Not(2)],
            vec![0]
        )
        .is_err()
    );
    assert!(
        Program::try_new(
            vec![],
            vec![Domain::Bool],
            vec![Op::Bool(true), Op::Int(0), Op::Not(1)],
            vec![0]
        )
        .is_err()
    );
    assert!(Program::try_new(vec![], vec![Domain::Bool], vec![Op::Int(0)], vec![0]).is_err());
    assert!(Program::try_new(vec![], vec![Domain::Bool], vec![Op::Bool(true)], vec![1]).is_err());
    // Dead and unselected checked overflow remains eager. Charge precedes trap.
    let p = Program::try_new(
        vec![],
        vec![Domain::Int { min: 0, max: 0 }],
        vec![
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Int(0),
            Op::Bool(false),
            Op::Select(4, 2, 3),
        ],
        vec![5],
    )?;
    assert_eq!(
        observe(&p, &[], 3),
        (Err(Failure::Arithmetic), [0, 0, 0, 0, 0, 0, 0, 3])
    );
    assert!(matches!(observe(&p, &[], 2).0, Err(Failure::Budget(_))));
    // Padded rewrite retains a trap's position, even when the overflowing node is dead.
    let mut nodes = kernel.nodes().to_vec();
    nodes.extend([Op::Int(i64::MAX), Op::Int(1), Op::Add(16, 17)]);
    let p = Program::try_new(
        vec![Domain::Bool; 4],
        vec![Domain::Bool; 3],
        nodes,
        vec![7, 11, 15],
    )?;
    let (_, padded, _) = propose(&p)?;
    for budget in 0..=20 {
        assert_eq!(
            observe(&p, &[0; 4], budget),
            observe(&padded, &[0; 4], budget)
        );
    }
    assert_eq!(observe(&padded, &[0; 4], 19).0, Err(Failure::Arithmetic));
    // Keep a block when its intermediate is observed outside the OR.
    let mut roots = kernel.roots().to_vec();
    roots.push(4);
    let p = Program::try_new(
        vec![Domain::Bool; 4],
        vec![Domain::Bool; 4],
        kernel.nodes().to_vec(),
        roots,
    )?;
    let (candidate, _, replacements) = propose(&p)?;
    assert_eq!(replacements, 2);
    for i in 0..16 {
        let x = row(p.inputs(), i);
        assert_eq!(observe(&p, &x, 20).0, observe(&candidate, &x, 20).0);
    }
    let internally_dependent = Program::try_new(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Not(0),
            Op::Not(1),
            Op::And(1, 2),
            Op::Not(3),
        ],
        vec![4],
    )?;
    assert_eq!(propose(&internally_dependent)?.2, 0);
    // A repeated row and an omitted row at unchanged count fail bijective coverage.
    let good: Vec<_> = (0..16).collect();
    let mut bad = good.clone();
    bad[15] = 14;
    let covered = |rows: &[usize]| {
        let mut seen = [false; 16];
        for i in rows {
            if *i >= 16 || seen[*i] {
                return false;
            }
            seen[*i] = true;
        }
        seen.iter().all(|x| *x)
    };
    assert!(covered(&good));
    assert!(!covered(&bad));
    assert!(!covered(&good[..15]));
    Ok(())
}

fn main() -> Checked<()> {
    let mut args = env::args().skip(1);
    let directory = args.next().ok_or("output directory required")?;
    let complete = match args.next().as_deref() {
        None => false,
        Some("--all-application-inputs") => true,
        _ => return Err("unknown argument".into()),
    };
    if args.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    let original = import_program(include_bytes!(
        "../../../../crates/zeno-fcis-cli/templates/withdrawal-queue/synthesized/program.zcve"
    ))?;
    assert_eq!(original.nodes().len(), 69);
    let extracted_kernel = kernel(&original)?;
    controls(&extracted_kernel)?;
    let controller = check_case(
        "retained-controller",
        &original,
        controller_rules,
        true,
        true,
        directory,
    )?;
    let kernel = check_case(
        "boolean-kernel",
        &extracted_kernel,
        |x| {
            let a = x[0] == 1 || x[2] == 1;
            let b = x[1] == 1 || x[3] == 1;
            vec![i64::from(a), i64::from(b), i64::from(a || b)]
        },
        true,
        true,
        directory,
    )?;
    let contract = production::Contract::new();
    let d = contract.descriptor();
    let original = Program::try_new(
        d.program.inputs.to_vec(),
        d.program.outputs.to_vec(),
        d.program.nodes.to_vec(),
        d.program.roots.to_vec(),
    )?;
    let application = check_case(
        "current-decision-scalars",
        &original,
        production_rules,
        complete,
        !complete,
        directory,
    )?;
    fs::write(
        directory.join("results.json"),
        format!(
            "{{\"schema\":\"zeno-fcis/withdrawal-research/1\",\"cases\":[{kernel},{controller},{application}],\"negative_controls\":\"passed\",\"runtime_source_changed\":false,\"proof_replayed\":false,\"performance_claim\":false}}\n"
        ),
    )?;
    println!("native withdrawal comparison passed; complete current scalar domain = {complete}");
    Ok(())
}
