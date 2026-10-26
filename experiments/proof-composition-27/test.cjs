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

const { checkGoals, SCHEMA: GOAL_SCHEMA } = require('./goal_checker.cjs');
function freezeGoal(allowed, using = ['auto_sum_000']) {
  return { schema: GOAL_SCHEMA, goals: [{ id: 'human_conservation',
    support: ['a', 'b', 'c'], allowed, using }] };
}
test('separately stated safety goal is entailed by proposed and independently proved invariants', () => {
  const original = makeChain();
  const goal = freezeGoal(original.invariants[0].allowed);
  original.invariants = [];
  const candidate = propose(original);
  const x = checkGoals(candidate.model, goal);
  assert.equal(x.status, 'proved');
  assert.equal(x.goal_safety_established, true);
  assert.equal(x.checked_goal_rows, 27);
  assert.equal(x.publication_authority, false);
});
test('false frozen intent is not accepted even if a different invariant is proved', () => {
  const m = makeChain();
  const g = freezeGoal([[2, 0, 0]], ['supply_conservation']);
  const x = checkGoals(m, g);
  assert.equal(x.status, 'violated');
  assert.equal(x.reason, 'goal-not-entailed-by-invariants');
  assert.equal(x.witness_may_be_unreachable, true);
});
test('false invariant or broken transition blocks goal proof before entailment', () => {
  const m = makeChain();
  m.actions[0].table.find(row => row.at[0] === 1 && row.at[1] === 1).next = [0, 0];
  const x = checkGoals(m, freezeGoal(m.invariants[0].allowed, ['supply_conservation']));
  assert.equal(x.status, 'violated');
  assert.equal(x.reason, 'invariant-proof-blocked');
});
test('an oversized frozen goal is inconclusive with zero entailment rows', () => {
  const m = makePairs(25);
  const g = { schema: GOAL_SCHEMA, goals: [{
    id: 'global', support: m.variables.map(v => v.id),
    allowed: [m.variables.map(v => v.initial)], using: ['pair_000']
  }] };
  const x = checkGoals(m, g);
  assert.equal(x.status, 'inconclusive');
  assert.equal(x.checked_goal_rows, 0);
  assert.equal(x.goal_safety_established, false);
});
test('malformed goal packet is refused rather than assumed', () => {
  const m = makeChain();
  const g = freezeGoal(m.invariants[0].allowed, ['supply_conservation']);
  g.goals[0].using = ['missing'];
  assert.throws(() => checkGoals(m, g), /unknown name/);
});
test('separate proposer, frozen goals and checker CLIs agree end-to-end', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-frozen-goal-'));
  try {
    const original = makeChain();
    const goal = freezeGoal(original.invariants[0].allowed);
    original.invariants = [];
    const input = path.join(folder, 'input.json');
    const candidate = path.join(folder, 'candidate.json');
    const goalFile = path.join(folder, 'goals.json');
    fs.writeFileSync(input, canonicalJSON(original));
    fs.writeFileSync(goalFile, canonicalJSON(goal));
    const p = spawnSync(process.execPath, [path.join(__dirname, 'propose.cjs'), input, candidate],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(p.status, 0, p.stderr);
    const checked = spawnSync(process.execPath,
      [path.join(__dirname, 'run_goals.cjs'), candidate, goalFile],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(checked.status, 0, checked.stderr);
    const verdict = JSON.parse(checked.stdout);
    assert.equal(verdict.goal_safety_established, true);
    assert.match(verdict.goal_sha256, /^[0-9a-f]{64}$/);
    assert.match(verdict.model_sha256, /^[0-9a-f]{64}$/);
    assert.equal(verdict.executable_refinement, false);
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});

const { STRENGTHENING_SCHEMA } = require('./proof_checker.cjs');
const { makeMutual, mutualStrengthening } = require('./models.cjs');

test('individually insufficient invariants are refuted without strengthening', () => {
  const x = verify(makeMutual());
  assert.equal(x.status, 'violated');
  assert.equal(x.reason, 'step-violation');
});
test('simultaneous induction validates mutually dependent lemmas', () => {
  const x = verify(makeMutual(), {}, mutualStrengthening());
  assert.equal(x.status, 'proved');
  assert.equal(x.strengthening_bindings, 2);
  assert.equal(x.checked_obligation_rows, 8);
  assert.equal(x.obligations.length, 2);
  assert.equal(x.obligations[0].assumed[0], 'y_zero');
  assert.equal(x.obligations[1].assumed[0], 'x_zero');
});
test('dropping even one necessary mutual assumption blocks induction', () => {
  const packet = mutualStrengthening();
  packet.bindings.pop();
  const x = verify(makeMutual(), {}, packet);
  assert.equal(x.status, 'violated');
});
test('unsound action remains refuted under both jointly discharged hypotheses', () => {
  const model = makeMutual();
  model.actions[0].table[0].next = [1]; // bad even at the actual genesis
  const x = verify(model, {}, mutualStrengthening());
  assert.equal(x.status, 'violated');
  assert.equal(x.reason, 'step-violation');
  assert.deepEqual(x.witness.state, [0, 0]);
});
test('unknown or self-citing strengthening premise is refused', () => {
  const wrong = mutualStrengthening();
  wrong.bindings[0].assuming = ['fake_lemma'];
  assert.throws(() => verify(makeMutual(), {}, wrong), /self or unknown/);
  wrong.bindings[0].assuming = ['x_zero'];
  assert.throws(() => verify(makeMutual(), {}, wrong), /self or unknown/);
});
test('duplicate or unsorted binding cannot conceal a missing proof', () => {
  const wrong = mutualStrengthening();
  wrong.bindings.reverse();
  assert.throws(() => verify(makeMutual(), {}, wrong), /unsorted or repeated binding/);
  const duplicate = mutualStrengthening();
  duplicate.bindings.push(duplicate.bindings[1]);
  assert.throws(() => verify(makeMutual(), {}, duplicate), /unsorted or repeated binding/);
});
test('strengthened implication exceeding its limit is unknown before checking rows', () => {
  const x = verify(makeMutual(), { obligationRows: 3 }, mutualStrengthening());
  assert.equal(x.status, 'inconclusive');
  assert.equal(x.evaluated_rows, 0);
  assert.equal(x.reason, 'obligation-budget');
});
test('frozen safety goal is derived from the mutually inductive conjunction', () => {
  const m = makeMutual();
  const goals = { schema: GOAL_SCHEMA, goals: [
    { id: 'human_goal', support: ['x'], allowed: [[0]], using: ['x_zero'] }
  ] };
  assert.equal(checkGoals(m, goals).status, 'violated');
  const checked = checkGoals(m, goals, undefined, undefined, mutualStrengthening());
  assert.equal(checked.status, 'proved');
  assert.equal(checked.strengthening_bindings, 2);
  assert.equal(checked.goal_safety_established, true);
});
test('mutually inductive CLI emits source- and assumption-bound receipt', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-mutual-'));
  try {
    const m = path.join(temp, 'model.json');
    const b = path.join(temp, 'strengthening.json');
    const g = path.join(temp, 'goals.json');
    fs.writeFileSync(m, canonicalJSON(makeMutual()));
    fs.writeFileSync(b, canonicalJSON(mutualStrengthening()));
    fs.writeFileSync(g, canonicalJSON({ schema: GOAL_SCHEMA, goals: [
      { id: 'human_goal', support: ['x'], allowed: [[0]], using: ['x_zero'] }
    ] }));
    const alone = spawnSync(process.execPath, [path.join(__dirname, 'run.cjs'), m],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(alone.status, 1);
    const strengthened = spawnSync(process.execPath, [path.join(__dirname, 'run.cjs'), m, b],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(strengthened.status, 0, strengthened.stderr);
    assert.equal(JSON.parse(strengthened.stdout).strengthening_bindings, 2);
    const claim = spawnSync(process.execPath, [path.join(__dirname, 'run_goals.cjs'), m, g, b],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(claim.status, 0, claim.stderr);
    const receipt = JSON.parse(claim.stdout);
    assert.equal(receipt.goal_safety_established, true);
    assert.match(receipt.strengthening_sha256, /^[0-9a-f]{64}$/);
    assert.equal(receipt.publication_authority, false);
    // Disturbing the original hypotheses must invalidate their digest AND the check.
    const tamper = mutualStrengthening();
    tamper.bindings.pop();
    fs.writeFileSync(b, canonicalJSON(tamper));
    const forbidden = spawnSync(process.execPath, [path.join(__dirname, 'run_goals.cjs'), m, g, b],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(forbidden.status, 1, forbidden.stderr);
  } finally { fs.rmSync(temp, { recursive: true, force: true }); }
});

const { examine } = require('./global_oracle.cjs');
test('independent global oracle agrees on qualified small models and coupling', () => {
  for (const [m, assumptions] of [
    [makePairs(1), null],
    [makePairs(2), null],
    [makeChain(), null],
    [makeMutual(), null],
    [makeMutual(), mutualStrengthening()]
  ]) {
    const local = verify(m, {}, assumptions);
    const reference = examine(m, assumptions);
    assert.equal(reference.status, local.status);
    if (local.status === 'proved') assert.equal(reference.reachable_safety, 'safe');
  }
});
test('global reachability and inductive proof are different claims', () => {
  const local = verify(makeMutual());
  const global = examine(makeMutual());
  assert.equal(local.status, 'violated'); // the supplied lemmas are not individually inductive
  assert.equal(global.reachable_safety, 'safe'); // actual execution stays at genesis
  assert.equal(global.reachable_states, 1);
  assert.equal(examine(makeMutual(), mutualStrengthening()).status, 'proved');
});
test('separate exhaustive oracle agrees with 30 deterministic candidate mutations', () => {
  let seed = 0x5eeda11;
  const next = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0);
  for (let sample = 0; sample < 30; sample++) {
    const m = makePairs(2);
    const a = m.actions[next() % m.actions.length];
    const r = a.table[next() % a.table.length];
    // Only corrupt exact declared next values; leave model otherwise well formed.
    r.next = [(next() % 3), (next() % 3)];
    const local = verify(m);
    const global = examine(m);
    assert.equal(global.status, local.status, 'mutation #' + sample);
    if (local.status === 'proved') assert.equal(global.reachable_safety, 'safe');
  }
});
test('global oracle refuses to enumerate enormous composition products', () => {
  const o = examine(makePairs(25));
  assert.equal(o.status, 'inconclusive');
  assert.equal(o.examined, 0);
});

const { checkLinear } = require('./linear_checker.cjs');
const { makeLinkedLedger, linearGoal } = require('./models.cjs');
test('connected 50-variable ledger proves a global sum without enumerating 3^50 states', () => {
  const m = makeLinkedLedger(50);
  const r = checkLinear(m, linearGoal(m));
  assert.equal(r.status, 'proved');
  assert.equal(r.global_conservation_established, true);
  assert.equal(r.global_states, (3n ** 50n).toString());
  assert.equal(r.action_delta_rows, 882);
  assert.equal(r.actions, 98);
  assert.equal(r.publication_authority, false);
});
test('connected 200-variable ledger proves a global sum from 3582 local rows', () => {
  const m = makeLinkedLedger(200);
  const r = checkLinear(m, linearGoal(m));
  assert.equal(r.status, 'proved');
  assert.equal(r.global_states, (3n ** 200n).toString());
  assert.equal(r.action_delta_rows, 3582);
  assert.equal(r.actions, 398);
});
test('non-conservative transfer is refuted even when model-admission invariant is tautological', () => {
  const m = makeLinkedLedger(50);
  m.actions[0].table.find(row => row.at[0] === 1 && row.at[1] === 1).next = [0, 0];
  const report = checkLinear(m, linearGoal(m));
  assert.equal(report.status, 'violated');
  assert.equal(report.reason, 'weighted-delta-violation');
  assert.equal(report.exact_delta, '-2');
});
test('frozen global total cannot be changed without a genesis failure', () => {
  const m = makeLinkedLedger(3);
  const goal = linearGoal(m);
  goal.expected_sum = '3';
  const report = checkLinear(m, goal);
  assert.equal(report.status, 'violated');
  assert.equal(report.reason, 'genesis-global-conservation');
});
test('partial, duplicate, zero and unrepresentable coefficient vectors refuse', () => {
  const m = makeLinkedLedger(3);
  let g = linearGoal(m); g.weights.pop();
  assert.throws(() => checkLinear(m, g), /must name every variable exactly once/);
  g = linearGoal(m); g.weights[1] = { ...g.weights[0] };
  assert.throws(() => checkLinear(m, g), /unsorted, missing or duplicate/);
  g = linearGoal(m); g.weights.forEach(w => w.coefficient = 0);
  assert.throws(() => checkLinear(m, g), /all-zero weight vector/);
  g = linearGoal(m); g.weights[1].coefficient = Number.MAX_SAFE_INTEGER;
  assert.throws(() => checkLinear(m, g), /unrepresentable weight/);
});
test('small global conservation agrees with exhaustive three-account reference', () => {
  const m = makeLinkedLedger(3), goal = linearGoal(m);
  const allowed = [];
  for (let a = 0; a < 3; a++) for (let b = 0; b < 3; b++)
    for (let c = 0; c < 3; c++) if (a + b + c === 2) allowed.push([a, b, c]);
  m.invariants = [{ id: 'global_exact', support: ['v000', 'v001', 'v002'], allowed }];
  assert.equal(verify(m).status, 'proved');
  assert.equal(examine(m).status, 'proved');
  assert.equal(examine(m).reachable_safety, 'safe');
  assert.equal(checkLinear(m, goal).status, 'proved');
});
test('independent conservation CLI freezes exact model and goal identities', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'zfcis-linear-'));
  try {
    const m = makeLinkedLedger(50), goal = linearGoal(m);
    const a = path.join(folder, 'model.json');
    const b = path.join(folder, 'goal.json');
    fs.writeFileSync(a, canonicalJSON(m));
    fs.writeFileSync(b, canonicalJSON(goal));
    const p = spawnSync(process.execPath, [path.join(__dirname, 'run_linear.cjs'), a, b],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(p.status, 0, p.stderr);
    const report = JSON.parse(p.stdout);
    assert.equal(report.global_conservation_established, true);
    assert.match(report.goal_sha256, /^[0-9a-f]{64}$/);
    assert.match(report.model_sha256, /^[0-9a-f]{64}$/);
    assert.equal(report.executable_refinement, false);
    const tampered = makeLinkedLedger(50);
    tampered.actions[0].table.find(row => row.at[0] === 1 && row.at[1] === 1).next = [0, 0];
    fs.writeFileSync(a, canonicalJSON(tampered));
    const falseProof = spawnSync(process.execPath, [path.join(__dirname, 'run_linear.cjs'), a, b],
      { encoding: 'utf8', timeout: 15000 });
    assert.equal(falseProof.status, 1, falseProof.stderr);
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});
