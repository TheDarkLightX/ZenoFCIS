'use strict';
// Frozen global goal and independently checked local-delta proof route.
// Does not authorize an application, executable, deployment or formal source refinement.
const fs = require('node:fs'), crypto = require('node:crypto');
const { checkLinear } = require('./linear_checker.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');
function hash(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }
function readCanonical(filename) {
  const info = fs.statSync(filename);
  if (!info.isFile() || info.size > 4 * 1024 * 1024)
    throw Error('input must be a regular file <= 4 MiB');
  const raw = fs.readFileSync(filename, 'utf8');
  const document = JSON.parse(raw);
  if (raw !== canonicalJSON(document))
    throw Error('noncanonical JSON or duplicate object keys');
  return { raw, document };
}
function main(argv) {
  if (argv.length !== 2 || argv.some(x => x.startsWith('-'))) {
    console.error('usage: node run_linear.cjs MODEL.json FROZEN_LINEAR_GOAL.json');
    return 64;
  }
  try {
    const m = readCanonical(argv[0]), goal = readCanonical(argv[1]);
    const report = checkLinear(m.document, goal.document);
    console.log(JSON.stringify({
      ...report, model_sha256: hash(m.raw), goal_sha256: hash(goal.raw),
      checker_source_sha256: hash(fs.readFileSync(require.resolve('./linear_checker.cjs'))),
      admission_checker_source_sha256: hash(fs.readFileSync(require.resolve('./proof_checker.cjs'))),
      publication_authority: false
    }, null, 2));
    return report.status === 'proved' ? 0 : report.status === 'violated' ? 1 : 2;
  } catch (error) {
    console.error(JSON.stringify({
      status: 'refused', claim_level: 'NONE', publication_authority: false,
      reason: String(error && error.message || error)
    }));
    return 65;
  }
}
if (require.main === module) process.exitCode = main(process.argv.slice(2));
module.exports = { main };
