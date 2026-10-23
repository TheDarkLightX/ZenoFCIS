// Independent review witnesses retained against the shipped reducer and page.
// The DOM adapter supplies no Chrome layout, raster or keyboard evidence.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync,readdirSync} from 'node:fs';
import * as core from '../public/zenoui/presentation.js';
import {project} from '../public/zenoui/semantic.js';
import {GRID,checkLayout} from '../public/factory-layout.js';
import {instantiate} from '../public/demo-module.js';
import {runFactory} from '../public/factory-runner.js';
import {ORIGIN,documentFrom,moduleLoader} from './browser-fixture.mjs';
const text=file=>readFileSync(new URL(file,import.meta.url),'utf8');
const json=file=>JSON.parse(text(file));
const native=json('./harness/factory-native.json'),details=json('./harness/inventory-details-native.json');
const contract=json('../public/zenoui/inventory.semantic.json');
const page=readFileSync(new URL('../public/index.html',import.meta.url),'utf8');
const source=readFileSync(new URL('../public/zenoui/presentation.js',import.meta.url),'utf8');
const money={amount:'1000',fee_bps:'30',reserve_in:'10000',reserve_out:'20000'};
const clone=x=>structuredClone(x);
const start=(act='search',draft=act==='money'?money:act.startsWith('inventory/')?{mutant:act.split('/')[1]}:{})=>core.step(core.initial('7'),{type:'Start',act,draft}).state;
const finish=(s,r)=>core.step(s,{type:'Result',identity:s.identity,result:r,elapsed:1});
function rejectResult(state,result,name='refused payload'){
 const before=JSON.stringify(state);assert.throws(()=>finish(state,result),undefined,name);
 assert.equal(JSON.stringify(state),before,`${name}: refusal preserves the pure pre-state`);
 assert.equal(core.view(state).tasks[state.act].verdict,null,`${name}: no result verdict before admission`);
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
 const {namespace}=await loader.evaluate(`${ORIGIN}/ZenoFCIS/factory.js`);
 return {root:document.getElementById('factory'),module:namespace,messages,timers,get worker(){return worker;},
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

// Re-review residual 2: an inventory payload whose cells, counts and ringed witness disagree
// is refused before any state change. Each witness is the only one its named check refuses.
const variant=(mutant,change)=>{const r=clone(native[`inventory/${mutant}`]);change(r);return r;};
const INVENTORY_CONTRADICTIONS={
 'reviewer: every difference flag cleared':['m0002',r=>{for(const c of r.grid.cells)c.different=false;r.differences=0;}],
 'ringed cell unchanged':['m0002',r=>{r.grid.first_ordinal=r.grid.cells.findIndex(c=>!c.different);}],
 'changed class without a flag':['m0002',r=>{r.grid.cells.find((c,i)=>c.class!==c.mutant_class&&i!==r.grid.first_ordinal).different=false;r.differences--;}],
 'unknown decision class':['m0002',r=>{r.grid.cells[0].class=r.grid.cells[0].mutant_class='Unknown';}],
 'difference in the unchanged contract':['none',r=>{const c=r.grid.cells[0];c.different=true;r.differences=1;r.grid.first_ordinal=0;r.status='distinguished';
  r.first_difference={input:c.input,original:{class:c.class,reason:null,laws:[]},mutant:{class:c.mutant_class,reason:200,laws:[]}};}],
 'passing count beside the grid':['m0018',r=>{r.mutant_passed=864;}],
 'differences without a witness':['m0002',r=>{r.first_difference=null;r.grid.first_ordinal=null;}],
 'witness without differences':['m0049',r=>{r.first_difference=native['inventory/m0002'].first_difference;}],
 'status beside the witness':['m0002',r=>{r.status='equivalent over the full domain';}],
 'witness decisions identical':['m0054',r=>{
  // Keep class/count bindings consistent so only decision equality rejects it.
  r.grid.cells[r.grid.first_ordinal].mutant_class=r.first_difference.original.class;
  r.mutant_passed=r.grid.cells.filter(c=>c.mutant_class!=='Refused').length;
  r.first_difference.mutant=clone(r.first_difference.original);
 }],
 'missing names':['m0002',r=>{delete r.names;}],
};
test('L6_rereview_contradictory_inventory_payloads_never_promote_a_verdict',()=>{
 for(const mutant of ['none','m0002','m0018','m0040','m0049','m0054','m0061'])assert.equal(finish(start(`inventory/${mutant}`),native[`inventory/${mutant}`]).view.tasks.inventory.verdict,'finite checked',mutant);
 for(const [name,[mutant,change]] of Object.entries(INVENTORY_CONTRADICTIONS))rejectResult(start(`inventory/${mutant}`),variant(mutant,change),name);
});

// Re-review residual 3: the reducer checks every field the page draws before a done state,
// and the page fences a drawing failure so the run stops, settles and claims nothing.
const altered=(act,change)=>{const r=clone(native[act]);change(r);return r;};
const UNDRAWABLE={
 'reviewer: search without first_difference':['search',r=>{delete r.first_difference;}],
 'search witness on another input':['search',r=>{r.first_difference.ordinal=2;}],
 'search witness not refused':['search',r=>{r.first_difference.swapped=r.first_difference.original;}],
 'search without node counts':['search',r=>{delete r.after_nodes;}],
 'search record not plain data':['search',r=>{r.winner=1n;}],
 'reviewer: synthesis without names':['synthesis',r=>{delete r.names;}],
 'synthesis without reason names':['synthesis',r=>{r.names.reason_ranks={};}],
 'synthesis without input names':['synthesis',r=>{delete r.inputs;}],
 'money probe without steps':['money',r=>{delete r.overflow.steps;}],
};
test('L6_rereview_undrawable_results_are_refused_before_done',()=>{
 for(const [name,[act,change]] of Object.entries(UNDRAWABLE))rejectResult(start(act),altered(act,change),name);
});
function stopped(ui,act,root=ui.root){
 const output=root.querySelector(`[data-factory-act="${act}"] .factory-output`);
 assert.match(root.querySelector(`[data-factory-act="${act}"] .factory-status`).textContent,/Stopped:.*No verdict/);
 assert.equal(output.dataset.complete,undefined,'a failed run is never marked complete');
 assert.equal(output.querySelector('.verdict'),null,'nothing partly drawn stays on the page');
 assert.equal(ui.worker.terminated,true,'the worker is stopped');
 for(const name of core.ACTS)assert.equal(root.querySelector(`button[data-act="${name}"]`).disabled,false,'controls are available again');
}
test('L6_rereview_mounted_undrawable_results_fail_closed',async()=>{
 for(const name of ['reviewer: search without first_difference','reviewer: synthesis without names']){
  const [act,change]=UNDRAWABLE[name],ui=await mount();let completions=0;ui.root.addEventListener('factory-result',()=>completions++);
  button(ui,act).click();const request=ui.messages.at(-1);
  ui.deliver({result:altered(act,change),elapsed:1},request);stopped(ui,act);
  ui.deliver({result:native[act],elapsed:1},request);assert.equal(completions,0,'a late genuine reply cannot revive it');
 }
});
const PAINT_FAILURES={search:['/factory.js','function render(output,data) {'],inventory:['/factory-grid.js','    finish(result) {']};
async function paintFailureFailsClosed(act,transform=(_u,s)=>s){
 const [file,site]=PAINT_FAILURES[act];
 const ui=await mount((u,s)=>transform(u,u.endsWith(file)?s.replace(site,`${site}throw new Error('planted drawing failure');`):s));
 const root=documentFrom(page).getElementById('factory'),factory=ui.module.mountFactory(root);
 let completions=0,outcome='pending';root.addEventListener('factory-result',()=>completions++);
 factory.run(act==='inventory'?'inventory/none':act).then(()=>{outcome='resolved';},error=>{outcome=`rejected: ${error.message}`;});
 const request=ui.messages.at(-1);ui.deliver({result:native[act==='inventory'?'inventory/none':act],elapsed:1},request);
 await new Promise(resolve=>setImmediate(resolve));
 assert.match(outcome,/^rejected: The page could not show this result/,'the run promise settles');assert.equal(completions,0);
 stopped(ui,act,root);if(act==='inventory')assert.equal(root.querySelector('.inventory-grid'),null);
 ui.deliver({result:native[act==='inventory'?'inventory/none':act],elapsed:1},request);assert.equal(completions,0);
}
test('L6_rereview_admitted_result_that_cannot_be_drawn_fails_the_run',async()=>{
 for(const act of Object.keys(PAINT_FAILURES))await paintFailureFailsClosed(act);
});
test('L6_rereview_cell_decision_that_cannot_be_drawn_shows_a_refusal',async()=>{
 const ui=await mount(),choice=ui.root.querySelector('.mutant-choice');choice.value='none';choice.dispatchEvent(new Event('change'));
 button(ui,'inventory').click();ui.deliver({result:native['inventory/none'],elapsed:1});
 const inspect=ui.messages.findLast(m=>m.kind==='inspect'),detail=clone(details[`none/${inspect.ordinal}`]);delete detail.original.laws;
 ui.deliver({detail,ordinal:inspect.ordinal},inspect);
 const panel=ui.root.querySelector('.cell-inspection');
 assert.match(panel.textContent,/could not show this decision/);assert.equal(panel.dataset.ready,undefined);
 assert.match(status(ui,'inventory'),/Done/,'a cell refusal leaves the checked run intact');
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

// P2 and re-review: every version the page names comes from its source of truth, and
// source status never implies a published release archive.
function releaseFacts(workspace=text('../../Cargo.toml'),factory=text('../factory/Cargo.toml'),site=text('../Cargo.toml')){
 const source=workspace.match(/^\[workspace\.package\]\nversion = "([0-9]+\.[0-9]+\.[0-9]+)"$/m)?.[1],patch=site.split('[patch.crates-io]')[1]??'';
 const pins=[...factory.matchAll(/^(zeno-fcis-[a-z-]+) = "=([^"]+)"$/gm)];
 assert.ok(source,'workspace release source version');assert.ok(pins.length>=3,'factory library pins');
 for(const [,name,version] of pins){
  assert.equal(version,source,`${name}: the factory pins the release source`);
  assert.match(patch,new RegExp(`^${name} = \\{ path = "\\.\\./crates/${name}" \\}$`,'m'),`${name}: patched to this checkout`);
  assert.match(text(`../../crates/${name}/Cargo.toml`),/^version\.workspace = true$/m,`${name}: carries the release source version`);
 }
 return {source,library:source};
}
function releaseCopy(html,{source,library}=releaseFacts()){
 const words=html.replace(/<[^>]*>/g,' ').replace(/\s+/g,' '),order=v=>v.split('.').reduce((a,n)=>a*1000+Number(n),0),e=v=>v.replaceAll('.','\\.');
 const released=[...words.matchAll(/ ([0-9]+\.[0-9]+\.[0-9]+) RELEASED /g)].map(m=>m[1]),planned=[...words.matchAll(/ ([0-9]+\.[0-9]+\.[0-9]+) PLANNED /g)].map(m=>m[1]);
 assert.equal(released.length,1,'one published release');const [published]=released;
 assert.ok(order(published)<=order(source),'publication never runs ahead of the release source');
 assert.ok(planned.every(v=>order(v)>order(source)),'planned versions follow the release source');
 const status=`${e(source)} is the current release source\\. Final qualification and publication remain separate release gates\\.`,factory=`Each act runs the library built from the ${e(library)} release source,`;
 const claims=[status,factory,`ZenoFCIS ${e(source.replace(/\.[0-9]+$/,''))} source `,` ${e(source)} RELEASE SOURCE `,`The ${e(source)} source includes`,
  ` ${e(published)} RELEASED `,`Get ${e(published)} `,`Limits in ${e(published)}:`,`Install ${e(published)} on Linux`,`VERSION=${e(published)} `,
  ...planned.map(v=>` ${e(v)} PLANNED `),'Apache-2\\.0','[0-9]+\\.[0-9]+ billion'];
 const covered=claims.flatMap(claim=>[...words.matchAll(new RegExp(claim,'g'))].map(m=>[m.index,m.index+m[0].length]));
 const stale=[...words.matchAll(/[0-9]+(?:\.[0-9]+)+/g)].filter(m=>!covered.some(([a,b])=>a<=m.index&&m.index+m[0].length<=b)).map(m=>words.slice(Math.max(0,m.index-40),m.index+m[0].length+20));
 assert.deepEqual(stale,[],'every version named in the page copy is explained by its source');
 assert.match(words,new RegExp(status),'the page states the release source and its separate gates');
 assert.match(words,new RegExp(factory),'the factory section names the library it runs');
 for(const [,version] of html.matchAll(/releases\/(?:tag|download)\/v([^"/$]+)/g))assert.equal(version,published,'release links name the published release');
 return {source,published};
}
test('review_public_copy_matches_current_release_source_and_keeps_publication_separate',()=>{
 releaseCopy(page);
 for(const file of readdirSync(new URL('../public/',import.meta.url),{recursive:true}).filter(f=>/\.(js|html)$/.test(f)&&f!=='index.html'))
  assert.doesNotMatch(text(`../public/${file}`),/\b[0-9]+\.[0-9]+\.[0-9]+\b/,`${file}: page scripts name no release version`);
});

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
 // The narrowed domain keeps the refused-proposal witness (input 2), so only the domain check refuses it.
 for(const [module,result] of [[selfDeclared,{...native.search,domain:2,passed:2,table:native.search.table.slice(0,2)}],[ignoresPasses,{...native.search,passed:0}]]){
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

// Each re-review check is killed by its own witness, and the drawing fence by a planted failure.
test('rereview_residual_controls_are_killed',async()=>{
 async function disabled(from,to=`if(false)${from}`){assert.equal(source.split(from).length,2,`one mutation site: ${from}`);return import('data:text/javascript;base64,'+Buffer.from(source.replace(from,to)).toString('base64'));}
 function admits(module,act,result){
  const running=module.step(module.initial('7'),{type:'Start',act,draft:act==='money'?money:act.startsWith('inventory/')?{mutant:act.split('/')[1]}:{}}).state;
  try{return module.step(running,{type:'Result',identity:running.identity,result,elapsed:1}).state.tasks[running.act].phase==='done';}catch{return false;}
 }
 const witness=name=>INVENTORY_CONTRADICTIONS[name]?[`inventory/${INVENTORY_CONTRADICTIONS[name][0]}`,variant(...INVENTORY_CONTRADICTIONS[name])]:[UNDRAWABLE[name][0],altered(...UNDRAWABLE[name])];
 for(const [from,to,names] of [
  ['if(cells.some(c=>!CLASSES',undefined,['unknown decision class']],
  ['if(cells.some(c=>c.class!==c.mutant_class&&!c.different))',undefined,['changed class without a flag']],
  ["||(r.mutant==='none'&&r.differences!==0)",'',['difference in the unchanged contract']],
  ["if(cells.filter(c=>c.mutant_class!=='Refused')",undefined,['passing count beside the grid']],
  ['if(r.differences===0?first!==null',undefined,['differences without a witness','witness without differences','status beside the witness']],
  ['if(first!==null&&(',undefined,['ringed cell unchanged']],
  ['||decision(w.original)===decision(w.mutant)','',['witness decisions identical']],
  ['if(!sequence(names?.inputs,5)',undefined,['missing names']],
  ['if(!integer(w?.ordinal,total-1)',undefined,['reviewer: search without first_difference','search witness on another input','search witness not refused']],
  ['if(!integer(r.before_nodes)',undefined,['search without node counts']],
  ['if(!plain(r))',undefined,['search record not plain data']],
  ['if(!sequence(r.inputs,12)',undefined,['reviewer: synthesis without names','synthesis without reason names','synthesis without input names']],
  ['if(!integer(r.overflow.steps))',undefined,['money probe without steps']],
 ]){
  const module=await disabled(from,to);
  for(const name of names){const [act,result]=witness(name);assert.equal(admits(core,act,result),false,name);assert.equal(admits(module,act,result),true,`${name} kills ${from}`);}
 }
 const fence='try{paint(result.view,event);}\n    catch(error){',unfenced=(u,s)=>u.endsWith('/factory.js')?(assert.ok(s.includes(fence)),s.replace(fence,'paint(result.view,event);\n    if(false){const error=undefined;')):s;
 for(const act of Object.keys(PAINT_FAILURES))await assert.rejects(paintFailureFailsClosed(act,unfenced),assert.AssertionError,`${act}: the fence is what fails the run`);
 const {source:release,published}=releaseCopy(page);
 for(const [from,to] of [
  [`the library built from the ${release} release source`,`the ${published} library`],
  [`built from the ${release} release source`,`built from the ${published} release source`],
  [`${release} is the current release source`,'Coming in 2.5'],
  [`<span class="ver">${release}</span><span class="chip dev">RELEASE SOURCE`,`<span class="ver">${release.replace(/\.[0-9]+$/,'')}</span><span class="chip dev">IN DEVELOPMENT`],
  [`Install ${published} on Linux`,'Install 2.3.0 on Linux'],
  [`releases/tag/v${published}`,'releases/tag/v2.3.0'],
  ['nothing is sent to a server.',`nothing is sent to a server. It runs ZenoFCIS ${published}.`],
 ]){assert.ok(page.includes(from),from);assert.throws(()=>releaseCopy(page.replace(from,to)),assert.AssertionError,to);}
 assert.throws(()=>releaseFacts(undefined,text('../factory/Cargo.toml').replace(`"=${release}"`,`"=${published}"`)),assert.AssertionError,'a stale library pin');
 const patched=text('../Cargo.toml'),unpatched=patched.replace(/^zeno-fcis-contract = \{ path = .*$/m,'');
 assert.notEqual(unpatched,patched);assert.throws(()=>releaseFacts(undefined,undefined,unpatched),assert.AssertionError,'a library not built from this checkout');
});
