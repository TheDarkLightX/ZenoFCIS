'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const { verify } = require('./proof_checker.cjs');
const { canonicalJSON } = require('./run_helpers.cjs');
function sha(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }
function main(argv) {
  if (argv.length !== 1 || argv[0].startsWith('-')) {
    console.error('usage: node run.cjs CANONICAL_MODEL.json');
    return 64;
  }
  try {
    const file = fs.statSync(argv[0]);
    if (!file.isFile() || file.size > 4 * 1024 * 1024) throw new Error('input must be a regular file <= 4 MiB');
    const raw = fs.readFileSync(argv[0], 'utf8');
    const m = JSON.parse(raw);
    if (raw !== canonicalJSON(m)) throw new Error('noncanonical JSON or duplicate keys');
    const report = verify(m);
    const bound = { ...report, model_sha256: sha(raw),
      checker_source_sha256: sha(fs.readFileSync(require.resolve('./proof_checker.cjs'))) };
    console.log(JSON.stringify(bound, null, 2));
    return report.status === 'proved' ? 0 : report.status === 'violated' ? 1 : 2;
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
