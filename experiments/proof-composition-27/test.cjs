'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { spawnSync } = require('node:child_process');
const { verify, SCHEMA } = require('./proof_checker.cjs');
const { makePairs, makeChain } = require('./models.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');

function runModel(model) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-composition-'));
  try {
    const filename = path.join(folder, 'model.json');
    fs.writeFileSync(filename, canonicalJSON(model));
    return spawnSync(process.execPath, [path.join(__dirname, 'run.cjs'), filename],
      { encoding: 'utf8', timeout: 15000 });
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
}
function rejectedChange(mutator, fragment) {
  const model = makePairs(1);
  mutator(model);
  assert.throws(() => verify(model), err => String(err).includes(fragment));
}
test('model schema and canonical bytes are stable', () => {
  const model = makeChain();
  assert.equal(model.schema, SCHEMA);
  assert.equal(canonicalJSON(JSON.parse(canonicalJSON(model))), canonicalJSON(model));
});
test('50 state variables prove without enumerating their global product', () => {
  const report = verify(makePairs(25));
  assert.equal(report.status, 'proved');
  assert.equal(report.global_states, '717897987691852588770249');
  assert.equal(report.monolithic_event_rows, (3n ** 50n * 50n).toString());
  assert.equal(report.checked_action_table_rows, 450);
  assert.equal(report.checked_obligation_rows, 450);
  assert.equal(report.obligations.length, 50);
  assert.equal(report.publication_authority, false);
  assert.equal(report.executable_refinement, false);
});
test('200 state variables remain localized', () => {
  const report = verify(makePairs(100));
  assert.equal(report.status, 'proved');
  assert.equal(report.global_states, (3n ** 200n).toString());
  assert.equal(report.checked_obligation_rows, 1800);
});
test('cross-action coupling property is inductive on three accounts', () => {
  const report = verify(makeChain());
  assert.equal(report.status, 'proved');
  assert.equal(report.checked_obligation_rows, 108);
  assert.equal(report.obligations.length, 4);
});
test('mutated transition violating conservation has a concrete replayable witness', () => {
  const model = makePairs(2);
  const row = model.actions[0].table.find(r => r.at[0] === 1 && r.at[1] === 1);
  row.next = [0, 0];
  const report = verify(model);
  assert.equal(report.status, 'violated');
  assert.equal(report.reason, 'step-violation');
  assert.deepEqual(report.witness.invariant_before, [1, 1]);
  assert.deepEqual(report.witness.invariant_after, [0, 0]);
});
test('wrong initial state is refuted, not treated as a proof', () => {
  const model = makeChain();
  model.invariants[0].allowed = [[0, 2, 0]];
  assert.equal(verify(model).reason, 'genesis-violation');
});
test('a global invariant exceeding the finite support budget is UNKNOWN', () => {
  const model = makePairs(25);
  model.invariants.push({
    id: 'monolithic_spec',
    support: model.variables.map(x => x.id),
    allowed: [model.variables.map(x => x.initial)]
  });
  const report = verify(model);
  assert.equal(report.status, 'inconclusive');
  assert.equal(report.reason, 'obligation-budget');
  assert.equal(report.evaluated_rows, 0);
  assert.equal(report.unbounded_history_claim, false);
});
test('a smaller caller-imposed budget cannot silently sample', () => {
  const report = verify(makePairs(2), { totalObligationRows: 1 });
  assert.equal(report.status, 'inconclusive');
  assert.equal(report.evaluated_rows, 0);
});
test('budgets cannot be raised past built-in maximum', () => {
  assert.throws(() => verify(makePairs(1), { obligationRows: 65536 }), /cannot increase/);
});
test('incomplete action table is refused', () =>
  rejectedChange(m => m.actions[0].table.pop(), 'incomplete action table'));
test('action table must be in exact domain order', () =>
  rejectedChange(m => m.actions[0].table.reverse(), 'lexicographic domain order'));
test('out-of-domain successor is refused', () =>
  rejectedChange(m => { m.actions[0].table[0].next = [3, 0]; }, 'outside'));
test('no undeclared write or ambient state read', () =>
  rejectedChange(m => { m.actions[0].writes = ['a000']; }, 'tuple shape'));
test('a new undeclared variable read is refused', () =>
  rejectedChange(m => { m.actions[0].reads = ['a000', 'b000', 'third']; }, 'unknown variable'));
test('unknown keys are refused before checking', () =>
  rejectedChange(m => { m.actions[0].trusted = true; }, 'wrong/missing/extra keys'));
