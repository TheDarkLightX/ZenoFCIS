// Domain idioms: components with role signatures in data (zenoui/idiom/1). The search may select an
// idiom only where the contract's roles match its signature; the whole-tree checker verifies every
// use. Idioms follow common product conventions described in words; they copy no product's code.
export const IDIOM_FORMAT = 'zenoui/idiom/1';
// Input roles that ui.json may give command inputs, as meaning only.
export const INPUT_ROLES = Object.freeze(['amount-in','min-out','requested-value']);
export const IDIOMS = Object.freeze([
  // State displays: every displayed state input sits in exactly one display idiom.
  {format:IDIOM_FORMAT,id:'table-list',region:'state',accepts:['table-entry'],min:1,max:64},
  {format:IDIOM_FORMAT,id:'stepper',region:'state',accepts:['stage'],min:1,max:1},
  {format:IDIOM_FORMAT,id:'metrics',region:'state',accepts:['amount'],min:1,max:12},
  {format:IDIOM_FORMAT,id:'lanes',region:'state',accepts:['lane'],min:2,max:24},
  {format:IDIOM_FORMAT,id:'facts',region:'state',accepts:['stage','status','amount','flag','identity','lane','approval-status'],min:1,max:64},
  // Command idiom: a swap card over one or two commands with the same inputs: an amount in, a
  // minimum out, and the amount out projected from the core's quote preview.
  {format:IDIOM_FORMAT,id:'swap-card',region:'commands',inputs:{'amount-in':1,'min-out':1},projections:{'accepted-decrease':1},commands:[1,2]},
  {format:IDIOM_FORMAT,id:'scalar-edit',region:'commands',inputs:{'requested-value':1},commands:[1,1]},
  {format:IDIOM_FORMAT,id:'approval-decision',region:'commands',inputs:{},commands:[2,2]},
]);
export const idiom=id=>IDIOMS.find(i=>i.id===id);
// The display idiom that fits a role best; "facts" is the generic fallback.
export function bestDisplay(role) {
  return role==='stage'?'stepper':role==='amount'?'metrics':role==='lane'?'lanes':'facts';
}
// Deterministic partition of the displayed state inputs into best-fit display idioms, in input order
// of each idiom's first field. A lone lane cannot form lanes, so it falls back to facts.
export function displayPlan(app) {
  const entries=new Set((app.tables??[]).flatMap(t=>t.entries));
  const shown=app.fields.filter(f=>f.source==='state'&&!entries.has(f.path));
  const lanes=new Set((app.lanes??[]).flatMap(g=>[g.lane,...g.members.map(m=>m.index)]));
  const roleOf=f=>lanes.has(f.index)?'lane':f.role;
  const groups=new Map();
  for (const f of shown) {
    let id=bestDisplay(roleOf(f));
    if (id==='stepper' && groups.has('stepper')) id='facts';
    if (!groups.has(id)) groups.set(id,[]);
    groups.get(id).push(f.path);
  }
  if (groups.has('lanes') && groups.get('lanes').length<2) { groups.set('facts',[...(groups.get('facts')??[]),...groups.get('lanes')]);groups.delete('lanes'); }
  const order=['stepper','facts','metrics','lanes'];
  return [...(app.tables??[]).map(t=>({kind:'display',idiom:'table-list',fields:t.entries})),...order.filter(id=>groups.has(id)).map(id=>({kind:'display',idiom:id,fields:groups.get(id)}))];
}
// Whether a display idiom accepts these state inputs.
export function displayFits(app, id, paths) {
  const spec=idiom(id);
  if (!spec || spec.region!=='state' || !Array.isArray(paths) || paths.length<spec.min || paths.length>spec.max) return false;
  if(id==='table-list')return (app.tables??[]).some(t=>JSON.stringify(t.entries)===JSON.stringify(paths));
  if(paths.some(p=>(app.tables??[]).some(t=>t.entries.includes(p))))return false;
  const lanes=new Set((app.lanes??[]).flatMap(g=>[g.lane,...g.members.map(m=>m.index)]));
  return paths.every(p=>{const f=app.fields.find(x=>x.path===p);return f && f.source==='state' && spec.accepts.includes(lanes.has(f.index)?'lane':f.role);});
}
// Whether a command scope fits the swap card: its commands share inputs with roles amount-in and
// min-out, and one quote projection covers each of them.
export function swapFits(app, requirements, commands, inputs) {
  const spec=idiom('swap-card');
  if (commands.length<spec.commands[0] || commands.length>spec.commands[1]) return false;
  const roles=inputs.map(p=>app.fields.find(f=>f.path===p)?.role);
  const quotes=requirements.projections.filter(p=>p.kind==='accepted-decrease'&&commands.every(c=>Object.hasOwn(p.fieldByCommand??{},c)));
  return roles.length===2 && roles.filter(r=>r==='amount-in').length===1 && roles.filter(r=>r==='min-out').length===1 && quotes.length===1;
}

// These declarations are presentation hints, not claims about the policy's effects.
export function editFits(app, requirements, commands, inputs, scope) {
  if(scope==='selected'||commands.length!==1||inputs.length!==1||requirements.projections.length) return false;
  const command=app.commands.find(c=>c.id===commands[0]),value=app.fields.find(f=>f.path===inputs[0]);
  const current=app.fields.find(f=>f.path===value?.stateField);
  const inLane=(app.lanes??[]).some(g=>g.lane===current?.index||g.members.some(m=>m.index===current?.index));
  return command?.uiRole==='update'&&command.consequential===true&&
    value?.source==='command'&&value.role==='requested-value'&&value.domain.kind==='int'&&
    requirements.primaryFields.includes(value.path)&&current?.source==='state'&&current.domain.kind==='int'&&
    current.unit===value.unit&&(current.decimals??0)===(value.decimals??0)&&!inLane;
}
export function approvalFits(app, requirements, commands, inputs) {
  const pair=commands.map(id=>app.commands.find(c=>c.id===id));
  const statuses=app.fields.filter(f=>f.source==='state'&&f.role==='approval-status');
  const inLane=(app.lanes??[]).some(g=>g.lane===statuses[0]?.index||g.members.some(m=>m.index===statuses[0]?.index));
  return app.commands.length===2&&commands.length===2&&inputs.length===0&&requirements.projections.length===0&&
    pair.every(c=>c?.consequential===true)&&pair.filter(c=>c.uiRole==='approve').length===1&&
    pair.filter(c=>c.uiRole==='reject').length===1&&statuses.length===1&&!inLane;
}
