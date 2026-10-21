// Independent review witnesses retained against the shipped reducer and page.
// The DOM adapter supplies no Chrome layout, raster or keyboard evidence.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import * as core from '../public/zenoui/presentation.js';
import {project} from '../public/zenoui/semantic.js';
import {GRID,checkLayout} from '../public/factory-layout.js';
import {instantiate} from '../public/demo-module.js';
import {runFactory} from '../public/factory-runner.js';
import {ORIGIN,documentFrom,moduleLoader} from './browser-fixture.mjs';
const json=file=>JSON.parse(readFileSync(new URL(file,import.meta.url)));
const native=json('./harness/factory-native.json'),details=json('./harness/inventory-details-native.json');
const contract=json('../public/zenoui/inventory.semantic.json');
const page=readFileSync(new URL('../public/index.html',import.meta.url),'utf8');
const source=readFileSync(new URL('../public/zenoui/presentation.js',import.meta.url),'utf8');
const money={amount:'1000',fee_bps:'30',reserve_in:'10000',reserve_out:'20000'};
const clone=x=>structuredClone(x);
const start=(act='search',draft=act==='money'?money:act.startsWith('inventory/')?{mutant:act.split('/')[1]}:{})=>core.step(core.initial('7'),{type:'Start',act,draft}).state;
const finish=(s,r)=>core.step(s,{type:'Result',identity:s.identity,result:r,elapsed:1});
function rejectResult(state,result){
 const before=JSON.stringify(state);assert.throws(()=>finish(state,result));
 assert.equal(JSON.stringify(state),before,'refusal preserves the pure pre-state');
 assert.equal(core.view(state).tasks[state.act].verdict,null,'no result verdict before admission');
}
async function mount(transform=(_u,s)=>s){
 const messages=[],timers=[];let worker;
 class Worker {constructor(){worker=this;}postMessage(m){messages.push(structuredClone(m));}terminate(){this.terminated=true;}}
 const document=documentFrom(page),create=document.createElement;
 document.createElement=tag=>{
  const node=create(tag);node.scrollLeft=0;node.clientWidth=640;
  if(tag==='canvas')node.getContext=()=>({fillRect(){},fillText(){},strokeRect(){},beginPath(){},arc(){},stroke(){},setLineDash(){}});
  return node;
 };
 const loader=moduleLoader({document,Worker,innerWidth:1440,matchMedia:()=>({matches:false,addEventListener(){},removeEventListener(){}}),
  getComputedStyle:()=>({getPropertyValue:()=> '#333333'}),setTimeout:f=>{timers.push(f);return timers.length;},clearTimeout(){}},transform);
 await loader.evaluate(`${ORIGIN}/ZenoFCIS/factory.js`);
 return {root:document.getElementById('factory'),messages,timers,get worker(){return worker;},
  deliver(data,request=messages.findLast(m=>m.kind==='run')){worker.onmessage({data:{identity:request.identity,...data}});}};
}
function edit(ui,key,value,notify=true){const input=ui.root.querySelector(`input[data-money="${key}"]`);input.value=value;if(notify)input.dispatchEvent(new Event('input'));}
const button=(ui,act)=>ui.root.querySelector(`button[data-act="${act}"]`);
const status=(ui,act)=>ui.root.querySelector(`[data-factory-act="${act}"] .factory-status`).textContent;

