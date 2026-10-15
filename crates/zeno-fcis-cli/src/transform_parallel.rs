//! Bounded imperative scheduling over the shared verified range evaluator.
//! Scoped joins prevent detached work, but only process containment can enforce
//! elapsed-time/memory limits against a stalled allocator, worker or kernel.

use crate::finite_checker::{RangeReport, merge_adjacent, scan_range};
use crate::transform_plan::{RunPlan, Stop};
use std::sync::mpsc;
use std::time::Instant;
use zeno_fcis_synthesis::finite::V2ScalarProgram;

#[derive(Debug)]
pub(crate) enum Failure {
    Semantic(crate::transform::Rejection),
    Stopped(Stop),
}

pub(crate) struct Outcome {
    pub(crate) result: Result<crate::transform::Equivalence, Failure>,
    pub(crate) progress: Progress,
}

/// The clock-bearing shell retains the reservation outside consuming evidence.
/// `started` is taken before the containing stage is requested, so this
/// deadline never falls after the supervisor's stage deadline.
pub(crate) fn check(
    original: &[u8],
    candidate: &[u8],
    semantic: crate::transform::Limits,
    limits: crate::transform_plan::RunLimits,
    started: Instant,
) -> Outcome {
    use crate::transform_plan::{plan, setup_work};
    let mut progress = Progress {
        accounting_complete: true,
        ..Progress::default()
    };
    let result = (|| {
        limits.validate().map_err(Failure::Stopped)?;
        let setup = setup_work([original.len() as u64, candidate.len() as u64])
            .map_err(Failure::Stopped)?;
        if setup > limits.work_units {
            return Err(Failure::Stopped(Stop::Work));
        }
        progress.reserved = setup;
        progress.setup_reserved = setup;
        let prepared =
            crate::transform::prepare(original, candidate, semantic).map_err(Failure::Semantic)?;
        let plan = plan(prepared.dimensions(), limits).map_err(Failure::Stopped)?;
        progress.reserved = plan.reserved();
        let deadline = started + std::time::Duration::from_millis(limits.deadline_ms);
        let (a, b) = prepared.subjects();
        let scheduled = run(
            &a,
            &b,
            semantic.input_tuples,
            semantic.steps,
            &plan,
            deadline,
        );
        progress = scheduled.progress;
        let report = scheduled.result.map_err(Failure::Stopped)?;
        let result = prepared.finish(report).map_err(|error| match error {
            crate::transform::RangeFinishError::Semantic(error) => Failure::Semantic(error),
            crate::transform::RangeFinishError::Coverage => Failure::Stopped(Stop::Coverage),
        });
        if Instant::now() >= deadline {
            return Err(Failure::Stopped(Stop::Deadline));
        }
        result
    })();
    Outcome { result, progress }
}

#[derive(Debug, Default)]
pub(crate) struct Progress {
    pub(crate) reserved: u64,
    pub(crate) setup_reserved: u64,
    pub(crate) evaluated: u64,
    pub(crate) equal_prefix: u64,
    pub(crate) measured_steps: [u128; 2],
    pub(crate) completed_chunks: u64,
    pub(crate) planned_chunks: u64,
    pub(crate) active_workers: u64,
    pub(crate) accounting_complete: bool,
}

/// Diagnostics survive every consuming merge/finalization and operational stop.
pub(crate) struct Scheduled<'a, 'p> {
    pub(crate) result: Result<RangeReport<'a, 'p>, Stop>,
    pub(crate) progress: Progress,
}

pub(crate) fn run<'a, 'p>(
    a: &'a V2ScalarProgram<'p>,
    b: &'a V2ScalarProgram<'p>,
    cap: u64,
    step_limit: u64,
    plan: &RunPlan,
    deadline: Instant,
) -> Scheduled<'a, 'p> {
    run_ordered(
        a,
        b,
        cap,
        step_limit,
        plan,
        deadline,
        #[cfg(test)]
        Faults::default(),
    )
}

/// Test-only scheduling faults: per-slot task delays, one failing slot and a
/// delay before each worker exits after its queue closes.
#[cfg(test)]
#[derive(Clone, Copy, Default)]
struct Faults<'d> {
    delays: &'d [u64],
    failing_slot: Option<usize>,
    exit_delay_ms: u64,
}

