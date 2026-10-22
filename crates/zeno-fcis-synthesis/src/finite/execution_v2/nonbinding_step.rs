//! Erased bridge from the existing eager scalar spec to the existing V2 meter.
//! Fresh usage and sufficient Step capacity preserve results and attempted work.
//! This module is registered only under verus_keep_ghost.

use super::super::evaluation::spec as scalar;
use super::super::evaluation::{Domain, Failure as ScalarFailure, Op};
use super::{Failure, Resource, spec};
use crate::resource::resource_index;
use vstd::prelude::*;

verus! {

// These helpers describe counter data and existing result/witness data only.
// They contain no instruction implementation or replacement evaluator.
spec fn zero_usage() -> Seq<u64> {
    Seq::new(8, |_: int| 0u64)
}

spec fn after_steps(count: nat) -> Seq<u64>
    recommends count <= u64::MAX,
{
    zero_usage().update(resource_index(Resource::Step) as int, count as u64)
}

spec fn lift_result(result: Result<Seq<i128>, ScalarFailure>)
    -> Result<Seq<i128>, Failure>
{
    match result {
        Ok(values) => Ok(values),
        Err(error) => Err(spec::scalar_failure(error)),
    }
}

// A witness over the actual scalar prefix/node results. It is nonrecursive.
spec fn refused_at(nodes: Seq<Op>, input: Seq<i128>, failed: nat) -> bool {
    failed < nodes.len() && match scalar::prefix(nodes, input, failed) {
        Ok(values) => scalar::node(nodes[failed as int], input, values).is_err(),
        Err(_) => false,
    }
}

proof fn zero_steps()
    ensures after_steps(0) == zero_usage(),
{
    broadcast use vstd::seq::group_seq_lemmas;
    assert(resource_index(Resource::Step) == 7);
    assert forall|i: int| 0 <= i < 8 implies
        #[trigger] after_steps(0)[i] == zero_usage()[i] by {
    }
    assert(after_steps(0) =~= zero_usage());
}

// One actual Step charge from the exact fresh-meter usage shape.
proof fn charge_step(limits: Seq<u64>, completed: nat)
    requires limits.len() == 8,
        completed < limits[resource_index(Resource::Step) as int],
    ensures spec::charge(limits, after_steps(completed), Resource::Step, 1)
        == (Ok(()), after_steps(completed + 1)),
{
    broadcast use vstd::seq::group_seq_lemmas;
    let step = resource_index(Resource::Step) as int;
    assert(step == 7);
    assert(completed + 1 <= limits[step] <= u64::MAX);
    assert(completed as u64 as int == completed as int);
    assert((completed + 1) as u64 as int == completed as int + 1);
    assert(after_steps(completed)[step] == completed as u64);
    assert forall|i: int| 0 <= i < 8 implies
        #[trigger] after_steps(completed).update(step, (completed + 1) as u64)[i]
            == after_steps(completed + 1)[i] by {
    }
    assert(after_steps(completed).update(step, (completed + 1) as u64)
        =~= after_steps(completed + 1));
    reveal(spec::charge);
}

// This induction never unfolds an instruction into another implementation.
// It compares the two actual prefix recurrences and their actual node result.
proof fn prefix_refines(nodes: Seq<Op>, input: Seq<i128>, count: nat,
    limits: Seq<u64>)
    requires count <= nodes.len(), limits.len() == 8,
        count <= limits[resource_index(Resource::Step) as int],
    ensures
        spec::prefix(nodes, input, count, limits, zero_usage()).0
            == lift_result(scalar::prefix(nodes, input, count)),
        match scalar::prefix(nodes, input, count) {
            Ok(_) => spec::prefix(nodes, input, count, limits, zero_usage()).1
                == after_steps(count),
            Err(_) => exists|failed: nat| failed < count
                && #[trigger] refused_at(nodes, input, failed)
                && spec::prefix(nodes, input, count, limits, zero_usage()).1
                    == after_steps(failed + 1),
        },
    decreases count,
{
    reveal_with_fuel(scalar::prefix, 2);
    reveal_with_fuel(spec::prefix, 2);
    if count == 0 {
        zero_steps();
    } else {
        let previous_count = (count - 1) as nat;
        prefix_refines(nodes, input, previous_count, limits);
        match scalar::prefix(nodes, input, previous_count) {
            Err(error) => {
                let failed = choose|failed: nat| failed < previous_count
                    && #[trigger] refused_at(nodes, input, failed)
                    && spec::prefix(nodes, input, previous_count, limits, zero_usage()).1
                        == after_steps(failed + 1);
                assert(spec::prefix(nodes, input, count, limits, zero_usage())
                    == spec::prefix(nodes, input, previous_count, limits, zero_usage()));
                assert(failed < count && refused_at(nodes, input, failed));
                assert(exists|f: nat| f < count && #[trigger] refused_at(nodes, input, f)
                    && spec::prefix(nodes, input, count, limits, zero_usage()).1
                        == after_steps(f + 1));
            },
            Ok(values) => {
                assert(spec::prefix(nodes, input, previous_count, limits, zero_usage())
                    == (Ok(values), after_steps(previous_count)));
                charge_step(limits, previous_count);
                match scalar::node(nodes[previous_count as int], input, values) {
                    Ok(value) => {
                        assert(spec::prefix(nodes, input, count, limits, zero_usage())
                            == (Ok(values.push(value)), after_steps(count)));
                    },
                    Err(error) => {
                        assert(refused_at(nodes, input, previous_count));
                        assert(spec::prefix(nodes, input, count, limits, zero_usage())
                            == (Err(spec::scalar_failure(error)), after_steps(count)));
                        assert(exists|failed: nat| failed < count
                            && #[trigger] refused_at(nodes, input, failed)
                            && spec::prefix(nodes, input, count, limits, zero_usage()).1
                                == after_steps(failed + 1));
                    },
                }
            },
        }
    }
}

