"""G10 negative controls. These are deliberate faults in isolated specimens only."""
from pathlib import Path
import re

from check_finite_execution import once

CLI = Path('crates/zeno-fcis-cli/src')
CORE = Path('crates/zeno-fcis-shell-sqlite/src/v2/equivalence')
RANGES = CORE / 'checker_ranges.rs'
LEDGER = CLI / 'neural_loop/ledger_accounting.rs'
MEMBERSHIP = CLI / 'neural_loop/ledger_membership.rs'
PLAN = CLI / 'transform_plan.rs'
SCHEDULER = CLI / 'transform_parallel.rs'
SESSION = CLI / 'neural_loop/session.rs'
LOOP = CLI / 'loop_command.rs'
TRANSFORM = CLI / 'transform.rs'

SEMANTIC_TARGETS = {
    'R1_wrong_matched': 'scan_range', 'R2_wrong_evaluated': 'scan_range',
    'R3_undercharge_range': 'scan_range', 'R4_later_witness': 'merge_adjacent',
    'R5_maximum_over_limit': 'merge_adjacent', 'R6_missing_zero_origin': 'finish_range',
    'R7_missing_full_coverage': 'finish_range', 'R8_wrong_unrank_width': 'tuple_at',
    'L1_free_preparation': 'accounting', 'L2_wrapping_work': 'accounting',
    'L3_unsettled_completion': 'accounting', 'L4_missing_membership': 'unresolved_attempts',
}
COVERAGE_TARGETS = {
    'range_weaken_projection': ('checker::checker::ranges::compare_two_ranges', 'ensures_sha256'),
    'range_narrow_domain': ('checker::checker::ranges::compare_two_ranges', 'requires_sha256'),
}
PLAN_TEST = 'transform_plan::tests::exact_reservation_one_short_and_total_partition'
NATIVE_TARGETS = {
    'P1_floor_partition': ('--bin', 'zeno-fcis', PLAN_TEST),
    'P2_gap_partition': ('--bin', 'zeno-fcis', PLAN_TEST),
    'P3_reverse_receivers': ('--bin', 'zeno-fcis', 'transform_parallel::tests::delayed_first_middle_and_last_slots_preserve_least_witness_and_all_work'),
    'P4_ignore_first_window_witness': ('--bin', 'zeno-fcis', 'transform_parallel::tests::least_witness_in_the_first_window_stops_dispatch_after_that_window'),
    'P5_ignore_worker_failure': ('--bin', 'zeno-fcis', 'transform_parallel::tests::a_failed_worker_is_inconclusive_with_incomplete_accounting'),
    'P6_ignore_final_deadline': ('--bin', 'zeno-fcis', 'transform_parallel::tests::a_deadline_that_passes_while_joining_workers_is_inconclusive'),
    'L5_overdraw_preparation': ('--bin', 'zeno-fcis', 'loop_command::tests::intake::intake_exact_and_one_short_precede_reads_and_never_refund'),
    'L6_reserve_closed_session': ('--test', 'loop_cli', 'the_local_proposer_runs_an_encoded_benchmark_case_to_exhaustion'),
    'L7_free_services': ('--bin', 'zeno-fcis', PLAN_TEST),
}