fn run_ordered<'a, 'p>(
    a: &'a V2ScalarProgram<'p>,
    b: &'a V2ScalarProgram<'p>,
    cap: u64,
    step_limit: u64,
    plan: &RunPlan,
    deadline: Instant,
    #[cfg(test)] faults: Faults<'_>,
) -> Scheduled<'a, 'p> {
    let mut progress = Progress {
        reserved: plan.reserved(),
        setup_reserved: plan.setup(),
        planned_chunks: plan.chunks(),
        accounting_complete: true,
        ..Progress::default()
    };
    let d = plan.dimensions();
    // Never silently run a plan for a different shape/tuple count. Actual
    // structural subjects are checked by every scan and finalization.
    if a.inputs.len() as u64 != d.inputs
        || b.inputs.len() as u64 != d.inputs
        || a.outputs.len() as u64 != d.outputs
        || b.outputs.len() as u64 != d.outputs
        || a.nodes.len() as u64 != d.nodes[0]
        || b.nodes.len() as u64 != d.nodes[1]
        || crate::finite_checker::validate_pair(a, b, cap) != Ok(d.tuples)
    {
        return Scheduled {
            result: Err(Stop::Dimensions),
            progress,
        };
    }
    if Instant::now() >= deadline {
        return Scheduled {
            result: Err(Stop::Deadline),
            progress,
        };
    }
    let result = std::thread::scope(|scope| {
        let count = plan.limits().workers.min(plan.chunks()) as usize;
        let mut senders = Vec::with_capacity(count);
        let mut receivers = Vec::with_capacity(count);
        let mut handles = Vec::with_capacity(count);
        let mut stop = None;
        for slot in 0..count {
            #[cfg(test)]
            let delay = faults.delays.get(slot).copied().unwrap_or(0);
            #[cfg(test)]
            let failing = faults.failing_slot == Some(slot);
            let (task_tx, task_rx) = mpsc::sync_channel::<(u64, u64, u64)>(1);
            let (report_tx, report_rx) = mpsc::sync_channel(1);
            let spawn = std::thread::Builder::new()
                .name(format!("finite-range-{slot}"))
                .stack_size(256 * 1024)
                .spawn_scoped(scope, move || {
                    while let Ok((lo, hi, reservation)) = task_rx.recv() {
                        #[cfg(test)]
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                        let report = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            #[cfg(test)]
                            if failing {
                                panic!("planted worker failure");
                            }
                            scan_range(a, b, cap, step_limit, lo, hi, reservation)
                        }));
                        if report_tx.send(report).is_err() {
                            break;
                        }
                    }
                    #[cfg(test)]
                    std::thread::sleep(std::time::Duration::from_millis(faults.exit_delay_ms));
                });
            match spawn {
                Ok(handle) => {
                    senders.push(task_tx);
                    receivers.push(report_rx);
                    handles.push(handle);
                    progress.active_workers += 1;
                }
                Err(_) => {
                    stop = Some(Stop::Spawn);
                    break;
                }
            }
        }
        let mut next = 0;
        let mut aggregate: Option<RangeReport<'a, 'p>> = None;
        while next < plan.chunks() && stop.is_none() {
            if Instant::now() >= deadline {
                stop = Some(Stop::Deadline);
                break;
            }
            let window = (plan.chunks() - next).min(count as u64) as usize;
            let mut dispatched = 0;
            for sender in senders.iter().take(window) {
                let Some(interval) = plan.interval(next + dispatched as u64) else {
                    stop = Some(Stop::Coverage);
                    break;
                };
                if sender.send(interval).is_err() {
                    stop = Some(Stop::Worker);
                    progress.accounting_complete = false;
                    break;
                }
                dispatched += 1;
            }
            // Each task owns its own result slot. Receive in ordinal order;
            // later arrival cannot overtake an earlier witness/refusal.
            for (slot, receiver) in receivers.iter().take(dispatched).enumerate() {
                let received = match receiver
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                {
                    Ok(report) => Ok(report),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        stop.get_or_insert(Stop::Deadline);
                        // Drain and join for honest work accounting. This may
                        // block: the enclosing process watchdog must kill it.
                        receiver.recv()
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::RecvError),
                };
                let report = match received {
                    Ok(Ok(Ok(report))) => report,
                    _ => {
                        progress.accounting_complete = false;
                        stop.get_or_insert(Stop::Worker);
                        continue;
                    }
                };
                let (lo, hi, _, evaluated, _) = report.progress();
                progress.evaluated += evaluated;
                progress.completed_chunks += 1;
                let steps = report.measured_steps();
                progress.measured_steps[0] += steps[0];
                progress.measured_steps[1] += steps[1];
                if plan.interval(next + slot as u64).map(|(l, h, _)| (l, h)) != Some((lo, hi)) {
                    stop.get_or_insert(Stop::Coverage);
                    continue;
                }
                if stop.is_some() {
                    continue;
                }
                let merged = match aggregate.take() {
                    None => Ok(report),
                    Some(left) => merge_adjacent(left, report),
                };
                match merged {
                    Ok(report) => {
                        progress.equal_prefix = report.progress().2;
                        aggregate = Some(report);
                    }
                    Err(_) => {
                        stop = Some(Stop::Coverage);
                    }
                }
            }
            next += dispatched as u64;
            if let Some(report) = &aggregate {
                let (_, hi, matched, _, _) = report.progress();
                if matched < hi {
                    break;
                }
            }
        }
        // Closing all task queues wakes idle workers. Every owned worker is
        // joined on every return path; never discard a JoinHandle.
        drop(senders);
        for handle in handles {
            if handle.join().is_err() {
                progress.accounting_complete = false;
                stop.get_or_insert(Stop::Worker);
            }
        }
        if Instant::now() >= deadline {
            stop.get_or_insert(Stop::Deadline);
        }
        match stop {
            Some(error) => Err(error),
            None => aggregate.ok_or(Stop::Coverage),
        }
    });
    let result = if measured_within_reservation(progress.measured_steps, progress.reserved) {
        result
    } else {
        progress.accounting_complete = false;
        Err(Stop::Accounting)
    };
    Scheduled { result, progress }
}

/// Measured evaluator Steps are a sub-account of the admission reservation.
/// This does not establish a bound for all uninstrumented shell services.
fn measured_within_reservation(steps: [u128; 2], reserved: u64) -> bool {
    steps[0]
        .checked_add(steps[1])
        .is_some_and(|total| total <= u128::from(reserved))
}

#[cfg(test)]
#[path = "transform_parallel_tests.rs"]
mod tests;