/// Fresh-meter scalar correspondence with sufficient Step capacity.
///
/// This is the only exported theorem. It speaks about the public closed
/// execution spec already used by actual execute's contract. Malformed
/// references remain Reference; a trapping instruction includes its own Step.
pub(crate) proof fn execution_refines_scalar(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>,
    input: Seq<i128>, limits: Seq<u64>,
)
    requires limits.len() == 8,
        nodes.len() <= limits[resource_index(Resource::Step) as int],
    ensures
        match scalar::execution(inputs, outputs, nodes, roots, input) {
            Ok(values) => super::execution(inputs, outputs, nodes, roots, input, limits,
                Seq::new(8, |_: int| 0u64)).0 == Ok(values),
            Err(ScalarFailure::InputDomain) => super::execution(inputs, outputs, nodes, roots,
                input, limits, Seq::new(8, |_: int| 0u64)).0 == Err(Failure::InputDomain),
            Err(ScalarFailure::Reference) => super::execution(inputs, outputs, nodes, roots,
                input, limits, Seq::new(8, |_: int| 0u64)).0 == Err(Failure::Reference),
            Err(ScalarFailure::Arithmetic) => super::execution(inputs, outputs, nodes, roots,
                input, limits, Seq::new(8, |_: int| 0u64)).0 == Err(Failure::Arithmetic),
            Err(ScalarFailure::OutputDomain) => super::execution(inputs, outputs, nodes, roots,
                input, limits, Seq::new(8, |_: int| 0u64)).0 == Err(Failure::OutputDomain),
        },
        !scalar::admitted(inputs, input) ==>
            super::execution(inputs, outputs, nodes, roots, input, limits,
                Seq::new(8, |_: int| 0u64)).1 == Seq::new(8, |_: int| 0u64),
        scalar::admitted(inputs, input) ==> match scalar::prefix(nodes, input, nodes.len()) {
            Ok(_) => super::execution(inputs, outputs, nodes, roots, input, limits,
                Seq::new(8, |_: int| 0u64)).1 == Seq::new(8, |_: int| 0u64).update(
                    resource_index(Resource::Step) as int, nodes.len() as u64),
            Err(_) => exists|failed: nat| failed < nodes.len()
                && match #[trigger] scalar::prefix(nodes, input, failed) {
                    Ok(values) => scalar::node(nodes[failed as int], input, values).is_err(),
                    Err(_) => false,
                }
                && super::execution(inputs, outputs, nodes, roots, input, limits,
                    Seq::new(8, |_: int| 0u64)).1 == Seq::new(8, |_: int| 0u64).update(
                        resource_index(Resource::Step) as int, (failed + 1) as u64),
        },
{
    reveal(super::execution);
    reveal(spec::execution);
    reveal(scalar::execution);
    if scalar::admitted(inputs, input) {
        prefix_refines(nodes, input, nodes.len(), limits);
        match scalar::prefix(nodes, input, nodes.len()) {
            Ok(values) => {
                assert(spec::prefix(nodes, input, nodes.len(), limits, zero_usage())
                    == (Ok(values), after_steps(nodes.len())));
                match scalar::projection(values, roots, roots.len()) {
                    Err(_) => {},
                    Ok(output) => {
                        if scalar::admitted(outputs, output) {} else {}
                    },
                }
            },
            Err(error) => {
                assert(spec::prefix(nodes, input, nodes.len(), limits, zero_usage()).0
                    == Err(spec::scalar_failure(error)));
                let failed = choose|failed: nat| failed < nodes.len()
                    && #[trigger] refused_at(nodes, input, failed)
                    && spec::prefix(nodes, input, nodes.len(), limits, zero_usage()).1
                        == after_steps(failed + 1);
                assert(refused_at(nodes, input, failed));
                assert(super::execution(inputs, outputs, nodes, roots, input, limits,
                    zero_usage()).1 == after_steps(failed + 1));
                assert(exists|f: nat| f < nodes.len()
                    && match #[trigger] scalar::prefix(nodes, input, f) {
                        Ok(values) => scalar::node(nodes[f as int], input, values).is_err(),
                        Err(_) => false,
                    }
                    && super::execution(inputs, outputs, nodes, roots, input, limits,
                        zero_usage()).1 == after_steps(f + 1));
            },
        }
    }
}

}
