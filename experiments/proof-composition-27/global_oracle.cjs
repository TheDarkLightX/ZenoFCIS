'use strict';
// Independent *global-product* oracle, intentionally unlike local checker.
// Only intended for small negative controls; no publication authority.
function examine(model, strengthening = null, maxStates = 4096, maxEdges = 50000) {
  function ensure(ok, reason) { if (!ok) throw Error('oracle: ' + reason); }
  ensure(Number.isSafeInteger(maxStates) && maxStates >= 1 && maxStates <= 4096,
    'invalid state cap');
  ensure(Number.isSafeInteger(maxEdges) && maxEdges >= 1 && maxEdges <= 50000,
    'invalid edge cap');
  ensure(model && Array.isArray(model.variables) && Array.isArray(model.actions) &&
    Array.isArray(model.invariants), 'model form');
  const vars = model.variables;
  const domains = vars.map(v => v.domain);
  const names = vars.map(v => v.id);
  ensure(new Set(names).size === names.length && domains.every(d => Array.isArray(d) && d.length),
    'variable domains');
  const total = domains.reduce((n, d) => n * BigInt(d.length), 1n);
  const plannedEdges = total * BigInt(model.actions.length);
  if (total > BigInt(maxStates) || plannedEdges > BigInt(maxEdges)) {
    return { status: 'inconclusive', reason: 'global-product-budget',
      global_states: total.toString(), planned_edges: plannedEdges.toString(),
      examined: 0, publication_authority: false };
  }
  const index = new Map(names.map((n, i) => [n, i]));
  const invs = new Map(model.invariants.map(p => [p.id, p]));
  ensure(invs.size === model.invariants.length, 'duplicate invariant');
  const selected = new Map();
  if (strengthening !== null) {
    ensure(Array.isArray(strengthening.bindings), 'strengthening form');
    for (const b of strengthening.bindings) {
      const k = b.invariant + ':' + b.action;
      ensure(!selected.has(k), 'duplicate strengthening binding');
      ensure(invs.has(b.invariant), 'unknown target');
      ensure(b.assuming.every(n => invs.has(n)), 'unknown assumption');
      selected.set(k, b.assuming);
    }
  }
  function projection(state, fields) {
    return fields.map(n => {
      ensure(index.has(n), 'unknown state field ' + n);
      return state[index.get(n)];
    });
  }
  function holds(state, invariant) {
    const values = projection(state, invariant.support);
    return invariant.allowed.some(row => row.length === values.length &&
      row.every((x, i) => x === values[i]));
  }
  function transition(state, action) {
    const read = projection(state, action.reads);
    const row = action.table.find(r => r.at.length === read.length &&
      r.at.every((v, i) => v === read[i]));
    ensure(row, 'transition table has a missing row');
    if (row.next === null) return state.slice();
    ensure(row.next.length === action.writes.length, 'transition output arity');
    const result = state.slice();
    for (let j = 0; j < action.writes.length; j++) {
      const col = index.get(action.writes[j]);
      ensure(col !== undefined, 'unknown write variable');
      ensure(domains[col].includes(row.next[j]), 'output outside domain');
      result[col] = row.next[j];
    }
    return result;
  }
  // This independently enumerates the *full global state product*.
  const all = [];
  function visit(i, prefix) {
    if (i === names.length) { all.push(prefix); return; }
    for (const value of domains[i]) visit(i + 1, [...prefix, value]);
  }
  visit(0, []);
  const genesis = vars.map(v => v.initial);
  for (const p of invs.values()) {
    if (!holds(genesis, p)) return {
      status: 'violated', reason: 'genesis', invariant: p.id,
      global_states: total.toString(), publication_authority: false
    };
  }
  let inductionFailure = null, scanned = 0;
  for (const state of all) {
    for (const action of model.actions) {
      const post = transition(state, action);
      scanned++;
      for (const p of invs.values()) {
        if (!holds(state, p)) continue;
        const hypotheses = selected.get(p.id + ':' + action.id) || [];
        if (hypotheses.some(name => !holds(state, invs.get(name)))) continue;
        if (!holds(post, p)) {
          inductionFailure = { invariant: p.id, action: action.id, pre: state, post,
            assumed: hypotheses };
          break;
        }
      }
      if (inductionFailure) break;
    }
    if (inductionFailure) break;
  }
  // Separately answer the *reachable safety* question; inductive failure
  // need not be an actual counterexample from genesis.
  const visited = new Set(), queue = [genesis], k = st => JSON.stringify(st);
  visited.add(k(genesis));
  let reachFailure = null, edges = 0;
  while (queue.length) {
    const state = queue.shift();
    for (const p of invs.values()) if (!holds(state, p)) {
      reachFailure = { invariant: p.id, state }; break;
    }
    if (reachFailure) break;
    for (const action of model.actions) {
      const after = transition(state, action);
      edges++;
      const encoded = k(after);
      if (!visited.has(encoded)) { visited.add(encoded); queue.push(after); }
    }
  }
  return {
    status: inductionFailure === null ? 'proved' : 'violated',
    reason: inductionFailure === null ? 'all-global-induction-pairs' : 'global-induction-failure',
    global_states: total.toString(), checked_global_edges: scanned,
    induction_counterexample: inductionFailure,
    reachable_states: visited.size, checked_reachable_edges: edges,
    reachable_safety: reachFailure === null ? 'safe' : 'violated',
    reachable_counterexample: reachFailure,
    claim_level: 'DECLARATIVE_FINITE_MODEL_ONLY',
    publication_authority: false
  };
}
module.exports = { examine };