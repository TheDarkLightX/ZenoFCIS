'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const { checkGoals } = require('./goal_checker.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');
function sha(x) { return crypto.createHash('sha256').update(x).digest('hex'); }
function input(filename) {
  const stat = fs.statSync(filename);
  if (!stat.isFile() || stat.size > 4 * 1024 * 1024)
    throw new Error('input must be a regular file <= 4 MiB');
  const raw = fs.readFileSync(filename, 'utf8');
  const parsed = JSON.parse(raw);
  if (raw !== canonicalJSON(parsed)) throw new Error('noncanonical JSON or duplicate keys');
  return { parsed, raw };
}
function main(argv) {
  if (argv.length !== 2 || argv.some(s => s.startsWith('-'))) {
    console.error('usage: node run_goals.cjs INDUCTIVE_MODEL.json FROZEN_GOALS.json');
    return 64;
  }
  try {
    const model = input(argv[0]);
    const goals = input(argv[1]);
    const verdict = checkGoals(model.parsed, goals.parsed);
    console.log(JSON.stringify({
      ...verdict,
      model_sha256: sha(model.raw),
      goal_sha256: sha(goals.raw),
      checker_source_sha256: sha(fs.readFileSync(require.resolve('./goal_checker.cjs'))),
      induction_checker_source_sha256: sha(fs.readFileSync(require.resolve('./proof_checker.cjs'))),
      publication_authority: false
    }, null, 2));
    return verdict.status === 'proved' ? 0 : verdict.status === 'violated' ? 1 : 2;
  } catch (e) {
    console.error(JSON.stringify({
      status: 'refused', claim_level: 'NONE', publication_authority: false,
      reason: String(e && e.message || e)
    }));
    return 65;
  }
}
if (require.main === module) process.exitCode = main(process.argv.slice(2));
module.exports = { main };
