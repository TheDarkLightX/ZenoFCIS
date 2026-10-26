'use strict';
const fs = require('node:fs'), path = require('node:path');
const { makePairs, makeChain } = require('./models.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');
const dest = process.argv[2];
if (!dest) { console.error('usage: node make_examples.cjs OUTPUT_DIRECTORY'); process.exit(64); }
fs.mkdirSync(dest, { recursive: true });
for (const [name, packet] of [['pairs25.json', makePairs(25)], ['chain3.json', makeChain()]]) {
  fs.writeFileSync(path.join(dest, name), canonicalJSON(packet), { flag: 'wx' });
  console.log(path.join(dest, name));
}
