// Real stdio MCP interoperability, using the repository's pinned SDK.
// No model or account calls; terminal transcripts simulate explicit review.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const project = mkdtempSync(join(tmpdir(), 'zal-sdk-'));
const here = resolve('integrations/zal');
const python = process.env.ZAL_TEST_PYTHON || 'python3';
const environment = { ...process.env, PYTHONDONTWRITEBYTECODE: '1' };
const sha = value => createHash('sha256').update(value).digest('hex');
const client = new Client({ name: 'zal-harness-test', version: '1.0.0' });
let transport;
const timer = setTimeout(() => { console.error('ZAL SDK test timed out'); process.exit(1); }, 30000);
function run(script, args, input) {
  return execFileSync(python, [join(here, script), ...args],
    { encoding: 'utf8', env: environment, input, timeout: 15000 });
}
async function call(name, args, expectError = false) {
  const result = await client.callTool({ name, arguments: args });
  assert.equal(result.isError === true, expectError, `${name}: ${result.content?.[0]?.text}`);
  assert.equal(result.content[0].type, 'text');
  return expectError ? result.content[0].text : JSON.parse(result.content[0].text);
}
try {
  run('setup.py', ['--project', project, '--harness', 'both', '--apply']);
  const configuration = JSON.parse(readFileSync(join(project, '.mcp.json'))).mcpServers.zal;
  const workspace = join(project, '.zeno-fcis/zal/workspace.json');
  const original = sha(readFileSync(workspace));
  transport = new StdioClientTransport({ command: configuration.command,
    args: configuration.args, env: environment, stderr: 'pipe' });
  await client.connect(transport);
  assert.equal(client.getServerVersion().name, 'zenofcis-zal');
  const expected = ['zal_read', 'zal_check', 'zal_explain', 'zal_help', 'zal_propose', 'zal_compare', 'zal_trace'];
  assert.deepEqual((await client.listTools()).tools.map(t => t.name).sort(), expected.sort());
  const read = await call('zal_read', {});
  const revision = read.model.revision;
  assert.equal(read.agreement, 'seed-example');
  assert.equal(read.authority, 'none');
  const checked = await call('zal_check', { revision });
  assert.equal(checked.status, 'pass-with-scope');
  assert.equal(checked.input_tuples, 24);
  assert.equal(checked.invariant_count, 0);
  assert.equal((await call('zal_help', { revision, topic: '' })).kind, 'language-help');
  assert.equal((await call('zal_explain', { revision, state: 'pending', event: 'approve',
    context: { authorized: false } })).outcome.class, 'Reject');
  const trace = await call('zal_trace', { revision, inputs: [
    { event: 'submit', context: { authorized: false } },
    { event: 'approve', context: { authorized: true } },
  ] });
  assert.equal(trace.trace[1].outcome.state, 'approved');
  const weakened = read.model.symbolic.replace('[authorized]', '[true]');
  assert.notEqual(weakened, read.model.symbolic);
  const compared = await call('zal_compare', { revision, syntax: 'symbolic', surface: weakened });
  assert.ok(compared.witnesses.length > 0);
  assert.equal(sha(readFileSync(workspace)), original, 'Read-only MCP calls changed the workspace');
  assert.match(await call('zal_check', { revision: 'stale' }, true), /Stale revision/);
  assert.match(await call('zal_accept', {}, true), /Unknown tool/);
  const proposed = await call('zal_propose', { base_revision: revision, object_ids: ['rule:approve_order'],
    message: 'Test: expose an authorization weakening for separate review.', syntax: 'symbolic', surface: weakened });
  assert.equal(proposed.model.revision, revision);
  assert.equal(proposed.agreement, 'seed-example');
  assert.notEqual(proposed.proposal.revision, revision);
  const transcript = run('terminal.py', ['review', '--workspace', workspace], 'candidate\nreject\nshow\naccept-current\nquit\n');
  assert.match(transcript, /human-accepted/);
  const accepted = await call('zal_read', {});
  assert.equal(accepted.model.revision, revision);
  assert.equal(accepted.proposal, null);
  assert.equal(accepted.agreement, 'human-accepted');
  const destination = join(project, 'declarations');
  const arguments_ = ['--workspace', workspace, '--revision', revision, '--out', destination];
  assert.equal(JSON.parse(run('workspace.py', ['export', ...arguments_])).factory_qualification, 'not-run');
  assert.equal(JSON.parse(run('workspace.py', ['verify', ...arguments_])).status, 'verified-reviewed-declarations');
  console.log('ZAL MCP SDK: 7 tools, stale refusal, weakening witness, separate review, reviewed export PASS');
} finally {
  clearTimeout(timer);
  await client.close();
  rmSync(project, { recursive: true, force: true });
}
