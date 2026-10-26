'use strict';
// UNTRUSTED invariant proposer. Nothing in this module can authorize proof.
// It discovers small dependency components and guesses local sum invariants.
// Every proposal must be checked afresh by proof_checker.cjs.
function propose(model, { maxSupport = 3, maxRows = 4096 } = {}) {
  if (!Number.isSafeInteger(maxSupport) || maxSupport < 1 || maxSupport > 6
    || !Number.isSafeInteger(maxRows) || maxRows < 1 || maxRows > 4096) {
    throw new Error('proposer limits may not exceed built-in caps');
  }
  if (!model || !Array.isArray(model.variables) || !Array.isArray(model.actions)
    || !Array.isArray(model.invariants)) throw new Error('malformed proposal input');
  const vars = new Map(model.variables.map(v => [v.id, v]));
  if (vars.size !== model.variables.length) throw new Error('duplicate variables');
  const parent = new Map(model.variables.map(v => [v.id, v.id]));
  function find(n) {
    if (!parent.has(n)) throw new Error('unknown read variable ' + n);
    let p = n;
    while (parent.get(p) !== p) p = parent.get(p);
    return p;
  }
  function union(a, b) { parent.set(find(a), find(b)); }
  for (const action of model.actions) {
    if (!Array.isArray(action.reads)) throw new Error('malformed action reads');
    for (let i = 1; i < action.reads.length; i++) union(action.reads[0], action.reads[i]);
  }
  const components = new Map();
  for (const name of vars.keys()) {
    const p = find(name);
    if (!components.has(p)) components.set(p, []);
    components.get(p).push(name);
  }
  const candidates = [], unsupported = [];
  const existing = new Set(model.invariants.map(p => p.id));
  let nextId = 0;
  for (const names of [...components.values()].map(x => x.sort())
    .sort((a, b) => a[0].localeCompare(b[0]))) {
    const rows = names.reduce((n, x) => n * BigInt(vars.get(x).domain.length), 1n);
    if (names.length > maxSupport || rows > BigInt(maxRows)) {
      unsupported.push({ support: names, reason: 'dependency-component-too-large', rows: rows.toString() });
      continue;
    }
    const sum0 = names.reduce((s, x) => s + BigInt(vars.get(x).initial), 0n);
    const allowed = [];
    const indexes = Array(names.length).fill(0);
    const domains = names.map(n => vars.get(n).domain);
    while (true) {
      const at = indexes.map((i, j) => domains[j][i]);
      if (at.reduce((s, v) => s + BigInt(v), 0n) === sum0) allowed.push(at);
      let k = indexes.length - 1;
      while (k >= 0) {
        indexes[k]++;
        if (indexes[k] < domains[k].length) break;
        indexes[k--] = 0;
      }
      if (k < 0) break;
    }
    if (allowed.length === Number(rows)) continue; // never propose a tautology
    let candidateId;
    do { candidateId = 'auto_sum_' + String(nextId++).padStart(3, '0'); } while (existing.has(candidateId));
    existing.add(candidateId);
    candidates.push({ id: candidateId, support: names, allowed });
  }
  return {
    model: { ...model, invariants: [...model.invariants, ...candidates].sort((a, b) => a.id.localeCompare(b.id)) },
    proposals: candidates.map(p => ({ invariant: p.id, support: p.support,
      method: 'untrusted-component-constant-sum' })),
    unsupported,
    proof_status: 'UNVERIFIED_PROPOSALS',
    publication_authority: false
  };
}
module.exports = { propose };
