'use strict';

// Experimental, independent, finite-table compositional safety checker.
// This is NOT the ZenoFCIS production Authority or a source/runtime refinement proof.
const SCHEMA = 'zeno-fcis/local-composition/experimental-1';
const MAX = Object.freeze({
  variables: 1000, actions: 2000, invariants: 2000,
  actionRows: 4096, totalActionRows: 1000000,
  obligationRows: 4096, totalObligationRows: 1000000,
  allowedRows: 65536
});

function requireThat(ok, message) {
  if (!ok) throw new Error(message);
}
function record(x, path, fields) {
  requireThat(x !== null && typeof x === 'object' && !Array.isArray(x), path + ': expected object');
  const seen = Object.keys(x).sort();
  const expected = fields.slice().sort();
  requireThat(JSON.stringify(seen) === JSON.stringify(expected), path + ': wrong/missing/extra keys');
}
function id(x, path) {
  requireThat(typeof x === 'string' && /^[a-z][a-z0-9_]{0,62}$/.test(x), path + ': invalid identifier');
  return x;
}
function orderedNames(names, path, varMap, maxLength) {
  requireThat(Array.isArray(names) && names.length > 0 && names.length <= maxLength, path + ': invalid size');
  for (let i = 0; i < names.length; i++) {
    id(names[i], path);
    requireThat(varMap.has(names[i]), path + ': unknown variable ' + names[i]);
    requireThat(i === 0 || names[i - 1] < names[i], path + ': names must be strictly sorted');
  }
  return names;
}
function sizeOf(names, variables) {
  return names.reduce((n, x) => n * BigInt(variables.get(x).domain.length), 1n);
}
function valuesValid(tuple, names, variables, path) {
  requireThat(Array.isArray(tuple) && tuple.length === names.length, path + ': tuple shape');
  for (let j = 0; j < names.length; j++) {
    requireThat(variables.get(names[j]).domain.includes(tuple[j]), path + ': value outside ' + names[j]);
  }
}
function tuples(names, variables, visitor) {
  const domains = names.map(x => variables.get(x).domain);
  const cursors = Array(names.length).fill(0);
  while (true) {
    visitor(cursors.map((v, i) => domains[i][v]));
    let k = cursors.length - 1;
    for (; k >= 0; k--) {
      cursors[k]++;
      if (cursors[k] < domains[k].length) break;
      cursors[k] = 0;
    }
    if (k < 0) break;
  }
}
function same(x, y) { return JSON.stringify(x) === JSON.stringify(y); }
function key(xs) { return JSON.stringify(xs); }
function result(status, reason, extra) {
  return Object.assign({
    schema: 'zeno-fcis/local-composition-result/experimental-1',
    status, reason,
    claim_level: 'DECLARATIVE_FINITE_MODEL_ONLY',
    unbounded_history_claim: status === 'proved',
    executable_refinement: false,
    storage_or_effects_refinement: false,
    publication_authority: false
  }, extra);
}
function budget(which, maximum, declared) {
  requireThat(declared === undefined || (Number.isSafeInteger(declared) && declared >= 1 && declared <= maximum),
    which + ': cannot increase built-in budget');
  return declared === undefined ? maximum : declared;
}

// Selected pre-state hypotheses must themselves be discharged as part of
// the same simultaneous induction. Circular dependencies are permissible:
// genesis establishes the conjunction and all actions preserve it.
const STRENGTHENING_SCHEMA = 'zeno-fcis/local-strengthening/experimental-1';
function validateStrengthening(packet, invariants, actions) {
  const found = new Map();
  if (packet === null) return found;
  record(packet, 'strengthening', ['schema', 'bindings']);
  requireThat(packet.schema === STRENGTHENING_SCHEMA, 'strengthening: unsupported schema');
  requireThat(Array.isArray(packet.bindings) && packet.bindings.length <= 10000,
    'strengthening: invalid binding count');
  let previous = '';
  for (const b of packet.bindings) {
    record(b, 'strengthening.binding', ['invariant', 'action', 'assuming']);
    id(b.invariant, 'strengthening.invariant');
    id(b.action, 'strengthening.action');
    requireThat(invariants.has(b.invariant) && actions.has(b.action),
      'strengthening: unknown invariant or action');
    requireThat(actions.get(b.action).writes.some(n => invariants.get(b.invariant).support.includes(n)),
      'strengthening: unused action/frame binding');
    const identity = b.invariant + ':' + b.action;
    requireThat(identity > previous, 'strengthening: unsorted or repeated binding');
    previous = identity;
    requireThat(Array.isArray(b.assuming) && b.assuming.length > 0 &&
      b.assuming.length <= 24, 'strengthening: invalid hypothesis count');
    const selected = [];
    for (const assumption of b.assuming) {
      id(assumption, 'strengthening.assuming');
      requireThat(assumption !== b.invariant && invariants.has(assumption),
        'strengthening: self or unknown hypothesis');
      requireThat(!selected.length || selected[selected.length - 1].id < assumption,
        'strengthening: unsorted or repeated hypothesis');
      selected.push(invariants.get(assumption));
    }
    found.set(identity, selected);
  }
  return found;
}