// P1: the two exact review witnesses, plus the same failure family in other acts.
test('L6_review_narrowed_search_and_zero_passed_never_promote_a_verdict',()=>{
 const running=start();assert.equal(finish(running,native.search).view.tasks.search.verdict,'finite checked');
 rejectResult(running,{...native.search,domain:1,passed:1,table:native.search.table.slice(0,1)});
 rejectResult(running,{...native.search,passed:0});
});
test('L6_fixed_domains_coverage_counts_and_request_binding',()=>{
 const narrowed=clone(native.synthesis);narrowed.domain=1;narrowed.passed=1;narrowed.candidates.at(-1).checked=1;
 rejectResult(start('synthesis'),narrowed);
 for(const act of ['search','synthesis','inventory/none']){
  const bad=clone(native[act]);bad.passed=0;rejectResult(start(act),bad);
 }
 for(const change of [r=>r.table[1].input=r.table[0].input,r=>r.table.pop(),r=>r.table[0].output=[2,0,0],r=>delete r.table[0],r=>delete r.table[0].input[0],r=>r.table[0].input[0]=false]){
  const bad=clone(native.search);change(bad);rejectResult(start(),bad);
 }
 const duplicate=clone(native['inventory/none']);duplicate.grid.cells[1].input=duplicate.grid.cells[0].input;
 rejectResult(start('inventory/none'),duplicate);
 for(const change of [r=>delete r.grid.cells[0],r=>r.grid.cells[0].input[2]='150',r=>r.grid.cells[0].input[3]='1']){
  const bad=clone(native['inventory/none']);change(bad);rejectResult(start('inventory/none'),bad);
 }
 rejectResult(start('inventory/m0002'),native['inventory/m0018']);
 const candidates=clone(native.synthesis);candidates.candidates.pop();rejectResult(start('synthesis'),candidates);
 const missing=clone(native.synthesis);missing.assignments=1;rejectResult(start('synthesis'),missing);
 rejectResult(start('money',{...money,amount:'2000'}),native.money);
});
test('L6_mounted_rejected_payload_fails_closed_and_late_success_cannot_revive_it',async()=>{
 for(const result of [{...native.search,passed:0},{...native.search,domain:1,passed:1,table:native.search.table.slice(0,1)}]){
  const ui=await mount();let completions=0;ui.root.addEventListener('factory-result',()=>completions++);
  button(ui,'search').click();const request=ui.messages.at(-1);
  ui.deliver({result,elapsed:1},request);
  assert.equal(completions,0);assert.match(status(ui,'search'),/Stopped:.*No verdict/);
  assert.equal(ui.root.querySelector('[data-factory-act="search"] .factory-output').dataset.complete,undefined);
  assert.equal(ui.worker.terminated,true);
  ui.deliver({result:native.search,elapsed:1},request);assert.equal(completions,0);
 }
});

// P2: bound policy and returned values cannot change a fixed state's meaning.
function descriptorPurity(module){
 const fixed=module.initial(),before=module.view(fixed),d=module.descriptor('checked');
 assert.equal(Object.isFrozen(module.DESCRIPTORS.checked),true,'bound descriptor policy is transitively immutable');
 assert.equal(Reflect.set(module.DESCRIPTORS.checked,'unit','dollars'),false,'exported policy cannot expose a mutable unit');
 assert.equal(Reflect.set(d,'unit','dollars'),false,'getter cannot expose a mutable unit');
 assert.deepEqual(module.view(fixed),before,'fixed-state projection cannot change through a retained alias');
 const custom={checked:{label:'Checked',type:'integer',unit:'inputs'}},copy=module.descriptor('checked',custom);
 custom.checked.unit='dollars';assert.equal(copy.unit,'inputs','caller-owned policy remains detached from returned metadata');
 assert.equal(Object.isFrozen(copy),true);
}
test('L5_review_descriptor_alias_cannot_change_fixed_state_or_bound_policy',()=>descriptorPurity(core));
test('L5_incoming_wide_Number_is_refused',()=>{
 const large=clone(native.money);large.outputs[0]=Number.MAX_SAFE_INTEGER+1;rejectResult(start('money'),large);
});

