'use strict';
// Separately stated safety GOALS may be entailed by independently checked
// candidate invariants. This checker never allows the proposer to pick goals.
const { verify, MAX } = require('./proof_checker.cjs');
const SCHEMA = 'zeno-fcis/local-goals/experimental-1';
function need(x, reason) { if (!x) throw new Error(reason); }
function exact(obj, fields, where) {
  need(obj !== null && typeof obj === 'object' && !Array.isArray(obj), where + ': object required');
  need(JSON.stringify(Object.keys(obj).sort()) === JSON.stringify(fields.slice().sort()),
    where + ': wrong/missing/extra keys');
}
function names(array, byName, where, cap) {
  need(Array.isArray(array) && array.length > 0 && array.length <= cap, where + ': invalid size');
  for (let i = 0; i < array.length; i++) {
    const n = array[i];
    need(typeof n === 'string' && byName.has(n), where + ': unknown name');
    need(i === 0 || array[i - 1] < n, where + ': not strictly sorted');
  }
  return array;
}
function cardinality(support, variables) {
  return support.reduce((z, n) => z * BigInt(variables.get(n).domain.length), 1n);
}
function enumerate(support, variables, fn) {
  const domains = support.map(n => variables.get(n).domain);
  const pos = Array(domains.length).fill(0);
  while (true) {
    fn(pos.map((x, i) => domains[i][x]));
    let k = pos.length - 1;
    while (k >= 0) {
      pos[k]++;
      if (pos[k] < domains[k].length) break;
      pos[k--] = 0;
    }
    if (k < 0) break;
  }
}
function rowsToSet(rows, support, vars, where) {
  need(Array.isArray(rows) && rows.length > 0 && rows.length <= MAX.allowedRows, where + ': invalid allowed rows');
  const result = new Set();
  for (const row of rows) {
    need(Array.isArray(row) && row.length === support.length, where + ': tuple shape');
    for (let i = 0; i < row.length; i++) {
      need(vars.get(support[i]).domain.includes(row[i]), where + ': value outside domain');
    }
    const k = JSON.stringify(row);
    need(!result.has(k), where + ': duplicate tuple');
    result.add(k);
  }
  return result;
}
function checkGoals(model, goalPacket, maxRows = MAX.obligationRows, maxTotal = MAX.totalObligationRows) {
  need(Number.isSafeInteger(maxRows) && maxRows >= 1 && maxRows <= MAX.obligationRows, 'invalid goal row limit');
  need(Number.isSafeInteger(maxTotal) && maxTotal >= 1 && maxTotal <= MAX.totalObligationRows, 'invalid total goal limit');
  const base = verify(model); // check actual induction before trusting candidate premises
  if (base.status !== 'proved') return {
    schema: 'zeno-fcis/local-goal-result/experimental-1', status: base.status,
    reason: 'invariant-proof-blocked', model_result: base,
    goal_safety_established: false, publication_authority: false
  };
  exact(goalPacket, ['schema', 'goals'], 'goals');
  need(goalPacket.schema === SCHEMA, 'unsupported goal schema');
  need(Array.isArray(goalPacket.goals) && goalPacket.goals.length > 0 && goalPacket.goals.length <= MAX.invariants,
    'goals: invalid count');
  const vars = new Map(model.variables.map(x => [x.id, x]));
  const invariants = new Map(model.invariants.map(x => [x.id, x]));
  const knownGoalIds = new Set(), jobs = [];
  let planned = 0n;
  for (const goal of goalPacket.goals) {
    exact(goal, ['id', 'support', 'allowed', 'using'], 'goal');
    need(typeof goal.id === 'string' && /^[a-z][a-z0-9_]{0,62}$/.test(goal.id), 'goal identifier');
    need(!knownGoalIds.has(goal.id), 'duplicate goal');
    knownGoalIds.add(goal.id);
    names(goal.support, vars, goal.id + '.support', MAX.variables);
    names(goal.using, invariants, goal.id + '.using', MAX.invariants);
    const allowed = rowsToSet(goal.allowed, goal.support, vars, goal.id);
    const premises = goal.using.map(n => {
      const p = invariants.get(n);
      return { support: p.support, allowed: rowsToSet(p.allowed, p.support, vars, n) };
    });
    const support = [...new Set([...goal.support, ...premises.flatMap(p => p.support)])].sort();
    const rows = cardinality(support, vars);
    if (rows > BigInt(maxRows) || planned + rows > BigInt(maxTotal)) return {
      schema: 'zeno-fcis/local-goal-result/experimental-1',
      status: 'inconclusive', reason: 'goal-entailment-budget',
      goal: goal.id, required_rows: rows.toString(), planned_before_refusal: planned.toString(),
      checked_goal_rows: 0, goal_safety_established: false, publication_authority: false
    };
    planned += rows;
    jobs.push({ goal, premises, support, rows: Number(rows), allowed });
  }
  let checks = 0, antecedentRows = 0;
  const goals = [];
  for (const job of jobs) {
    let witness = null, localAntecedents = 0;
    enumerate(job.support, vars, values => {
      if (witness) return;
      checks++;
      const state = Object.fromEntries(job.support.map((n, i) => [n, values[i]]));
      const holds = job.premises.every(p =>
        p.allowed.has(JSON.stringify(p.support.map(n => state[n]))));
      if (!holds) return;
      antecedentRows++;
      localAntecedents++;
      const observed = job.goal.support.map(n => state[n]);
      if (!job.allowed.has(JSON.stringify(observed))) {
        witness = { goal: job.goal.id, support: job.support,
          state: values, goal_projection: observed, premises: job.goal.using };
      }
    });
    if (witness) return {
      schema: 'zeno-fcis/local-goal-result/experimental-1',
      status: 'violated', reason: 'goal-not-entailed-by-invariants', witness,
      checked_goal_rows: checks, goal_safety_established: false,
      // This is an implication countermodel, NOT necessarily a reachable state.
      witness_may_be_unreachable: true, publication_authority: false
    };
    need(localAntecedents > 0, job.goal.id + ': inconsistent proof premises');
    goals.push({ goal: job.goal.id, premises: job.goal.using,
      check_domain_rows: job.rows, premise_rows: localAntecedents });
  }
  return {
    schema: 'zeno-fcis/local-goal-result/experimental-1', status: 'proved',
    reason: 'frozen-goals-entailed-by-checked-invariants',
    claim_level: 'DECLARATIVE_FINITE_MODEL_ONLY',
    goal_safety_established: true, unbounded_history_claim: true,
    checked_goal_rows: checks, planned_goal_rows: planned.toString(),
    antecedent_rows: antecedentRows,
    checked_inductive_obligation_rows: base.checked_obligation_rows,
    goals, publication_authority: false, executable_refinement: false,
    theorem: 'Each chosen invariant holds on the initial model state and is inductive under every model action; the checked implication from selected invariants entails each separately stated safety goal in every reachable state and every finite prefix.'
  };
}
module.exports = { SCHEMA, checkGoals };
