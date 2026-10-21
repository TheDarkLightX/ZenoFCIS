//! Exhaustive finite monitor proposals. No contract field or law is installed.
use serde_json::{Value, json};

fn words(bound: usize) -> Vec<Vec<usize>> {
    let mut all = vec![vec![]];
    let mut level = all.clone();
    for _ in 0..bound {
        let mut next = vec![];
        for word in level {
            for event in [0, 1] {
                let mut w = word.clone();
                w.push(event);
                next.push(w);
            }
        }
        all.extend(next.clone());
        level = next;
    }
    all
}

fn required(word: &[usize]) -> bool {
    word.iter().rev().take_while(|e| **e == 0).count() < 1
}

pub(crate) fn suggestions() -> Result<Value, String> {
    // 0 = a non-completing event while pending; 1 = completion/reset.
    // Output is read BEFORE the next move. Its event is consumed AFTERWARD.
    let samples = words(6);
    let contradictory = [(vec![0usize], true), (vec![0usize], false)];
    let identical_history_conflict = contradictory
        .iter()
        .any(|(a, x)| contradictory.iter().any(|(b, y)| a == b && x != y));
    if !identical_history_conflict {
        return Err("expected-identical-history-conflict".into());
    }
    let mut exclusions = vec![];
    for states in 1usize..=4 {
        let cells = states * 2;
        let tables = states.pow(u32::try_from(cells).map_err(|_| "monitor-size")?);
        let mut tested = 0u64;
        for encoding in 0..tables {
            let mut number = encoding;
            let mut transition = vec![0; cells];
            for slot in &mut transition {
                *slot = number % states;
                number /= states;
            }
            for output in 0..(1usize << states) {
                tested += 1;
                let fits = samples.iter().all(|word| {
                    let q = word.iter().fold(0, |q, e| transition[2 * q + e]);
                    ((output >> q) & 1 != 0) == required(word)
                });
                if fits {
                    // Fresh full product check: all words through K=6, not just the separating pair.
                    let product_checks = words(6)
                        .iter()
                        .map(|word| {
                            let mut q = 0;
                            let mut elapsed = 0usize;
                            for event in word {
                                if ((output >> q) & 1 != 0) != (elapsed < 1) {
                                    return false;
                                }
                                q = transition[2 * q + event];
                                elapsed = if *event == 1 { 0 } else { elapsed + 1 };
                            }
                            ((output >> q) & 1 != 0) == (elapsed < 1)
                        })
                        .collect::<Vec<_>>();
                    if product_checks.iter().any(|x| !*x) {
                        return Err("monitor-product".into());
                    }
                    return Ok(
                        json!({"status":"owner-review-proposal","adopted":false,"states":states,
                        "initial":0,"alphabet":["pending_noncompletion","request_or_completion_reset"],
                        "transition":transition,"allow_delay_outputs":(0..states).map(|q|(output>>q)&1!=0).collect::<Vec<_>>(),
                        "excluded_smaller":exclusions,"tested_at_solution_size":tested,"minimality":"all smaller total deterministic machines exhausted under this fixed alphabet and bound",
                        "observed_alarm_pair":{"histories":[["alarm","request"],["request","alarm"]],"current_alarm":[true,true],"next_request":"alarm","required":[true,false]},
                        "identical_history_conflict":{"history":["alarm"],"labels":[true,false],"status":"unsatisfiable-for-this-observation-alphabet"},
                        "product_words_checked":product_checks.len(),"K":6,
                        "meaning":"While an exit is pending, alarms do not replenish its two-event deadline. Completion resets the monitor.",
                        "zal_t_reading":"always(submitted -> within_2(completed)); strong finite endpoints; a late request is open, never completed by truncation",
                        "lowering":"inert reading only; ZAL-T patch absent from baseline",
                        "initialization_migration":"initialize on request; reset only on completion; owner must review migration before any adoption",
                        "state_cost_bits":1,"guard_timing":"preceding prefix and current request only; resulting event updates afterward"}),
                    );
                }
            }
        }
        exclusions.push(json!({"states":states,"total_machines":tested,"complete":true}));
    }
    Err("no-monitor-with-at-most-four-states".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_two_state_monitor() {
        let result = super::suggestions().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(result["states"], 2);
        assert_eq!(result["adopted"], false);
        assert_eq!(result["excluded_smaller"][0]["total_machines"], 2);
    }
}
