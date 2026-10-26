'use strict';
// Independent local-delta proof of one *global* conserved weighted sum.
// A complete validated action table and exact frame rule are its premises.
// Result never grants publication, runtime or source/refinement authority.
const { verify } = require('./proof_checker.cjs');
const SCHEMA = 'zeno-fcis/linear-conservation/experimental-1';
function req(x, why) { if (!x) throw new Error('linear-conservation: ' + why); }
function exact(x, fields, why) {
  req(x !== null && typeof x === 'object' && !Array.isArray(x), why + ': expected object');
  req(JSON.stringify(Object.keys(x).sort()) === JSON.stringify(fields.slice().sort()),
    why + ': invalid/missing/extra keys');
}
function checkLinear(model, goal) {
  const base = verify(model);
  if (base.status !== 'proved') return {
    status: base.status, reason: 'model-admission-or-invariant-blocked',
    model_result: base, global_conservation_established: false,
    publication_authority: false
  };
  exact(goal, ['schema', 'weights', 'expected_sum'], 'goal');
  req(goal.schema === SCHEMA, 'unsupported goal schema');
  req(typeof goal.expected_sum === 'string' && /^-?(0|[1-9][0-9]*)$/.test(goal.expected_sum) &&
    goal.expected_sum !== '-0', 'noncanonical exact integer total');
  req(Array.isArray(goal.weights) && goal.weights.length === model.variables.length,
    'must name every variable exactly once');
  const variables = new Map(model.variables.map(v => [v.id, v]));
  const weights = new Map();
  let last = '';
  for (const item of goal.weights) {
    exact(item, ['variable', 'coefficient'], 'weight');
    req(typeof item.variable === 'string' && variables.has(item.variable),
      'unknown weight variable');
    req(item.variable > last, 'unsorted, missing or duplicate variable weight');
    req(typeof item.coefficient === 'number' && Number.isSafeInteger(item.coefficient) &&
      Math.abs(item.coefficient) <= 1000000, 'unrepresentable weight');
    last = item.variable;
    weights.set(item.variable, BigInt(item.coefficient));
  }
  req(weights.size === variables.size, 'partial coefficient vector');
  req([...weights.values()].some(x => x !== 0n), 'all-zero weight vector is vacuous');
  const initial = [...variables].reduce((sum, [name, variable]) =>
    sum + weights.get(name) * BigInt(variable.initial), 0n);
  const expected = BigInt(goal.expected_sum);
  if (initial !== expected) return {
    status: 'violated', reason: 'genesis-global-conservation',
    expected_sum: expected.toString(), actual_sum: initial.toString(),
    global_conservation_established: false, publication_authority: false
  };
  let visitedRows = 0, enabledRows = 0;
  for (const action of model.actions) {
    const reads = new Map(action.reads.map((name, i) => [name, i]));
    for (const row of action.table) {
      visitedRows++;
      if (row.next === null) continue;
      enabledRows++;
      let delta = 0n;
      for (let j = 0; j < action.writes.length; j++) {
        const name = action.writes[j], before = row.at[reads.get(name)];
        delta += weights.get(name) * (BigInt(row.next[j]) - BigInt(before));
      }
      if (delta !== 0n) return {
        status: 'violated', reason: 'weighted-delta-violation',
        action: action.id, read_values: row.at, write_names: action.writes,
        written_values: row.next, exact_delta: delta.toString(),
        examined_rows: visitedRows, global_conservation_established: false,
        publication_authority: false
      };
    }
  }
  const states = model.variables.reduce((n, v) => n * BigInt(v.domain.length), 1n);
  return {
    status: 'proved', reason: 'global-weighted-sum-preserved-by-every-local-action',
    schema: 'zeno-fcis/linear-conservation-result/experimental-1',
    claim_level: 'DECLARATIVE_FINITE_MODEL_ONLY',
    global_conservation_established: true,
    unbounded_history_claim: true,
    expected_sum: expected.toString(),
    global_states: states.toString(),
    model_invariant_rows: base.checked_obligation_rows,
    action_delta_rows: visitedRows, enabled_action_delta_rows: enabledRows,
    variables: variables.size, actions: model.actions.length,
    publication_authority: false,
    executable_refinement: false, storage_or_effects_refinement: false,
    theorem: 'For each exact action row, the weighted sum of changes to its declared write fields is zero. All other fields are unchanged by the abstract frame rule. The initial sum equals the separately frozen expected total. Induction over action histories conserves the total for every model state reachable from genesis, without enumerating the global state product.'
  };
}
module.exports = { SCHEMA, checkLinear };