function verify(model, limits = {}, strengthening = null) {
  record(limits, 'limits', Object.keys(limits)); // validated explicitly below
  for (const name of Object.keys(limits)) {
    requireThat(name === 'obligationRows' || name === 'totalObligationRows', 'unknown limit ' + name);
  }
  const maxPer = budget('obligationRows', MAX.obligationRows, limits.obligationRows);
  const maxTotal = budget('totalObligationRows', MAX.totalObligationRows, limits.totalObligationRows);
  record(model, 'model', ['schema', 'variables', 'actions', 'invariants']);
  requireThat(model.schema === SCHEMA, 'model: unsupported schema');
  const { variables: declarations, actions: transitions, invariants: conditions } = model;
  requireThat(Array.isArray(declarations) && declarations.length > 0 && declarations.length <= MAX.variables, 'variables: invalid length');
  requireThat(Array.isArray(transitions) && transitions.length > 0 && transitions.length <= MAX.actions, 'actions: invalid length');
  requireThat(Array.isArray(conditions) && conditions.length > 0 && conditions.length <= MAX.invariants, 'invariants: invalid length');

  const variables = new Map();
  for (const v of declarations) {
    record(v, 'variable', ['id', 'domain', 'initial']);
    id(v.id, 'variable.id');
    requireThat(!variables.has(v.id), 'duplicate variable ' + v.id);
    requireThat(Array.isArray(v.domain) && v.domain.length > 0 && v.domain.length <= 16, 'variable ' + v.id + ': domain size');
    for (let j = 0; j < v.domain.length; j++) {
      requireThat(Number.isSafeInteger(v.domain[j]) && (j === 0 || v.domain[j - 1] < v.domain[j]), 'variable ' + v.id + ': domain order/integer');
    }
    requireThat(v.domain.includes(v.initial), 'variable ' + v.id + ': initial outside domain');
    variables.set(v.id, v);
  }
  const states = sizeOf([...variables.keys()], variables);
  let tableRows = 0;
  const actions = new Map();
  for (const a of transitions) {
    record(a, 'action', ['id', 'reads', 'writes', 'table']);
    id(a.id, 'action.id');
    requireThat(!actions.has(a.id), 'duplicate action ' + a.id);
    orderedNames(a.reads, a.id + '.reads', variables, 12);
    orderedNames(a.writes, a.id + '.writes', variables, 12);
    requireThat(a.writes.every(n => a.reads.includes(n)), a.id + ': writes not subset of reads');
    const count = sizeOf(a.reads, variables);
    if (count > BigInt(MAX.actionRows) || BigInt(tableRows) + count > BigInt(MAX.totalActionRows)) {
      return result('inconclusive', 'action-table-budget', {
        action: a.id, required_rows: count.toString(), max_action_rows: MAX.actionRows,
        global_states: states.toString(), evaluated_rows: 0
      });
    }
    requireThat(Array.isArray(a.table) && a.table.length === Number(count), a.id + ': incomplete action table');
    const lookup = new Map();
    let rowIndex = 0;
    tuples(a.reads, variables, at => {
      const row = a.table[rowIndex++];
      record(row, a.id + '.table.row', ['at', 'next']);
      valuesValid(row.at, a.reads, variables, a.id + '.at');
      requireThat(same(row.at, at), a.id + ': table must cover every read tuple exactly in lexicographic domain order');
      if (row.next !== null) valuesValid(row.next, a.writes, variables, a.id + '.next');
      lookup.set(key(at), row.next);
    });
    tableRows += Number(count);
    actions.set(a.id, { ...a, lookup, writeOffsets: new Map(a.writes.map((n, i) => [n, i])) });
  }

  const invariants = new Map();
  for (const p of conditions) {
    record(p, 'invariant', ['id', 'support', 'allowed']);
    id(p.id, 'invariant.id');
    requireThat(!invariants.has(p.id), 'duplicate invariant ' + p.id);
    orderedNames(p.support, p.id + '.support', variables, MAX.variables);
    requireThat(Array.isArray(p.allowed) && p.allowed.length > 0 && p.allowed.length <= MAX.allowedRows, p.id + ': allowed size');
    const allowed = new Set();
    for (const at of p.allowed) {
      valuesValid(at, p.support, variables, p.id + '.allowed');
      requireThat(!allowed.has(key(at)), p.id + ': duplicate allowed tuple');
      allowed.add(key(at));
    }
    invariants.set(p.id, { ...p, allowedSet: allowed });
  }

  const assumptions = validateStrengthening(strengthening, invariants, actions);
  for (const p of invariants.values()) {
    const initial = p.support.map(n => variables.get(n).initial);
    if (!p.allowedSet.has(key(initial))) {
      return result('violated', 'genesis-violation', {
        invariant: p.id, initial, global_states: states.toString(), evaluated_rows: 0
      });
    }
  }

  // Complete planning happens BEFORE any obligation evaluation; no sampling.
  const planned = [];
  let total = 0n;
  for (const p of invariants.values()) {
    for (const a of actions.values()) {
      if (!a.writes.some(n => p.support.includes(n))) continue; // exact frame
      const using = assumptions.get(p.id + ':' + a.id) || [];
      const names = [...new Set([
        ...p.support, ...a.reads, ...using.flatMap(q => q.support)
      ])].sort();
      const count = sizeOf(names, variables);
      if (count > BigInt(maxPer) || total + count > BigInt(maxTotal)) {
        return result('inconclusive', 'obligation-budget', {
          invariant: p.id, action: a.id, required_rows: count.toString(),
          planned_before_refusal: total.toString(), max_obligation_rows: maxPer,
          max_total_rows: maxTotal, global_states: states.toString(), evaluated_rows: 0
        });
      }
      total += count;
      planned.push({ p, a, using, names, count: Number(count) });
    }
  }

  let checked = 0, premises = 0, enabled = 0;
  const obligations = [];
  for (const { p, a, using, names, count } of planned) {
    let localPremises = 0, localEnabled = 0, failure = null;
    tuples(names, variables, tuple => {
      if (failure) return;
      checked++;
      const state = Object.fromEntries(names.map((n, j) => [n, tuple[j]]));
      const before = p.support.map(n => state[n]);
      if (!p.allowedSet.has(key(before))) return;
      // A candidate lemma is useful only where it holds on the pre-state;
      // every lemma is separately required to be preserved by all actions.
      if (using.some(q => !q.allowedSet.has(key(q.support.map(n => state[n]))))) return;
      premises++;
      localPremises++;
      const read = a.reads.map(n => state[n]);
      const next = a.lookup.get(key(read));
      requireThat(next !== undefined, 'internal error: incomplete table');
      if (next === null) return; // no commit; this action stutters
      enabled++;
      localEnabled++;
      const after = p.support.map(n => a.writeOffsets.has(n) ? next[a.writeOffsets.get(n)] : state[n]);
      if (!p.allowedSet.has(key(after))) {
        failure = { invariant: p.id, action: a.id, assumed: using.map(q => q.id),
          frame: names, state: tuple,
          read, writes: a.writes, written: next, invariant_before: before, invariant_after: after };
      }
    });
    if (failure) {
      return result('violated', 'step-violation', {
        witness: failure, checked_obligation_rows: checked, checked_action_table_rows: tableRows,
        global_states: states.toString()
      });
    }
    requireThat(localPremises > 0, 'internal error: inconsistent induction hypotheses');
    obligations.push({ invariant: p.id, action: a.id, assumed: using.map(q => q.id),
      local_states: count, admissible_pre_rows: localPremises, enabled_pre_rows: localEnabled });
  }
  return result('proved', strengthening ? 'all-mutual-induction-obligations-checked' : 'all-local-induction-obligations-checked', {
    global_states: states.toString(),
    monolithic_event_rows: (states * BigInt(actions.size)).toString(),
    variables: variables.size, actions: actions.size, invariants: invariants.size,
    strengthening_bindings: assumptions.size,
    checked_action_table_rows: tableRows, planned_obligation_rows: total.toString(),
    checked_obligation_rows: checked, premise_rows: premises, enabled_rows: enabled,
    obligations,
    theorem: 'By simultaneous induction over arbitrary finite or unbounded action histories, the conjunction of all invariants holds for every reachable state of this exact finite-table model. Each obligation assumes selected conjuncts only in the pre-state, proves its target after the step, and relies on the declared frame rule. No executable or shell refinement is covered.'
  });
}
module.exports = { SCHEMA, STRENGTHENING_SCHEMA, MAX, verify };