test('duplicate allowed state is refused', () =>
  rejectedChange(m => { m.invariants[0].allowed.push([2, 0]); }, 'duplicate allowed'));
test('duplicate state declaration is refused', () =>
  rejectedChange(m => m.variables.push({ ...m.variables[0] }), 'duplicate variable'));
test('CLI works end-to-end on canonical, good input', () => {
  const result = runModel(makeChain());
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'proved');
  assert.match(report.model_sha256, /^[0-9a-f]{64}$/);
  assert.match(report.checker_source_sha256, /^[0-9a-f]{64}$/);
  assert.equal(report.publication_authority, false);
});
test('CLI returns exit 1 on a real counterexample', () => {
  const model = makeChain();
  model.actions[0].table.find(r => r.at[0] === 1 && r.at[1] === 1).next = [0, 0];
  const result = runModel(model);
  assert.equal(result.status, 1, result.stderr);
  assert.equal(JSON.parse(result.stdout).status, 'violated');
});
test('CLI refuses noncanonical duplicate-key JSON', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-composition-'));
  try {
    const f = path.join(folder, 'model.json');
    const canonical = canonicalJSON(makeChain());
    fs.writeFileSync(f, canonical.replace('"schema": "' + SCHEMA + '"',
      '"schema": "' + SCHEMA + '", "schema": "' + SCHEMA + '"'));
    const p = spawnSync(process.execPath, [path.join(__dirname, 'run.cjs'), f],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(p.status, 65, p.stderr);
    assert.match(p.stderr, /noncanonical/);
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});
test('CLI returns exit 2 for an oversized proof obligation', () => {
  const model = makePairs(25);
  model.invariants.push({ id: 'global', support: model.variables.map(v => v.id),
    allowed: [model.variables.map(v => v.initial)] });
  const p = runModel(model);
  assert.equal(p.status, 2, p.stderr);
  assert.equal(JSON.parse(p.stdout).status, 'inconclusive');
});

const { propose } = require('./propose_invariants.cjs');
test('untrusted proposer discovers and independent checker proves 25 conservation laws', () => {
  const model = makePairs(25);
  model.invariants = [];
  const proposed = propose(model);
  assert.equal(proposed.proposals.length, 25);
  assert.equal(proposed.unsupported.length, 0);
  assert.equal(proposed.proof_status, 'UNVERIFIED_PROPOSALS');
  assert.equal(proposed.publication_authority, false);
  const checked = verify(proposed.model);
  assert.equal(checked.status, 'proved');
  assert.equal(checked.checked_obligation_rows, 450);
});
test('proposer also discovers the three-account coupling invariant', () => {
  const model = makeChain();
  model.invariants = [];
  const guess = propose(model);
  assert.equal(guess.proposals.length, 1);
  assert.deepEqual(guess.proposals[0].support, ['a', 'b', 'c']);
  assert.equal(verify(guess.model).status, 'proved');
});
test('proposer cannot promote an incorrect transition to a proof', () => {
  const model = makeChain();
  model.invariants = [];
  model.actions[0].table.find(r => r.at[0] === 1 && r.at[1] === 1).next = [0, 0];
  const guess = propose(model);
  assert.equal(guess.proof_status, 'UNVERIFIED_PROPOSALS');
  assert.equal(verify(guess.model).status, 'violated');
});
test('proposer refuses rather than hiding an overlarge connected support', () => {
  const model = makePairs(2);
  model.invariants = [];
  const table = [];
  for (let a = 0; a < 3; a++) for (let b = 0; b < 3; b++) {
    table.push({ at: [a, b], next: null });
  }
  model.actions.push({ id: 'cross_read', reads: ['a000', 'a001'],
    writes: ['a000'], table });
  const guess = propose(model);
  assert.equal(guess.proposals.length, 0);
  assert.equal(guess.unsupported.length, 1);
  assert.equal(guess.publication_authority, false);
});
test('untrusted proposer CLI yields a candidate that requires a separate checker invocation', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-discovery-'));
  try {
    const original = makePairs(25);
    original.invariants = [];
    const input = path.join(folder, 'input.json');
    const candidate = path.join(folder, 'candidate.json');
    fs.writeFileSync(input, canonicalJSON(original));
    const produced = spawnSync(process.execPath, [path.join(__dirname, 'propose.cjs'), input, candidate],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(produced.status, 0, produced.stderr);
    assert.equal(JSON.parse(produced.stderr).publication_authority, false);
    const output = runModel(JSON.parse(fs.readFileSync(candidate, 'utf8')));
    assert.equal(output.status, 0, output.stderr);
    assert.equal(JSON.parse(output.stdout).status, 'proved');
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});
