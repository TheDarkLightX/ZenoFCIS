import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import * as core from '../public/zenoui/presentation.js';
import {project,validateContract} from '../public/zenoui/semantic.js';
import {GRID,checkLayout,hitTest} from '../public/factory-layout.js';
const native=JSON.parse(readFileSync(new URL('harness/factory-native.json',import.meta.url)));
const finite=JSON.parse(readFileSync(new URL('presentation-finite.json',import.meta.url)));
const contract=JSON.parse(readFileSync(new URL('../public/zenoui/inventory.semantic.json',import.meta.url)));
const source=readFileSync(new URL('../public/zenoui/presentation.js',import.meta.url),'utf8');
const copy=x=>structuredClone(x);
const run=(state,event)=>core.step(state,event);
function started(act='inventory/none',preferences){return run(core.initial('7',preferences),{type:'Start',act,draft:act.startsWith('inventory/')?{mutant:act.split('/')[1]}:act==='money'?{amount:'1000',fee_bps:'30',reserve_in:'10000',reserve_out:'20000'}:{}});}
function finished(act='inventory/none',preferences){const s=started(act,preferences);return run(s.state,{type:'Result',identity:s.state.identity,result:native[act],elapsed:1});}
async function mutant(from,to){assert.ok(source.includes(from),'mutation site exists');return import('data:text/javascript;base64,'+Buffer.from(source.replace(from,to)).toString('base64'));}
function killed(check,candidate){assert.throws(()=>check(candidate),assert.AssertionError);}
function law1(module){const v=module.view(module.initial());for(const task of Object.values(v.tasks)){assert.deepEqual(task.regions,['instrument','metrics','comparison','record'],'L1 complete_start');assert.equal(task.result,null);assert.equal(task.evidence.checked,0);assert.equal(task.verdict,null);assert.ok(task.metrics.length);}}
test('L1_complete_start',async()=>{law1(core);killed(law1,await mutant("['instrument','metrics','comparison','record']","['instrument','metrics','comparison']"));});
function law2(module){
 const begin=module.step(module.initial('9'),{type:'Start',act:'search'}),old=begin.state.identity;
 for(const field of ['epoch','run','request'])for(const type of ['Result','Progress','Fail']){
  const identity={...old,[field]:'999'};
  const result=module.step(begin.state,{type,identity,result:native.search,candidate:native.synthesis.candidates[0],message:'late',elapsed:1});assert.equal(result.state,begin.state,`L2 stale ${field} ${type}`);assert.deepEqual(result.intents,[]);
 }
 const cancel=module.step(begin.state,{type:'Cancel'}),restart=module.step(cancel.state,{type:'Start',act:'search'});
 assert.equal(module.step(restart.state,{type:'Result',identity:old,result:native.search,elapsed:1}).state,restart.state,'L2 old epoch after restart');
}
test('L2_run_identity',async()=>{
 law2(core);killed(law2,await mutant("['epoch','run','request'].every", "['run','request'].every"));
 let s=finished().state;const identity=s.identity;
 s=run(s,{type:'Select',identity,ordinal:0}).state;const old=s.selection.identity;
 s=run(s,{type:'Select',identity,ordinal:143}).state;
 const detail=JSON.parse(readFileSync(new URL('harness/inventory-details-native.json',import.meta.url)))['none/0'];
 assert.equal(run(s,{type:'CellResult',identity:old,ordinal:0,result:detail}).state,s,'L2 old cell after a new selection');
 assert.equal(run(s,{type:'CellResult',identity:s.selection.identity,ordinal:0,result:detail}).state,s,'L2 ordinal mismatch');
 assert.equal(run(s,{type:'Select',identity:{...identity,run:'0'},ordinal:0}).state,s,'L2 detached grid');
});
function meaning(module,preferences){
 let s=module.initial('0',preferences);const observations=[];
 for(const act of ['synthesis','inventory/none','search','money']){
  const draft=act.startsWith('inventory/')?{mutant:'none'}:act==='money'?{amount:'1000',fee_bps:'30',reserve_in:'10000',reserve_out:'20000'}:{};
  let r=module.step(s,{type:'Start',act,draft});s=r.state;observations.push(r.intents);
  if(act==='synthesis'){r=module.step(s,{type:'Progress',identity:s.identity,candidate:native.synthesis.candidates[0]});s=r.state;observations.push([s.phase,s.tasks.synthesis.evidence]);}
  r=module.step(s,{type:'Result',identity:s.identity,result:native[act],elapsed:1});s=r.state;
  observations.push([s.phase,module.view(s).tasks[act.split('/')[0]],r.intents]);
  if(act==='inventory/none'){r=module.step(s,{type:'Select',identity:s.identity,ordinal:0});s=r.state;observations.push([s.selection,r.intents]);}
 }
 return observations;
}
function law3(module){const expected=meaning(module,{motion:'normal',theme:'light',width:1440});for(const motion of ['normal','reduce'])for(const theme of ['light','dark'])for(const width of [320,390,1440])assert.deepEqual(meaning(module,{motion,theme,width}),expected,'L3 preferences_noninterference');}
test('L3_preferences_never_change_meaning',async()=>{law3(core);killed(law3,await mutant('  const intents=[];',"  const intents=[];if(state.preferences.motion==='reduce'&&event.type==='Progress')return {state,intents,view:view(state)};"));});
test('L4_layout_declared_once',()=>{assert.equal(checkLayout(),864);assert.equal(hitTest(0,0),null);assert.throws(()=>checkLayout({...GRID,groupStep:0}),/layout_overlap|layout_hit_inverse/);assert.throws(()=>checkLayout({...GRID,width:50}),/layout_bounds/);});
function law5(module){for(const key of ['checked','total','passed','differences','law_checks','elapsed','assignments','refuted','nodes','work','quantity','available','reserved','authorized','action','reason','law','fee','fee_bps','net','out','reserve_in','reserve_out','product','steps','boolean_input','boolean_output'])assert.ok(module.descriptor(key).unit,'L5 names_types_units');const wide='340282366920938463463374607431768211455';assert.equal(module.count(wide),BigInt(wide).toLocaleString('en-US'),'L5 wide_integer_exact');assert.throws(()=>module.count(Number.MAX_SAFE_INTEGER+1));}
test('L5_names_types_units',async()=>{
 law5(core);assert.throws(()=>core.descriptor('available',{...core.DESCRIPTORS,available:{label:'Available',type:'integer'}}),/Missing value descriptor/);
 killed(law5,await mutant("return BigInt(value).toLocaleString('en-US');","return Number(value).toLocaleString('en-US');"));
});
function law6(module){assert.equal(module.evidence(48,864).verdict,null,'L6 partial_has_no_verdict');const b=module.step(module.initial(),{type:'Start',act:'search'});const timeout=module.step(b.state,{type:'Fail',identity:b.state.identity,message:'Timed out'});assert.equal(timeout.view.tasks.search.verdict,null,'L6 timeout_has_no_verdict');assert.equal(timeout.view.tasks.search.result,null);assert.match(timeout.view.tasks.search.status,/No verdict/);}
test('L6_claims_come_from_evidence',async()=>{law6(core);assert.throws(()=>core.evidence(48,864,true),/Invalid evidence counts/);killed(law6,await mutant("verdict:claimGate(checked,total,complete,false,complete?'finite checked':'tested')?'complete':null","verdict:'complete'"));});
const table=finite.enablement.rows;
function law7(candidate){
 const witnesses={},omissions={};let contexts=0;
 for(const cell of native['inventory/none'].grid.cells){
  const [available,reserved,action,quantity,authorized]=cell.input;
  const actionName={150:'Reserve',151:'Release',152:'Ship',153:'Restock'}[action];
  for(const busy of [false,true])for(const fault of [false,true])for(const task of candidate.tasks){
   const view=project(candidate,{state:{available,reserved},steps:0},{task:task.id,input:{quantity,authorized:authorized===1},busy,fault});
   const enabled=view.actions.find(a=>a.id===actionName).enabled;
   const accepted=cell.class==='Accept';
   const row=table.find(([input])=>input[0]===0&&input[1]===Number(enabled)&&input[2]===Number(accepted));
   assert.ok(row,'L7 pair present in finite contract');assert.equal(row[1],'Accept',`L7 never_offer_refused at ${JSON.stringify(cell.input)}`);
   if(enabled)witnesses[actionName]??=cell.input;
   if(accepted&&!enabled&&!busy&&!fault&&task.id===actionName.toLowerCase())(omissions[actionName]??=[]).push(cell.input);
   contexts++;
  }
 }
 assert.deepEqual(Object.keys(witnesses).sort(),['Release','Reserve','Restock','Ship']);
 assert.deepEqual(Object.keys(omissions),['Restock'],'only declared conservative restock omissions');
 return {contexts,witnesses,omissions:omissions.Restock.length};
}
test('L7_never_offer_what_the_core_refuses',()=>{
 validateContract(contract);console.log('L7 separate inventory projection over full real-authority domain',law7(contract));
 const bad=copy(contract);bad.actions.find(a=>a.id==='Reserve').when.args.pop();killed(law7,bad);
});
test('finite_contract_runtime_refinement',()=>{
 assert.equal(finite.lifecycle.domain,55296);
 for(const [input,decision,post] of finite.lifecycle.rows){
  assert.equal(decision,'Accept');const [phase,selection,mask,event,...flags]=input;
  const actual=core.control({phase,selection,mask},event,flags.slice(0,4).map(Boolean),Boolean(flags[4]));
  assert.deepEqual([actual.phase,actual.selection,actual.mask],post,'finite lifecycle / mounted JS control');
 }
 for(const [input,decision,post] of finite.claims.rows){
  assert.equal(decision,'Accept');const [,checked,total,complete,timeout,scope]=input;
  assert.equal(post[0],Number(core.claimGate(checked,total,complete===1,timeout===1,scope===1?'finite checked':'tested')),'finite claim gate');
 }
});
test('concrete_lifecycle_refines_the_checked_control',()=>{
 const details=JSON.parse(readFileSync(new URL('harness/inventory-details-native.json',import.meta.url)));
 let state=core.initial('11');
 function apply(event,ready=true){
  const pre=core.abstractState(state),target=event.type==='CellResult'?state.selection?.identity:state.identity;
  const matches=['epoch','run','request'].map(k=>!!target&&event.identity?.[k]===target[k]);matches.push(event.ordinal===state.selection?.ordinal);
  const index=core.EVENTS.indexOf(event.type),expected=core.control(pre,index,matches,ready),result=core.step(state,event);
  assert.deepEqual(core.abstractState(result.state),expected,`mounted reducer / finite control: ${event.type}`);
  state=result.state;return result;
 }
 apply({type:'Start',act:'inventory/none',draft:{mutant:'none'}});
 const identity=state.identity;
 apply({type:'Progress',identity,progress:{...native['inventory/none'].grid,start:0,cells:native['inventory/none'].grid.cells.slice(0,48),names:native['inventory/none'].names}});
 apply({type:'Result',identity:{...identity,epoch:'0'},result:native['inventory/none'],elapsed:1});
 apply({type:'Result',identity,result:native['inventory/none'],elapsed:1});
 apply({type:'Select',identity,ordinal:0});const old=state.selection.identity;
 apply({type:'Select',identity,ordinal:143});
 apply({type:'CellResult',identity:old,ordinal:0,result:details['none/0']});
 apply({type:'CellResult',identity:state.selection.identity,ordinal:143,result:details['none/143']});
 apply({type:'PreferencesChanged',preferences:{width:390,motion:'reduce',theme:'dark'}});
 apply({type:'Cancel'});
 apply({type:'Start',act:'search'});
 apply({type:'Fail',identity:state.identity,message:'Timed out'});
});
test('ownership_rejection_and_mounted_worker_protocol',()=>{
 const start=started('search'),draft={};assert.ok(Object.isFrozen(start.state.tasks.search));
 const payload=copy(native.search);const done=run(start.state,{type:'Result',identity:start.state.identity,result:payload,elapsed:1});payload.table[0].output[0]=99;assert.notEqual(done.state.tasks.search.result.table[0].output[0],99);
 assert.throws(()=>{done.view.tasks.search.result.table[0].output[0]=99;},TypeError);
 assert.equal(run(done.state,{type:'Result',identity:start.state.identity,result:native.search,elapsed:1}).state,done.state,'duplicate result is no-op');
 assert.equal(run(start.state,{type:'Start',act:'search',draft}).state,start.state,'single flight refusal is no-op');
 const worker=readFileSync(new URL('../public/factory-worker.js',import.meta.url),'utf8');assert.match(worker,/identity/);assert.match(worker,/ordinal/);
 for(const result of Object.values(native)){
  if(!result.act||result.act==='candidate')continue;
  const act=result.act==='inventory'?`inventory/${result.mutant}`:result.act;
  assert.equal(finished(act).state.phase,'done');
 }
});
