//! Composition regressions at the retained policy/terminal boundary.

use alloc::vec;
use zeno_fcis_codec::Hash32;

use super::driver::Binding;
use super::handler::{Fault, Handled, Invocation, Layer, Stack, Terminal};
use super::laws;
use super::policy::{Monitor, Policy};
use super::port::{Code, OpId, Operation, Response, Sent};
use super::simulation::{self, Durable, Profile, Script, Simulation, World};

#[test]
fn policy_composed_suspend_retains_forwarded_reservation() {
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let operation = Operation::Send(simulation::store(1), durable.outbox[0].0.clone());
    let binding = Binding::of(OpId::first(1), &operation);
    let mut simulation = Simulation::new(
        World::new(durable),
        Profile::frozen(0),
        simulation::ALL_PORTS,
        Script::new(vec![0]),
    );
    let mut stack = Stack {
        layers: vec![Layer::Policy(Monitor::new(Policy::AtMostOne {
            capacity: 2,
        }))],
    };
    let initial = stack.policy_bytes();
    let mut script = Script::new(vec![6]);
    assert_eq!(
        stack.handle(
            &mut Fault::new(&mut simulation, &mut script),
            &binding,
            &operation,
        ),
        Handled::Suspend
    );
    assert_eq!(simulation.world.queue.len(), 1, "first send was forwarded");
    let erased = stack.policy_bytes() == initial;
    simulation.script = Script::new(vec![0]);
    let mut retry = Script::new(vec![0]);
    let second = stack.handle(
        &mut Fault::new(&mut simulation, &mut retry),
        &binding,
        &operation,
    );
    assert_eq!(
        second,
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate))),
        "composed Suspend erased a forwarded reservation: erased={erased}, copies={}",
        simulation.world.queue.len()
    );
    assert!(!erased);
    assert_eq!(simulation.world.queue.len(), 1);
}

#[test]
fn policy_fault_invocation_reports_match_world() {
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let operation = Operation::Send(simulation::store(1), durable.outbox[0].0.clone());
    let binding = Binding::of(OpId::first(1), &operation);
    let mut prefixes = vec![vec![]];
    for a in 0u8..7 {
        prefixes.push(vec![a]);
        for b in 0u8..7 {
            prefixes.push(vec![a, b]);
        }
    }
    let mut checked = 0;
    let mut partial_forwards = 0;
    for outer in 0u8..8 {
        for prefix in &prefixes {
            let mut simulation = Simulation::new(
                World::new(durable.clone()),
                Profile::frozen(0),
                simulation::ALL_PORTS,
                Script::new(prefix.clone()),
            );
            let mut actual = simulation.clone();
            let before = simulation.world.clone();
            let choices = if outer == 7 { vec![] } else { vec![outer] };
            let mut script = Script::new(choices.clone());
            let report = Fault::new(&mut simulation, &mut script)
                .handle_with_invocation(&binding, &operation);
            let rollback =
                report.outcome == Handled::Suspend && report.invocation == Invocation::NotInvoked;
            if rollback {
                assert_eq!(simulation.world, before, "false non-invocation report");
            }
            if report.outcome == Handled::Suspend && !simulation.world.queue.is_empty() {
                partial_forwards += 1;
                assert_eq!(report.invocation, Invocation::MayHaveInvoked);
            }
            let mut stack = Stack {
                layers: vec![Layer::Policy(Monitor::new(Policy::AtMostOne {
                    capacity: 2,
                }))],
            };
            let initial = stack.policy_bytes();
            let mut script = Script::new(choices);
            assert_eq!(
                stack.handle(
                    &mut Fault::new(&mut actual, &mut script),
                    &binding,
                    &operation,
                ),
                report.outcome
            );
            assert_eq!(actual.world, simulation.world);
            assert_eq!(stack.policy_bytes() == initial, rollback);
            actual.script = Script::new(vec![0]);
            let mut retry = Script::new(vec![0]);
            let result = stack.handle(
                &mut Fault::new(&mut actual, &mut retry),
                &binding,
                &operation,
            );
            if rollback {
                assert!(matches!(
                    result,
                    Handled::Answer(Response::Send(Sent::Sent(_)))
                ));
            } else {
                assert_eq!(
                    result,
                    Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
                );
                assert_eq!(actual.world, simulation.world);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 456);
    assert!(partial_forwards > 0);
}

#[test]
fn policy_nested_suspend_preserves_invocation() {
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let operation = Operation::Send(simulation::store(1), durable.outbox[0].0.clone());
    let binding = Binding::of(OpId::first(1), &operation);
    let mut simulation = Simulation::new(
        World::new(durable),
        Profile::frozen(0),
        simulation::ALL_PORTS,
        Script::new(vec![0]),
    );
    let mut outer = Script::new(vec![6]);
    let mut inner = Script::new(vec![0]);
    let report = Fault::new(&mut Fault::new(&mut simulation, &mut inner), &mut outer)
        .handle_with_invocation(&binding, &operation);
    assert_eq!(report.outcome, Handled::Suspend);
    assert_eq!(report.invocation, Invocation::MayHaveInvoked);
    assert_eq!(simulation.world.queue.len(), 1);
}

#[test]
fn policy_unknown_suspend_keeps_reservation() {
    struct Unqualified(usize);
    impl Terminal for Unqualified {
        fn ports(&self) -> &'static [super::registry::Port] {
            simulation::ALL_PORTS
        }
        fn handle(&mut self, _: &Binding, _: &Operation) -> Handled {
            self.0 += 1;
            Handled::Suspend
        }
    }
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let operation = Operation::Send(simulation::store(1), durable.outbox[0].0.clone());
    let binding = Binding::of(OpId::first(1), &operation);
    let mut stack = Stack {
        layers: vec![Layer::Policy(Monitor::new(Policy::AtMostOne {
            capacity: 2,
        }))],
    };
    let mut terminal = Unqualified(0);
    assert_eq!(
        stack.handle(&mut terminal, &binding, &operation),
        Handled::Suspend
    );
    assert_eq!(
        stack.handle(&mut terminal, &binding, &operation),
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
    );
    assert_eq!(terminal.0, 1);
}