// P2: current draft drives live enablement; submitted inputs drive the response.
test('L7_review_edit_during_search_updates_the_actual_money_button_and_feedback',async()=>{
 const ui=await mount();button(ui,'search').click();const request=ui.messages.at(-1);
 edit(ui,'amount','3000000001');assert.equal(button(ui,'money').disabled,true,'all buttons unavailable during single flight');
 ui.deliver({result:native.search,elapsed:1},request);
 assert.equal(button(ui,'money').disabled,true,'invalid live draft remains unavailable after the other act completes');
 assert.match(status(ui,'money'),/3,000,000,000|bounds/);
 const count=ui.messages.length;button(ui,'money').click();assert.equal(ui.messages.length,count);
 edit(ui,'amount','1000');assert.equal(button(ui,'money').disabled,false);
 button(ui,'money').click();assert.equal(ui.messages.at(-1).options.amount,'1000');
 ui.deliver({result:native.money,elapsed:1});
});
test('L7_active_request_keeps_submitted_inputs_while_next_draft_changes',async()=>{
 const draft={...money},submitted=core.step(core.initial('7'),{type:'Start',act:'money',draft});
 draft.amount='999';let state=submitted.state;const identity=state.identity;
 assert.equal(state.tasks.money.submitted.amount,'1000');assert.equal(Object.isFrozen(state.tasks.money.submitted),true);
 assert.equal(Reflect.set(submitted.intents[0].options,'amount','999'),false,'intent and submitted snapshot expose no mutable input alias');
 const edited=core.step(state,{type:'DraftChanged',act:'money',draft:{...money,amount:'2000'}});
 assert.deepEqual(edited.intents,[]);assert.equal(edited.state.identity,identity);
 assert.equal(edited.state.tasks.money.draft.amount,'2000');
 state=finish(edited.state,native.money).state;
 assert.equal(state.tasks.money.result.input[0],'1000');assert.equal(state.tasks.money.draft.amount,'2000');
 const ui=await mount();button(ui,'money').click();const request=ui.messages.at(-1);
 edit(ui,'amount','2000');ui.deliver({result:native.money,elapsed:1},request);
 assert.match(status(ui,'money'),/Done/);assert.equal(button(ui,'money').disabled,false);
 button(ui,'money').click();assert.equal(ui.messages.at(-1).options.amount,'2000');
});
test('L7_mounted_controls_refine_real_Wasm_admission_and_the_finite_implication',async()=>{
 const ui=await mount(),api=await instantiate(readFileSync(new URL('../public/factory.wasm',import.meta.url)));
 const gate=json('./presentation-finite.json').enablement.rows;
 const cases=[money,...Object.entries({amount:['0','3000000000','3000000001','-1',''],fee_bps:['0','10000','10001'],reserve_in:['0','1','3000000000','3000000001'],reserve_out:['0','1','3000000000','3000000001']}).flatMap(([key,values])=>values.map(value=>({...money,[key]:value})))];
 for(const draft of cases){
  for(const [key,value] of Object.entries(draft))edit(ui,key,value);
  api.reset();const reply=api.step({act:'money',...draft}),enabled=!button(ui,'money').disabled,accepted=!reply.error;
  const row=gate.find(([input])=>input[0]===0&&input[1]===Number(enabled)&&input[2]===Number(accepted));
  assert.equal(row?.[1],'Accept',`mounted money enablement / real Wasm: ${JSON.stringify(draft)}`);
 }
 api.reset();assert.equal(api.step({act:'money',...money,amount:'3000000001'}).error,'InputDomain');
 for(const [key,value] of Object.entries(money))edit(ui,key,value);
 for(const act of ['search','synthesis',...['none','m0002','m0018','m0040','m0049','m0054','m0061'].map(m=>`inventory/${m}`)]){
  const [kind,mutant]=act.split('/');
  if(mutant){const choice=ui.root.querySelector('.mutant-choice');choice.value=mutant;choice.dispatchEvent(new Event('change'));}
  assert.equal(button(ui,kind).disabled,false,'valid frozen factory act has a positive mounted witness');button(ui,kind).click();
  for(const name of core.ACTS)assert.equal(button(ui,name).disabled,true,'single-flight live gate');
  const request=ui.messages.findLast(m=>m.kind==='run');
  const result=runFactory(api,act,p=>ui.deliver(p.kind==='inventory'?{progress:p}:{candidate:p},request));
  ui.deliver({result,elapsed:1},request);assert.equal(ui.root.querySelector(`[data-factory-act="${kind}"] .factory-output`).dataset.complete,'true',`${act}: ${status(ui,kind)}`);
 }
 console.log('L7 mounted factory controls: 17 money boundary drafts against actual Wasm, 9 finite act/variant witnesses; prototype projection is separate');
});
test('L7_programmatic_start_rechecks_visible_inputs_and_exposes_refusal',async()=>{
 const ui=await mount();edit(ui,'amount','3000000001',false);button(ui,'money').click();
 assert.equal(ui.messages.length,0);assert.equal(button(ui,'money').disabled,true);assert.match(status(ui,'money'),/3,000,000,000|bounds/);
});

