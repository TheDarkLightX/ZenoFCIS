'use strict';
// Separate untrusted producer: can write candidates, never accept or authorize them.
const fs = require('node:fs');
const { canonicalJSON } = require('./run_helpers.cjs');
const { propose } = require('./propose_invariants.cjs');

function main(argv) {
  if (argv.length !== 2 || argv.some(a => a.startsWith('-'))) {
    console.error('usage: node propose.cjs CANONICAL_INPUT.json NEW_CANDIDATE.json');
    return 64;
  }
  try {
    const stat = fs.statSync(argv[0]);
    if (!stat.isFile() || stat.size > 4 * 1024 * 1024) throw new Error('input must be a regular file <= 4 MiB');
    const raw = fs.readFileSync(argv[0], 'utf8');
    const original = JSON.parse(raw);
    if (raw !== canonicalJSON(original)) throw new Error('noncanonical JSON or duplicate keys');
    const reply = propose(original);
    if (reply.proposals.length === 0 || reply.unsupported.length !== 0) {
      console.error(JSON.stringify({
        status: 'inconclusive', proposals: reply.proposals, unsupported: reply.unsupported,
        publication_authority: false
      }));
      return 2;
    }
    fs.writeFileSync(argv[1], canonicalJSON(reply.model), { flag: 'wx' });
    console.error(JSON.stringify({
      status: 'candidate-only', candidate_path: argv[1],
      candidates: reply.proposals.length, publication_authority: false,
      instruction: 'Run run.cjs against the candidate; proposal is NOT a proof.'
    }));
    return 0;
  } catch (err) {
    console.error(JSON.stringify({
      status: 'refused', publication_authority: false,
      reason: String(err && err.message || err)
    }));
    return 65;
  }
}
if (require.main === module) process.exitCode = main(process.argv.slice(2));
module.exports = { main };