def mutations(root):
    controls = {}
    def change(name, path, before, after, kind='proof'):
        controls[name] = (path, once((root / path).read_text(), before, after), kind)
    change('R1_wrong_matched', RANGES, 'matched: ordinal,', 'matched: ordinal + 1,')
    change('R2_wrong_evaluated', RANGES, 'evaluated: ordinal - lo + 1,', 'evaluated: ordinal - lo,')
    # The whole-unit composition has a second cost expression: mutate only scan_range.
    change('R3_undercharge_range', RANGES,
           'let cost = original.nodes.len() as u128 + candidate.nodes.len() as u128 + 2;\n    let required = match ((hi - lo)',
           'let cost = original.nodes.len() as u128 + candidate.nodes.len() as u128 + 1;\n    let required = match ((hi - lo)')
    change('R4_later_witness', RANGES, 'let hi = right.hi; if left.witness.is_some() {', 'let hi = right.hi; if right.witness.is_some() {')
    change('R5_maximum_over_limit', RANGES,
           'left.tally.over_limit[0] + right.tally.over_limit[0],',
           'left.tally.over_limit[0].max(right.tally.over_limit[0]),')
    change('R6_missing_zero_origin', RANGES,
           '    if report.lo != 0 {\n        return Err(RangeError::IncompleteCoverage);\n    }\n', '')
    change('R7_missing_full_coverage', RANGES,
           '    if report.hi != size {\n        return Err(RangeError::IncompleteCoverage);\n    }\n', '')
    change('R8_wrong_unrank_width', RANGES,
           'let maybe_width = interval_width(min, max);',
           'let maybe_width = match interval_width(min, max) { Some(width) => Some(width - 1), None => None };')
    contract = 'ensures two_ranges_result(original, candidate, cap, step_limit, split, reserved_work, result),'
    change('range_weaken_projection', RANGES, contract, contract[:-1] + ' || true,', 'coverage')
    change('range_narrow_domain', RANGES, contract, 'requires false, ' + contract, 'coverage')
    preparation = '''Stage::Preparation { work } => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 1; }
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }'''
    change('L1_free_preparation', LEDGER, preparation,
           preparation.replace('totals.work_reserved = totals.work_reserved.saturating_add(*work);', 'let _ = work;'))
    check_work = 'totals.checks = totals.checks.saturating_add(1);\n                        totals.work_reserved = totals.work_reserved.saturating_add(*work);'
    change('L2_wrapping_work', LEDGER, check_work, check_work.replace('saturating_add(*work)', 'wrapping_add(*work)'))
    # Keep type inference intact so refusal must be semantic, never a Rust error.
    change('L3_unsettled_completion', LEDGER, 'settled.push(*attempt);', 'if false { settled.push(*attempt); }')
    change('L4_missing_membership', MEMBERSHIP, 'present[settled[index] as usize] = true;', 'present[settled[index] as usize] = false;')
    change('P1_floor_partition', PLAN, 'd.tuples.div_ceil(limits.chunk_tuples)', '(d.tuples / limits.chunk_tuples)', 'native')
    change('P2_gap_partition', PLAN, 'lo + self.limits.chunk_tuples)', 'lo + self.limits.chunk_tuples - 1)', 'native')
    change('P3_reverse_receivers', SCHEDULER, 'receivers.iter().take(dispatched).enumerate()', 'receivers.iter().rev().take(dispatched).enumerate()', 'native')
    change('P4_ignore_first_window_witness', SCHEDULER,
           '                if matched < hi {\n                    break;\n                }',
           '                let _ = (matched, hi);', 'native')
    change('P5_ignore_worker_failure', SCHEDULER,
           '                        stop.get_or_insert(Stop::Worker);\n                        continue;',
           '                        continue;', 'native')
    source = (root / SCHEDULER).read_text()
    for before in (
        '        if Instant::now() >= deadline {\n            return Err(Failure::Stopped(Stop::Deadline));\n        }\n',
        '        if Instant::now() >= deadline {\n            stop.get_or_insert(Stop::Deadline);\n        }\n',
    ):
        source = once(source, before, '')
    controls['P6_ignore_final_deadline'] = (SCHEDULER, source, 'native')
    change('L5_overdraw_preparation', SESSION, 'if work > remaining {', 'if work > remaining.saturating_add(1) {', 'native')
    source = (root / LOOP).read_text()
    start = source.index('    if let Status::Closed(reason) = session.status() {', source.index('fn candidate_command_acquired('))
    end = source.index('    // Prepay bounded acquisition', start)
    closed = source[start:end]
    source = once(source, closed, '')
    anchor = '    let bytes = match (candidate, candidate_json) {'
    source = once(source, anchor, closed + anchor)
    controls['L6_reserve_closed_session'] = (LOOP, source, 'native')
    change('L7_free_services', PLAN, '        + services\n', '        + 0 * services\n', 'native')
    source = (root / TRANSFORM).read_text()
    pattern = r'let complete = crate::finite_checker::finish_range\([\s\S]*?\}\)\?;'
    matches = list(re.finditer(pattern, source))
    if len(matches) != 1:
        raise ValueError('PreparedCheck::finish mutation anchor changed')
    match = matches[0]
    replacement = 'let complete = (self.pair.size, crate::finite_checker::Usage { max_steps: [0, 0], candidate_uses_more: 0, usage_differs: false });'
    controls['route_handbuilt_completion'] = (TRANSFORM, source[:match.start()] + replacement + source[match.end():], 'route')
    return controls