// P2: an independent prototype projection retains its separate exhaustive scope.
test('L7_projection_missing_Release_capacity_has_22_violations_and_exact_omissions',()=>{
 const mutant=clone(contract);mutant.actions.find(a=>a.id==='Release').when.args.pop();
 const violations=[],omissions=[],witnesses={};let contexts=0;
 for(const cell of native['inventory/none'].grid.cells){
  const [available,reserved,action,quantity,authorized]=cell.input,name={150:'Reserve',151:'Release',152:'Ship',153:'Restock'}[action];
  for(const busy of [false,true])for(const fault of [false,true])for(const task of contract.tasks){
   const snapshot={state:{available,reserved},steps:0},ui={task:task.id,input:{quantity,authorized:authorized===1},busy,fault};
   const enabled=project(contract,snapshot,ui).actions.find(a=>a.id===name).enabled;
   assert.ok(!enabled||cell.class==='Accept');if(enabled)witnesses[name]??=cell.input;
   if(cell.class==='Accept'&&!enabled&&!busy&&!fault&&task.id===name.toLowerCase())omissions.push(cell.input);
   if(project(mutant,snapshot,ui).actions.find(a=>a.id===name).enabled&&cell.class!=='Accept')violations.push(cell.input);
   contexts++;
  }
 }
 assert.equal(contexts,17280);assert.equal(violations.length,22);assert.deepEqual(violations[0],[3,3,151,3,1]);
 assert.equal(omissions.length,41);assert.ok(omissions.every(([a,r,action,q])=>action===153&&a+q<=5&&a+r+q>5));
 assert.deepEqual(witnesses,{Restock:[0,0,153,1,1],Release:[0,1,151,1,1],Ship:[0,1,152,1,1],Reserve:[1,0,150,1,1]});
});

// P2: source status never implies a published release archive.
function releaseCopy(html){
 assert.doesNotMatch(html,/Coming in 2\.5|<span class="ver">2\.5<\/span><span class="chip dev">IN DEVELOPMENT/);
 assert.match(html,/2\.5\.0 is the current release source/);assert.match(html,/Final qualification and publication remain separate release gates/);
}
test('review_public_copy_matches_current_release_source_and_keeps_publication_separate',()=>releaseCopy(page));

