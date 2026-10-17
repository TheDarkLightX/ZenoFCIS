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
  const expected = ['zal_read', 'zal_check', 'zal_explain', 'zal_help', 'zal_propose', 'zal_compare', 'zal_trace',
    'zal_loop_read', 'zal_loop_packet', 'zal_loop_submit', 'zal_loop_translate', 'zal_loop_legend',
    'zal_capture_read', 'zal_capture_packet', 'zal_capture_submit', 'zal_capture_translate', 'zal_capture_legend'];
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
  // Explicit expectations below are regression fixtures, not supplied owner intent.
  const fixtureProvenance = 'Protocol regression fixture; not an owner approval';
  run('terminal.py', ['review', '--workspace', workspace],
    `label deny pending approve authorized=no expect Reject pending not_enabled "${fixtureProvenance}"\nquit\n`);
  const labelled = await call('zal_read', {});
  assert.equal(labelled.owner_examples[0].expected.class, 'Reject');
  assert.equal(labelled.owner_examples[0].provenance, fixtureProvenance);
  const labelledComparison = await call('zal_compare', { revision, syntax: 'symbolic', surface: weakened });
  assert.equal(labelledComparison.owner_examples.cases[0].current.status, 'matched');
  assert.equal(labelledComparison.owner_examples.cases[0].candidate.status, 'conflict');
  assert.deepEqual(labelledComparison.owner_examples.cases[0].expected, labelled.owner_examples[0].expected);
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
  run('terminal.py', ['review', '--workspace', workspace],
    `label cancel approved cancel authorized=yes expect Reject approved not_enabled "${fixtureProvenance}"\nquit\n`);
  assert.equal((await call('zal_read', {})).freshness.review.status, 'stale');
  const changedSurface = read.model.symbolic + '\nrule cancel_approved: approved + cancel [true] -> accept cancelled;\n';
  const proposalArgs = { base_revision: revision, object_ids: ['rule:approve_order'],
    message: 'Regression journey: explicit cancellation change; retain independent fixture expectations.',
    syntax: 'symbolic', surface: changedSurface };
  await call('zal_propose', proposalArgs);
  run('terminal.py', ['review', '--workspace', workspace],
    `label deny pending approve authorized=no expect Reject pending not_enabled "${fixtureProvenance}; second display"\nquit\n`);
  const staleCorpus = await call('zal_read', {});
  assert.equal(staleCorpus.freshness.proposal.corpus, 'stale');
  const beforeRefusal = sha(readFileSync(workspace));
  assert.match(run('terminal.py', ['review', '--workspace', workspace], 'candidate\naccept\nquit\n'),
    /Owner examples changed; propose again/);
  assert.equal(sha(readFileSync(workspace)), beforeRefusal, 'Refused stale-corpus acceptance changed workspace');
  await call('zal_propose', proposalArgs);
  assert.match(run('terminal.py', ['review', '--workspace', workspace], 'candidate\naccept\nquit\n'), /human-accepted/);
  const changed = await call('zal_read', {});
  assert.notEqual(changed.model.revision, revision);
  assert.equal(changed.freshness.review.status, 'current');
  assert.equal(changed.realization, 'not-run');
  assert.deepEqual(changed.authenticated_facts, []);
  const cancellation = changed.example_comparison.cases.find(c => c.id === 'cancel');
  assert.equal(cancellation.expected.class, 'Reject');
  assert.equal(cancellation.current.status, 'conflict');
  assert.equal(cancellation.provenance, fixtureProvenance);
  assert.match(await call('zal_explain', { revision, state: 'approved', event: 'cancel',
    context: { authorized: true } }, true), /Stale revision/);
  const changedTrace = await call('zal_trace', { revision: changed.model.revision, inputs: [
    { event: 'submit', context: { authorized: false } },
    { event: 'approve', context: { authorized: true } },
    { event: 'cancel', context: { authorized: true } },
  ] });
  assert.equal(changedTrace.trace[2].outcome.state, 'cancelled');
  assert.deepEqual(changedTrace.authenticated_facts, []);
  assert.deepEqual(changedTrace.supplied_assumptions.steps[2], { observations: { authorized: true } });
  const changedOut = join(project, 'changed-declarations');
  const changedArguments = ['--workspace', workspace, '--revision', changed.model.revision, '--out', changedOut];
  const exported = JSON.parse(run('workspace.py', ['export', ...changedArguments]));
  assert.equal(exported.owner_examples.cases.find(c => c.id === 'cancel').current.status, 'conflict');
  assert.equal(exported.authority, 'none');
  assert.equal(JSON.parse(run('workspace.py', ['verify', ...changedArguments])).status, 'verified-reviewed-declarations');
  console.log('ZAL MCP SDK: 7 tools, supplied fixture labels, revision change, stale-corpus refusal/recovery, trace and reviewed export PASS');
} finally {
  clearTimeout(timer);
  await client.close();
  rmSync(project, { recursive: true, force: true });
}
