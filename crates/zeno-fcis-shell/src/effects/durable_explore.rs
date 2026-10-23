//! The process-loss extension of the simulation explorer. Uses production
//! admission, restore and write ordering, and the existing Simulation terminal.
use super::super::{
    handler::Handling,
    port::Code,
    registry::Port,
    simulation::{self, Durable, Profile, Script, Simulation, World},
};
use super::tests::{Memory, binding, manifest, ok, operation, policies};
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

// Ten cuts: no loss; before/after reserve commit; before invocation; after
// invocation/before answer; after answer/before release commit; after release;
// returned reserve error; returned release error; ambiguous reserve error.
const CUTS: u8 = 10;

struct Process<'a> {
    simulation: &'a mut Simulation,
    cut: u8,
    entered: bool,
}
impl Terminal for Process<'_> {
    fn ports(&self) -> &'static [Port] {
        &Port::ALL
    }
    fn handle(&mut self, b: &Binding, op: &Operation) -> Handled {
        self.handle_with_invocation(b, op).outcome
    }
    fn handle_with_invocation(&mut self, b: &Binding, op: &Operation) -> Handling {
        if self.cut == 3 {
            panic!("process loss before invocation");
        }
        self.entered = true;
        let report = self.simulation.handle_with_invocation(b, op);
        if self.cut == 4 {
            panic!("process loss after invocation before answer");
        }
        report
    }
}

#[test]
fn process_loss_explorer() {
    // Exact enumeration of all prefixes through two attempts: two keys, ten
    // cuts and eight terminal choices (seven Send outcomes plus NotInvoked).
    // Every node discards the handle; only bytes survive to its child.
    let mut frontier = vec![(
        Memory::new().0.borrow().bytes.clone(),
        World::new(Durable::genesis()),
        [false; 2],
        [0u8; 2],
        0usize,
    )];
    let mut edges = 0usize;
    let mut leaves = 0usize;
    let mut releases = 0usize;
    let mut effects = 0usize;
    let mut census = Vec::new();
    while let Some((bytes, world, expected_reserved, attempts, depth)) = frontier.pop() {
        if depth == 2 {
            leaves += 1;
            continue;
        }
        for key in 0..2 {
            for cut in 0..CUTS {
                for choice in 0..8 {
                    let disk = Memory::new();
                    disk.0.borrow_mut().bytes = bytes.clone();
                    disk.0.borrow_mut().stop = match cut {
                        1 => Some((1, 0)),
                        2 => Some((1, 1)),
                        5 => Some((2, 0)),
                        6 => Some((2, 1)),
                        7 => Some((1, 2)),
                        8 => Some((2, 2)),
                        9 => Some((1, 3)),
                        _ => None,
                    };
                    let mut relay = ok(Relay::open(
                        disk.clone(),
                        simulation::STORE,
                        &policies(),
                        manifest(),
                    ));
                    let script = if choice == 7 { vec![] } else { vec![choice] };
                    let mut simulation = Simulation::new(
                        world.clone(),
                        Profile::frozen(0),
                        &Port::ALL,
                        Script::new(script),
                    );
                    // Receiver drains between attempts; duplication remains
                    // visible in effects, while protocol admission is separate.
                    let op = operation(key as u8 + 1);
                    let mut terminal = Process {
                        simulation: &mut simulation,
                        cut,
                        entered: false,
                    };
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        relay.handle(&mut terminal, &binding(&op), &op)
                    }));
                    let entered = terminal.entered;
                    let mut counts = attempts;
                    if entered && choice != 7 {
                        counts[key] += 1;
                    }
                    assert!(
                        counts[key] <= 1,
                        "process_loss_explorer: second protocol invocation, depth={depth} key={key} cut={cut} choice={choice}"
                    );
                    let stored = disk.0.borrow().bytes.clone();
                    let restored = ok(restore(simulation::STORE, &policies(), &stored));
                    let mut expected = expected_reserved;
                    // Independent Boolean reference, no policy/restore helper.
                    if !expected[key] && cut != 1 && cut != 7 {
                        expected[key] = true;
                        if choice == 7 && ![2, 3, 4, 5, 8, 9].contains(&cut) {
                            expected[key] = false;
                            releases += 1;
                        }
                    }
                    let mut actual = [false; 2];
                    for (k, expected_key) in expected.iter().enumerate() {
                        let refused = policy_record::admit(&restored, &operation(k as u8 + 1))
                            .1
                            .rejection()
                            == Some(Code::PolicyDuplicate);
                        actual[k] = refused;
                        assert_eq!(
                            refused, *expected_key,
                            "reference mismatch depth={depth} key={key} cut={cut} choice={choice}"
                        );
                    }
                    if expected_reserved[key] {
                        assert!(matches!(
                            result,
                            Ok(Ok(Handled::Answer(Response::Send(Sent::Refused(
                                Code::PolicyDuplicate
                            )))))
                        ));
                    }
                    while !simulation.world.queue.is_empty() {
                        simulation.receive(0);
                    }
                    let received = &simulation.world.effects;
                    for id in received {
                        assert_eq!(
                            received.iter().filter(|other| *other == id).count(),
                            1,
                            "process_loss_explorer: duplicate receiver effect"
                        );
                    }
                    census.extend_from_slice(&[
                        depth as u8,
                        key as u8,
                        cut,
                        choice,
                        u8::from(actual[0]),
                        u8::from(actual[1]),
                        counts[0],
                        counts[1],
                        u8::from(received.contains(&Hash32::new([1; 32]))),
                        u8::from(received.contains(&Hash32::new([2; 32]))),
                    ]);
                    effects += received.len();
                    edges += 1;
                    frontier.push((stored, simulation.world, expected, counts, depth + 1));
                }
            }
        }
    }
    assert_eq!(edges, 25_760);
    assert_eq!(leaves, 25_600);
    assert!(
        releases > 0 && effects > 0,
        "non-vacuity: no releases or receiver effects"
    );
    std::println!(
        "durable explorer: bound=2, keys=2, cuts=10, outcomes=8, edges={edges}, leaves={leaves}, certified_releases={releases}, receiver_effect_observations={effects}, complete=true, census={}",
        RustCryptoSha256::hash(&census)
    );
}
