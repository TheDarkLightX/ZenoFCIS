'use strict';
// Declarative fixtures only. This file has NO role in the independent checker.
const { SCHEMA } = require('./proof_checker.cjs');
const DOMAIN = [0, 1, 2];
const pairs = [['0', '2'], ['1', '1'], ['2', '0']].map(row => row.map(Number));

function table(step) {
  const rows = [];
  for (const x of DOMAIN) for (const y of DOMAIN) {
    rows.push({ at: [x, y], next: step(x, y) });
  }
  return rows;
}
function twoWay(a, b, prefix) {
  return [
    { id: prefix + '_forward', reads: [a, b], writes: [a, b],
      table: table((x, y) => x > 0 && y < 2 ? [x - 1, y + 1] : null) },
    { id: prefix + '_reverse', reads: [a, b], writes: [a, b],
      table: table((x, y) => x < 2 && y > 0 ? [x + 1, y - 1] : null) }
  ];
}
function makePairs(count) {
  if (!Number.isSafeInteger(count) || count < 1 || count > 500) throw new Error('count must be 1..500');
  const variables = [], actions = [], invariants = [];
  for (let i = 0; i < count; i++) {
    const suffix = String(i).padStart(3, '0');
    const a = 'a' + suffix, b = 'b' + suffix;
    variables.push({ id: a, domain: DOMAIN.slice(), initial: 2 });
    variables.push({ id: b, domain: DOMAIN.slice(), initial: 0 });
    actions.push(...twoWay(a, b, 'shift_' + suffix));
    invariants.push({ id: 'pair_' + suffix, support: [a, b],
      allowed: [[0, 2], [1, 1], [2, 0]] });
  }
  return { schema: SCHEMA,
    variables: variables.sort((a, b) => a.id.localeCompare(b.id)),
    actions: actions.sort((a, b) => a.id.localeCompare(b.id)),
    invariants };
}
function makeChain() {
  const variables = [
    { id: 'a', domain: DOMAIN.slice(), initial: 2 },
    { id: 'b', domain: DOMAIN.slice(), initial: 0 },
    { id: 'c', domain: DOMAIN.slice(), initial: 0 }
  ];
  const actions = [...twoWay('a', 'b', 'move_ab'), ...twoWay('b', 'c', 'move_bc')]
    .sort((a, b) => a.id.localeCompare(b.id));
  const allowed = [];
  for (const a of DOMAIN) for (const b of DOMAIN) for (const c of DOMAIN) {
    if (a + b + c === 2) allowed.push([a, b, c]);
  }
  return { schema: SCHEMA, variables, actions,
    invariants: [{ id: 'supply_conservation', support: ['a', 'b', 'c'], allowed }] };
}
function makeMutual() {
  // Each invariant needs the OTHER invariant to be inductive. Circular use is
  // sound only when every conjunct is proved under a common simultaneous
  // induction, including the initial state.
  const domain = [0, 1];
  const table = fn => domain.flatMap(x => domain.map(y => ({
    at: [x, y], next: [fn(x, y)]
  })));
  return {
    schema: SCHEMA,
    variables: [
      { id: 'x', domain: domain.slice(), initial: 0 },
      { id: 'y', domain: domain.slice(), initial: 0 }
    ],
    actions: [
      { id: 'set_x', reads: ['x', 'y'], writes: ['x'], table: table((x, y) => y) },
      { id: 'set_y', reads: ['x', 'y'], writes: ['y'], table: table((x, y) => x) }
    ],
    invariants: [
      { id: 'x_zero', support: ['x'], allowed: [[0]] },
      { id: 'y_zero', support: ['y'], allowed: [[0]] }
    ]
  };
}
function mutualStrengthening() {
  return {
    schema: 'zeno-fcis/local-strengthening/experimental-1',
    bindings: [
      { invariant: 'x_zero', action: 'set_x', assuming: ['y_zero'] },
      { invariant: 'y_zero', action: 'set_y', assuming: ['x_zero'] }
    ]
  };
}

module.exports = { makePairs, makeChain, makeMutual, mutualStrengthening };