// Additional independent witnesses from the review, including negative controls.
test('L1_review_missing_metrics_mount_is_caught',async()=>{
 const normal=await mount();assert.equal(normal.root.querySelectorAll('[data-region]').length,16);
 const bad=await mount((u,s)=>u.endsWith('/factory.js')?s.replace('output.replaceChildren(instrument,metricsHost,comparison,record)','output.replaceChildren(instrument,comparison,record)'):s);
 assert.throws(()=>assert.equal(bad.root.querySelectorAll('[data-region]').length,16),assert.AssertionError);
});
test('L2_review_same_cell_reselection_and_reordered_replies_are_noops',()=>{
 let s=finish(start('inventory/none'),native['inventory/none']).state;const identity=s.identity;
 s=core.step(s,{type:'Select',identity,ordinal:0}).state;const old=s.selection.identity;
 s=core.step(s,{type:'Select',identity,ordinal:0}).state;
 assert.equal(core.step(s,{type:'CellResult',identity:old,ordinal:0,result:details['none/0']}).state,s);
 s=core.step(s,{type:'CellResult',identity:s.selection.identity,ordinal:0,result:details['none/0']}).state;
 assert.equal(core.step(s,{type:'CellResult',identity:old,ordinal:0,result:details['none/0']}).state,s);
});
function dynamicPreferences(module){
 let state=finish(start('inventory/none'),native['inventory/none']).state;
 state=core.step(state,{type:'Select',identity:state.identity,ordinal:0}).state;
 for(const received of [false,true]){
  if(received)state=core.step(state,{type:'CellResult',identity:state.selection.identity,ordinal:0,result:details['none/0']}).state;
  const meaning=s=>{const {preferences,...rest}=module.view(s);return rest;};
  const before=meaning(state);
  for(const preferences of [{motion:'normal',theme:'dark',width:320},{motion:'reduce',theme:'light',width:1440}]){
   const result=module.step(state,{type:'PreferencesChanged',preferences});assert.deepEqual(result.intents,[]);assert.deepEqual(meaning(result.state),before);
  }
 }
}
test('L3_review_dynamic_preferences_preserve_pending_and_received_selection',()=>dynamicPreferences(core));
test('L4_review_overlapping_authorization_blocks_are_refused',()=>assert.throws(()=>checkLayout({...GRID,authGap:-60}),/layout_overlap|layout_hit_inverse/));
test('L2_review_invalid_chunk_then_Fail_fences_a_late_success',()=>{
 let running=start('inventory/none'),old=running.identity;
 running=core.step(running,{type:'Cancel'}).state;running=core.step(running,{type:'Start',act:'inventory/none',draft:{mutant:'none'}}).state;
 for(const type of ['Result','Progress','Fail'])for(const key of ['epoch','run','request']){
  const identity={...running.identity,[key]:old[key]===running.identity[key]?'999':old[key]};
  assert.equal(core.step(running,{type,identity,result:native['inventory/none'],elapsed:1,message:'late'}).state,running);
 }
 const progress={...native['inventory/none'].grid,start:48,cells:native['inventory/none'].grid.cells.slice(48,96),names:native['inventory/none'].names};
 assert.throws(()=>core.step(running,{type:'Progress',identity:running.identity,progress}),/Invalid inventory progress/);
 const failed=core.step(running,{type:'Fail',identity:running.identity,message:'Invalid inventory progress'}).state;
 assert.equal(finish(failed,native['inventory/none']).state,failed);
});
test('review_synthesis_result_before_progress_is_self_contained',async()=>{
 const ui=await mount();let completed=0;ui.root.addEventListener('factory-result',()=>completed++);
 button(ui,'synthesis').click();ui.deliver({result:native.synthesis,elapsed:1});
 assert.equal(completed,1);assert.equal(ui.root.querySelectorAll('.candidate-cards li').length,8);assert.match(status(ui,'synthesis'),/Done/);
});
test('review_new_policy_and_preference_controls_are_killed',async()=>{
 async function mutant(from,to){assert.ok(source.includes(from));return import('data:text/javascript;base64,'+Buffer.from(source.replace(from,to)).toString('base64'));}
 const shallow=await mutant('DESCRIPTORS = freeze({','DESCRIPTORS = Object.freeze({');
 assert.throws(()=>descriptorPurity(shallow),assert.AssertionError);
 const width=await mutant('const updated=freeze({...state,preferences});','const updated=freeze({...state,preferences,selection:preferences.width===320?null:state.selection});');
 assert.throws(()=>dynamicPreferences(width),assert.AssertionError);
 const selfDeclared=await mutant('const total=TOTALS[act];\n  if(r.domain!==total||task.evidence.total!==total)', 'const total=r.domain;\n  if(false)');
 const ignoresPasses=await mutant('if(r.passed!==total)', 'if(false)');
 for(const [module,result] of [[selfDeclared,{...native.search,domain:1,passed:1,table:native.search.table.slice(0,1)}],[ignoresPasses,{...native.search,passed:0}]]){
  const running=module.step(module.initial(),{type:'Start',act:'search'}).state;
  assert.throws(()=>assert.throws(()=>module.step(running,{type:'Result',identity:running.identity,result,elapsed:1})),assert.AssertionError,'review claim check kills the weakened validator');
 }
 const lostEdit=await mutant("if(!ACTS.includes(event.act))", "if(state.phase==='running'||!ACTS.includes(event.act))");
 const search=lostEdit.step(lostEdit.initial(),{type:'Start',act:'search'}).state;
 const changed=lostEdit.step(search,{type:'DraftChanged',act:'money',draft:{...money,amount:'3000000001'}}).state;
 const done=lostEdit.step(changed,{type:'Result',identity:changed.identity,result:native.search,elapsed:1});
 assert.throws(()=>assert.equal(done.view.actions.find(a=>a.act==='money').enabled,false),assert.AssertionError,'lost-edit control recreates the live enablement defect');
 assert.throws(()=>releaseCopy(page.replace('2.5.0 is the current release source','Coming in 2.5')),assert.AssertionError);
});
