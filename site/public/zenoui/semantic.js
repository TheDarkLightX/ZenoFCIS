// ZenoUI semantic contract v1: deterministic policy evaluation, never authority.
// Inputs and output are plain data. No DOM, network, clock, eval, or effects.
export const FORMAT = 'zenoui/semantic-contract/1';
const ID = /^[a-z][a-z0-9_]*$/;
const OPS = new Set(['all', 'any', 'not', 'eq', 'gte', 'lte', 'gt', 'lt', 'truthy']);
const keys = (o) => Object.keys(o).sort();
function record(o) { return o !== null && typeof o === 'object' && !Array.isArray(o) && (Object.getPrototypeOf(o) === Object.prototype || Object.getPrototypeOf(o) === null); }
function exact(o, allowed, required = allowed) {
  if (!record(o) || keys(o).some(k => !allowed.includes(k)) || required.some(k => !Object.hasOwn(o,k))) throw new Error('invalid semantic contract object');
}
function unique(values) { if (new Set(values).size !== values.length) throw new Error('duplicate semantic identifier'); }
function name(value) { if (typeof value !== 'string' || !ID.test(value)) throw new Error('invalid identifier'); return value; }
function label(value) { if (typeof value !== 'string' || value.length < 1 || value.length > 300) throw new Error('invalid label'); return value; }
function source(node, fields, depth = 0) {
  if (depth > 24) throw new Error('operand too deep');
  if (record(node) && node.op === 'add') {
    exact(node, ['op','args']);
    if (!Array.isArray(node.args) || node.args.length !== 2) throw new Error('invalid add operand');
    node.args.forEach(arg => source(arg, fields, depth+1));
    return;
  }
  exact(node, ['ref','value'], []);
  if (Object.keys(node).length !== 1) throw new Error('operand must have exactly one source');
  if (Object.hasOwn(node,'ref')) {
    if (typeof node.ref !== 'string' || !fields.has(node.ref)) throw new Error(`unknown source ${String(node.ref)}`);
  } else if (node.value !== null && !['number','boolean','string'].includes(typeof node.value)) {
    throw new Error('non-scalar operand');
  } else if (typeof node.value === 'number' && !Number.isSafeInteger(node.value)) throw new Error('non-integer operand');
}
function expression(x, fields, depth = 0) {
  if (depth > 24) throw new Error('expression too deep');
  exact(x, ['op','args']);
  if (!OPS.has(x.op) || !Array.isArray(x.args)) throw new Error('unknown expression');
  const arity = x.op === 'not' || x.op === 'truthy' ? 1 : (x.op === 'all' || x.op === 'any' ? -1 : 2);
  if ((arity === -1 && (x.args.length < 1 || x.args.length > 20)) || (arity !== -1 && x.args.length !== arity)) throw new Error('invalid expression arity');
  if (['all','any','not'].includes(x.op)) x.args.forEach(a => expression(a, fields, depth + 1));
  else x.args.forEach(a => source(a, fields));
}
export function validateContract(c) {
  exact(c, ['format','name','state','inputs','tasks','actions','invariants','authority'], ['format','name','state','inputs','tasks','actions','invariants','authority']);
  if (c.format !== FORMAT) throw new Error('unsupported semantic-contract version');
  name(c.name);
  exact(c.authority,['profile','description']);
  label(c.authority.profile); label(c.authority.description);
  if (!Array.isArray(c.state) || !Array.isArray(c.inputs) || !Array.isArray(c.tasks) || !Array.isArray(c.actions) || !Array.isArray(c.invariants)) throw new Error('invalid contract collections');
  if (c.state.length > 32 || c.inputs.length > 24 || c.tasks.length < 1 || c.tasks.length > 24 || c.actions.length > 32 || c.invariants.length > 32) throw new Error('contract size exceeded');
  c.state.forEach(name); unique(c.state);
  c.inputs.forEach(f=>{
    exact(f,['id','label','kind','min','max','initial'],['id','label','kind','initial']);
    name(f.id); label(f.label);
    if (f.kind === 'integer') {
      if (!Number.isSafeInteger(f.min) || !Number.isSafeInteger(f.max) || f.min > f.max || f.max - f.min > 100000 || !Number.isSafeInteger(f.initial) || f.initial < f.min || f.initial > f.max) throw new Error('invalid integer input');
    } else if (f.kind === 'boolean') {
      if (Object.hasOwn(f,'min') || Object.hasOwn(f,'max') || typeof f.initial !== 'boolean') throw new Error('invalid boolean input');
    } else throw new Error('unsupported input type');
  });
  unique(c.inputs.map(f=>f.id));
  const sources = new Set([...c.state.map(s=>`state.${s}`),...c.inputs.map(f=>`input.${f.id}`), 'context.busy', 'context.fault']);
  c.tasks.forEach(t=>{exact(t,['id','label','description']); name(t.id);label(t.label);label(t.description);});
  unique(c.tasks.map(t=>t.id));
  const tasks = new Set(c.tasks.map(t=>t.id));
  c.actions.forEach(a=>{
    exact(a,['id','label','task','when','explanation','inputs']);
    if (typeof a.id !== 'string' || !/^[A-Z][A-Za-z0-9]*$/.test(a.id)) throw new Error('invalid action');
    label(a.label); label(a.explanation);
    if (!tasks.has(a.task)) throw new Error('unknown action task');
    if (!Array.isArray(a.inputs)) throw new Error('invalid action inputs');
    a.inputs.forEach(s=>{ if (!c.inputs.some(f=>f.id===s)) throw new Error('unknown action input'); });
    unique(a.inputs);
    expression(a.when,sources);
  });
  unique(c.actions.map(a=>a.id));
  unique(c.invariants);
  for (const x of c.invariants) if (!['confirmation_requires_commit','no_ui_authority','disabled_actions_explained','all_tasks_discoverable'].includes(x)) throw new Error('unknown UI invariant');
  return c;
}
export function canonicalContract(c) {
  validateContract(c);
  const visit = x => Array.isArray(x) ? x.map(visit) : record(x) ? Object.fromEntries(keys(x).map(k=>[k,visit(x[k])])) : x;
  return JSON.stringify(visit(c));
}
function value(node, env) {
  if (node.op === 'add') {
    const left = value(node.args[0],env), right = value(node.args[1],env);
    if (!Number.isSafeInteger(left) || !Number.isSafeInteger(right) || !Number.isSafeInteger(left+right)) return Number.NaN;
    return left+right;
  }
  if (Object.hasOwn(node,'value')) return node.value;
  const [scope,key] = node.ref.split('.');
  return env[scope][key];
}
function evaluate(x, env) {
  const a = x.args;
  switch(x.op) {
    case 'all': return a.every(v=>evaluate(v,env));
    case 'any': return a.some(v=>evaluate(v,env));
    case 'not': return !evaluate(a[0],env);
    case 'eq': return value(a[0],env) === value(a[1],env);
    case 'gte': return value(a[0],env) >= value(a[1],env);
    case 'lte': return value(a[0],env) <= value(a[1],env);
    case 'gt': return value(a[0],env) > value(a[1],env);
    case 'lt': return value(a[0],env) < value(a[1],env);
    case 'truthy': return value(a[0],env) === true;
    default: throw new Error('unknown operation');
  }
}
function spokenOperand(x) {
  if (x.op === 'add') return `(${x.args.map(spokenOperand).join(' plus ')})`;
  if (Object.hasOwn(x,'ref')) return x.ref.replace('.', ' ');
  return JSON.stringify(x.value);
}
function spokenGuard(g) {
  if (g.op === 'all' || g.op === 'any') return '(' + g.args.map(spokenGuard).join(g.op === 'all' ? ' AND ' : ' OR ') + ')';
  if (g.op === 'not') return `NOT ${spokenGuard(g.args[0])}`;
  if (g.op === 'truthy') return `${spokenOperand(g.args[0])} is true`;
  const compare = {eq:'equals',gte:'is at least',lte:'is at most',gt:'is greater than',lt:'is less than'};
  return `${spokenOperand(g.args[0])} ${compare[g.op]} ${spokenOperand(g.args[1])}`;
}
export function readback(c) {
  validateContract(c);
  return [
    `ZenoUI semantic contract ${c.name}, ${c.format}.`,
    `Application authority: ${c.authority.profile}. ${c.authority.description}`,
    ...c.tasks.map(t=>`Task ${t.id}: ${t.label}. ${t.description}`),
    ...c.actions.map(a=>`Action ${a.id} belongs to ${a.task}. Suggest enabling when ${spokenGuard(a.when)}. When unavailable explain: ${a.explanation} Eligibility is advisory, never authorization.`),
    ...c.invariants.map(i=>`Obligation: ${i.replaceAll('_',' ')}.`),
  ].join('\n');
}
// Presentation Core. Only `snapshot.state` is sourced from the application.
// User intent and context never supply business authorization.
export function project(c, snapshot, ui) {
  validateContract(c);
  if (!record(snapshot) || !record(snapshot.state) || !record(ui) || !record(ui.input) || !c.tasks.some(t=>t.id===ui.task)) throw new Error('invalid projection inputs');
  const state = Object.fromEntries(c.state.map(k=>{
    if (!Object.hasOwn(snapshot.state,k) || !Number.isSafeInteger(snapshot.state[k])) throw new Error(`missing authoritative state ${k}`);
    return [k,snapshot.state[k]];
  }));
  const input = Object.fromEntries(c.inputs.map(f=>{
    const x = ui.input[f.id];
    if (f.kind === 'integer' && (!Number.isSafeInteger(x) || x < f.min || x > f.max)) return [f.id, null];
    if (f.kind === 'boolean' && typeof x !== 'boolean') return [f.id,null];
    return [f.id,x];
  }));
  const busy = ui.busy === true, fault = ui.fault === true;
  const env = {state,input,context:{busy,fault}};
  const actions = c.actions.map(a=>{
    const chosen = a.task === ui.task;
    const eligible = a.inputs.every(k=>input[k] !== null) && evaluate(a.when,env);
    const enabled = chosen && eligible && !busy && !fault;
    const explanation = !chosen ? 'Select this task to propose an action.' : fault ? 'The authority state is uncertain; reconcile it first.'
      : busy ? 'An operation is in progress.' : eligible ? '' : a.explanation;
    return {id:a.id,task:a.task,label:a.label,chosen,enabled,eligible,explanation};
  });
  const tasks = c.tasks.map(t=>({id:t.id,label:t.label,description:t.description,selected:t.id===ui.task}));
  return {tasks,actions,state,inputs:input,task:ui.task,busy,fault,revision:snapshot.steps,showConfirmation:ui.outcome?.committed === true && ui.outcome?.decision === 'Accept',
    outcome:ui.outcome?.message ?? '', authorityLabel:c.authority.description};
}
