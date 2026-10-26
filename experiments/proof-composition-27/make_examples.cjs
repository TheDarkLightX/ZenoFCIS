'use strict';
const fs = require('node:fs'), path = require('node:path');
const { makePairs, makeChain, makeMutual, mutualStrengthening } = require('./models.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');
const dest = process.argv[2];
if (!dest) { console.error('usage: node make_examples.cjs OUTPUT_DIRECTORY'); process.exit(64); }
fs.mkdirSync(dest, { recursive: true });
for (const [name, packet] of [
  ['pairs25.json', makePairs(25)], ['chain3.json', makeChain()],
  ['pairs25-unproven.json', { ...makePairs(25), invariants: [] }],
  ['chain3-unproven.json', { ...makeChain(), invariants: [] }],
  ['mutual.json', makeMutual()],
  ['mutual-strengthening.json', mutualStrengthening()],
  ['mutual-goals.json', { schema: 'zeno-fcis/local-goals/experimental-1',
    goals: [{ id: 'safety', support: ['x'], allowed: [[0]], using: ['x_zero'] }] }],
  ['chain3-owner-goals.json', {
    schema: 'zeno-fcis/local-goals/experimental-1',
    goals: [{ id: 'conserve_supply', support: ['a', 'b', 'c'],
      allowed: makeChain().invariants[0].allowed, using: ['auto_sum_000'] }]
  }]
]) {
  fs.writeFileSync(path.join(dest, name), canonicalJSON(packet), { flag: 'wx' });
  console.log(path.join(dest, name));
}
