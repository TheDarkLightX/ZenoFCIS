// Retained native descriptor/DTOs plus explicitly synthetic controls; no native/browser execution.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import {describe,checkPresentation,verifyPresentation,formFields} from '../public/zenoui/app-contract.js';
import {scopeCommands,scopeInputs,validateAst} from '../public/zenoui/ui-grammar.js';
import {buildFragmentCache,searchUi} from '../public/zenoui/ui-search.js';
import {initial,step,view,inputFor} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {mount,sha256} from '../public/zenoui/app.js';
import {documentFrom} from './browser-fixture.mjs';
import {approvalFixture,editFixture} from './idiom-fixtures.mjs';

const fixture=JSON.parse(await readFile(new URL('./test-data/workflow-ui-native.json',import.meta.url),'utf8'));
const metadata=JSON.parse(await readFile(new URL('./test-data/grouped-workflow-metadata.json',import.meta.url),'utf8'));
const nativeSamples=JSON.parse(await readFile(new URL('./test-data/grouped-workflow-samples.json',import.meta.url),'utf8'));
const app=describe(fixture.descriptor,metadata),contract=fixture.authorityContract,principal='01'.repeat(32);
const ids=['120','121','122','123','124'],quantities=['1','2','3'];
const find=(n,p)=>p(n)?n:(n.children??[]).map(c=>find(c,p)).find(Boolean);
function start(values=['150',null,'0'],context='0'){
 let s=step(initial(app,contract),{type:'Principal',value:principal}).state;
 s=step(s,{type:'Context',index:5,value:context}).state;
 return step(s,{type:'Snapshot',values,revision:'0',observation:{contract,principal,pending:'0'}}).state;
}
function ready(){
 let s=step(start(),{type:'Select',command:'122'}).state;
 const edited=step(s,{type:'Edit',command:'122',index:4,value:'3'}),binding=edited.intents.find(i=>i.binding.command==='122').binding;
 assert.deepEqual(binding.input,fixture.reply.input);
 return {s:step(edited.state,{type:'Preview',binding,reply:fixture.reply}).state,binding};
}
function scopeNode(data,description=app){
 const rendered=presentationView(description,data,{server:true});
 return {rendered,scope:Object.values(rendered.nodes).find(n=>Object.hasOwn(n,'ready')&&Object.hasOwn(n,'commands')),selector:Object.values(rendered.nodes).find(n=>n.id==='command-direction')};
}

test('one checked selected scope retains every command, full field binding and declared Action label',()=>{
 assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);
 assert.deepEqual(app.presentation.summary.scopes,['selected']);assert.equal(app.presentation.summary.visibleForms,1);assert.equal(app.presentation.summary.duplicateInputs,0);
 assert.deepEqual(scopeCommands(app,app.presentation.requirements,'selected'),ids);
 assert.deepEqual(scopeInputs(app,app.presentation.requirements,'selected'),['command.101.164']);
 for(const id of ids)assert.deepEqual(formFields(app,id).map(f=>f.index),[4]);
 const {scope,selector}=scopeNode(view(start()));assert.equal(scope.label,'Start repair');assert.equal(selector.label,'Action');assert.deepEqual(selector.options.map(o=>o.value),ids);
 assert.equal(app.domain,'1800');assert.equal(app.possible,null);assert.deepEqual(app.relevant,Object.fromEntries(ids.map(id=>[id,[4,5]])));
});

test('all fifteen command/quantity combinations keep separate drafts and exact six-slot preview bindings',()=>{
 let s=start();const expected=Object.fromEntries(ids.map(id=>[id,'1']));
 for(const quantity of quantities)for(const id of ids){
  s=step(s,{type:'Select',command:id}).state;
  const edited=step(s,{type:'Edit',command:id,index:4,value:quantity});s=edited.state;expected[id]=quantity;
  assert.equal(s.selectedCommand,id);assert.equal(s.consent,null);assert.equal(s.review,null);
  assert.equal(edited.intents.length,5);assert.ok(edited.intents.every(i=>i.kind==='preview'));
  for(const intent of edited.intents){const command=intent.binding.command;assert.deepEqual(intent.binding.input,['150',null,'0',command,expected[command],'0']);}
  for(const command of ids)assert.equal(s.drafts[command][4],expected[command]);
  const {scope,rendered}=scopeNode(view(s));assert.equal(scope.command,id);assert.equal(scope.label,app.commands.find(c=>c.id===id).label);
  assert.equal(Object.values(rendered.nodes).find(n=>n.id==='command-selected-field-4').value,quantity);
 }
 // Different final drafts must survive switching away and back, not converge to one shared value.
 for(const [i,id] of ids.entries())s=step(s,{type:'Edit',command:id,index:4,value:quantities[i%3]}).state;
 for(const [i,id] of ids.entries()){s=step(s,{type:'Select',command:id}).state;assert.equal(inputFor(s,id)[4],quantities[i%3]);}
});

test('all fourteen retained native samples preserve each selected command outcome without auto-switching',()=>{
 const accepted=new Set();
 for(const sample of nativeSamples){
  const id=sample.input[3];let s=start(sample.input.slice(0,3),sample.input[5]);s=step(s,{type:'Select',command:id}).state;
  const edited=step(s,{type:'Edit',command:id,index:4,value:sample.input[4]}),binding=edited.intents.find(i=>i.binding.command===id).binding;
  assert.deepEqual(binding.input,sample.input);
  s=step(edited.state,{type:'Preview',binding,reply:sample.reply}).state;
  const data=view(s);assert.equal(data.selectedCommand,id);assert.equal(data.actions.find(a=>a.id===id).enabled,sample.reply.class==='Accept');
  assert.equal(scopeNode(data).scope.command,id);
  if(sample.reply.class==='Accept')accepted.add(id);
 }
 assert.deepEqual([...accepted].sort(),ids);
});

test('masked native preview and confirmation retain exact binding; edits and selection clear consent',()=>{
 let {s,binding}=ready();const data=view(s);assert.equal(data.selectedCommand,'122');assert.ok(data.actions.find(a=>a.id==='122').enabled);
 assert.equal(step(s,{type:'Dispatch',command:'122'}).intents.length,0);
 s=step(s,{type:'Review',command:'122'}).state;assert.equal(view(s).confirmation.fields.length,5);
 assert.ok(!view(s).confirmation.fields.some(f=>f.index===1));assert.equal(inputFor(s,'122')[1],null);assert.equal(inputFor(s,'122').length,6);
 s=step(s,{type:'Confirm'}).state;const sent=step(s,{type:'Dispatch',command:'122'});assert.equal(sent.intents.length,1);assert.deepEqual(sent.intents[0].binding.input,fixture.reply.input);
 const edited=step(s,{type:'Edit',command:'122',index:4,value:'2'}).state;assert.equal(edited.review,null);assert.equal(edited.consent,null);assert.equal(step(edited,{type:'Dispatch',command:'122'}).intents.length,0);
 const switched=step(s,{type:'Select',command:'121'}).state;assert.equal(switched.consent,null);assert.equal(switched.review,null);
 for(const key of ['epoch','run','request','command','contract','revision','principal'])assert.equal(step(switched,{type:'Preview',binding:{...binding,[key]:'stale'},reply:fixture.reply}).state,switched);
 assert.equal(step(switched,{type:'Preview',binding,reply:fixture.reply}).state,switched);
 assert.equal(step(s,{type:'Context',index:5,value:'1'}).state,s);assert.equal(step(s,{type:'Principal',value:'other'}).state,s);
});

test('grouped chosen receipts, coverage and incompatible caches refuse forged or stale inputs',()=>{
 const old=describe(fixture.descriptor,fixture.metadata),copied=structuredClone(app);copied.searchReceipt=structuredClone(old.searchReceipt);
 assert.throws(()=>checkPresentation(copied),/UiPresentationReceiptMismatch/);
 for(const [key,value] of [['format','zenoui/ui-search/0'],['format','zenoui/ui-search/999'],['grammar','zenoui/ordered-interval-ui/2'],['grammar','zenoui/ordered-interval-ui/999'],['checker','zenoui/component-checker/2'],['checker','zenoui/component-checker/999']]){
  const bad=structuredClone(app);bad.searchReceipt[key]=value;assert.throws(()=>checkPresentation(bad),/UiPresentationReceiptMismatch/);assert.throws(()=>verifyPresentation(bad),/UiPresentationReceiptMismatch/);
 }
 const missing=structuredClone(app.presentation.ast);(function strip(n){if(n.children)n.children=n.children.filter(c=>c.kind!=='input').map(c=>(strip(c),c));})(missing);
 assert.equal(validateAst(app,app.presentation.requirements,missing).valid,false);
 const forged=structuredClone(app);forged.relevant['120']=[5];assert.throws(()=>checkPresentation(forged),/UiInvalidFallbackRelevance/);
 const {cache}=buildFragmentCache({fragmentSize:2});for(const version of ['zenoui/ordered-interval-ui/2','zenoui/ordered-interval-ui/999']){const bad=structuredClone(cache);bad.identity.grammar=version;assert.throws(()=>searchUi(app,app.presentation.requirements,{cache:bad,fragmentSize:2,maxCandidates:1}),/UiFragmentCacheMismatch/);}
});

test('selected approval keeps Decline, its Decision label and its specialized title',()=>{
 const {raw,meta}=approvalFixture();meta.requirements.groups=[];meta.requirements.maxVisibleForms=1;
 const decision=describe(raw,meta);let s=step(initial(decision,contract),{type:'Select',command:'221'}).state;
 const refresh=step(s,{type:'Refresh'});s=refresh.state;
 for(const {binding} of refresh.intents)s=step(s,{type:'Preview',binding,reply:{input:binding.input,class:'Accept',reason:null,post:[binding.command==='220'?'1':'2'],checked:'1',total:'1'}}).state;
 const result=scopeNode(view(s),decision);assert.equal(result.selector.label,'Decision');assert.equal(result.scope.label,'Approve or Decline');assert.equal(result.scope.command,'221');
 assert.equal(view(s).actions.find(a=>a.primary).id,'221');assert.equal(step(s,{type:'Dispatch',command:'221'}).intents.length,0);
});

test('swap selected scope retains Swap and Direction; scalar edit still exposes its own idiom',()=>{
 // A synthetic typed swap descriptor, used only to guard view-copy compatibility.
 const field=(path,index,source,domain,initial,options=[])=>({path,index,source,domain,initial,options,type_name:'Fixture'});
 const raw={fields:[field('pre.a',0,'state',{kind:'int',min:'0',max:'20'},'10'),field('pre.b',1,'state',{kind:'int',min:'0',max:'20'},'10'),field('command.action',2,'command',{kind:'sum',variants:[1,2]},'1',[{value:'1'},{value:'2'}]),field('command.pay',3,'command',{kind:'int',min:'0',max:'3'},'1'),field('command.min',4,'command',{kind:'int',min:'0',max:'3'},'0'),field('context.party',5,'context',{kind:'bool'},'1')],reasons:[],enablement:{table:[false,false,false,true]},domain:'28224'};
 const req={...metadata.requirements,primaryFields:['command.pay'],detailFields:['command.min'],contextFields:['context.party'],projections:[{id:'receive',label:'You receive',kind:'accepted-decrease',fieldByCommand:{1:'pre.b',2:'pre.a'}}],quoteOverrides:{'command.min':'0'}};
 const meta={format:'zenoui/metadata/2',title:'Synthetic swap',action:'command.action',fields:{'pre.a':{label:'A reserve',unit:'A'},'pre.b':{label:'B reserve',unit:'B'},'command.action':{label:'Action',unit:'unitless'},'command.pay':{label:'Pay',unit:'units',role:'amount-in'},'command.min':{label:'Minimum',unit:'units',role:'min-out'},'context.party':{label:'Operator',unit:'unitless'}},variants:{1:'A for B',2:'B for A'},commands:{1:{label:'A for B',consequential:true},2:{label:'B for A',consequential:true}},reasons:{},requirements:req};
 const swap=describe(raw,meta),result=scopeNode(view(initial(swap,contract)),swap);assert.equal(find(swap.presentation.ast,n=>n.kind==='idiom').idiom,'swap-card');assert.equal(result.scope.label,'Swap');assert.equal(result.selector.label,'Direction');
 const edit=editFixture(),scalar=describe(edit.raw,edit.meta);assert.equal(find(scalar.presentation.ast,n=>n.kind==='idiom').idiom,'scalar-edit');
});

async function settle(predicate){for(let i=0;i<800;i++){if(predicate())return;await delay(5);}throw Error('grouped synthetic shell control timed out');}
test('mounted explicit Go-to changes only selection, focuses the existing heading and preserves exact draft/confirmation',async()=>{
 const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app'),restore=[];
 for(const [key,value] of Object.entries({document:doc,crypto:webcrypto,innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})})){const previous=Object.getOwnPropertyDescriptor(globalThis,key);Object.defineProperty(globalThis,key,{value,writable:true,configurable:true});restore.push(()=>previous?Object.defineProperty(globalThis,key,previous):delete globalThis[key]);}
 let last;const dispatched=[];root.addEventListener('zenoui-view',e=>{last=e.detail;});
 const transport={terminate(){},postMessage(data){
  if(data.kind==='init')queueMicrotask(()=>this.onmessage({data:{kind:'ready'}}));
  else if(data.kind==='preview'){
   const reply=JSON.stringify(data.binding.input)===JSON.stringify(fixture.reply.input)?fixture.reply:{input:data.binding.input,class:'Reject',reason:'200',post:[],checked:'1',total:'1'};
   queueMicrotask(()=>this.onmessage({data:{kind:'preview',binding:data.binding,reply}}));
  }else{dispatched.push(data.binding);queueMicrotask(()=>this.onmessage({data:{kind:'error',binding:data.binding}}));}
 }};
 try{
  const source=fixture.source,bundle={description:app,source};bundle.contract=await sha256(bundle);
  await mount(root,bundle,{session:{contract,principal,revision:'0',values:['150',null,'0'],context:['0'],pending:'0'},transport});
  const selector=root.querySelector('#command-direction'),quantity=root.querySelector('#command-selected-field-4');
  let shown=quantity.value,valueWrites=0;
  Object.defineProperty(quantity,'value',{get:()=>shown,set:value=>{shown=String(value);valueWrites++;quantity.selectionStart=quantity.selectionEnd=shown.length;}});
  const choose=id=>{selector.focus();selector.value=id;selector.dispatchEvent(new Event('change'));};
  const edit=value=>{quantity.focus();quantity.value=value;quantity.dispatchEvent(new Event('input'));};
  choose('122');edit('3');quantity.value='03';quantity.selectionStart=quantity.selectionEnd=1;const typedWrites=valueWrites;
  await settle(()=>last?.actions.every(a=>a.tone!=='pending'));
  assert.equal(quantity.value,'03','SAME_OWNER_PREVIEW_PRESERVES_FOCUSED_TEXT');assert.equal(valueWrites,typedWrites,'SAME_OWNER_PREVIEW_DOES_NOT_ASSIGN_VALUE');
  assert.equal(quantity.selectionStart,1);assert.equal(quantity.selectionEnd,1);
  quantity.value='3';selector.value='120';selector.dispatchEvent(new Event('change'));
  assert.equal(doc.activeElement===quantity,true,'PROGRAMMATIC_SELECT_RETAINS_INPUT_FOCUS');
  assert.equal(quantity.value,'1','FOCUSED_INPUT_OWNER_REFRESHES_RETAINED_DRAFT');
  await settle(()=>last?.actions.every(a=>a.tone!=='pending'));
  assert.equal(last.selectedCommand,'120','previews never silently select an accepted alternative');assert.equal(root.querySelector('form').dataset.command,'120');assert.equal(root.querySelectorAll('form').length,1);
  assert.equal([...root.querySelectorAll('label')].find(label=>label.htmlFor===selector.id)?.textContent,'Action');assert.equal(root.querySelector('#action-selected-title').tagName,'H3');assert.equal(root.querySelector('#action-selected-title').textContent,'Start repair');assert.equal(quantity.value,'1');
  const heading=root.querySelector('#action-selected-title'),jump=root.querySelector('.zu-next-action');assert.equal(jump.textContent,'Go to Check in item');
  jump.click();assert.equal(doc.activeElement,heading,'GO_TO_FOCUSES_MOUNTED_SELECTED_HEADING');assert.equal(root.querySelector('#action-selected-title'),heading);assert.equal(last.selectedCommand,'122');assert.equal(quantity.value,'3');assert.equal(last.confirmation,null);assert.equal(dispatched.length,0);
  await settle(()=>!root.querySelector('button[data-command="122"]').disabled);assert.doesNotMatch(root.textContent,/Repairs started/);
  root.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));assert.equal(dispatched.length,0);assert.match(root.querySelector('[data-region="confirmation"]').textContent,/Visible request fields \(5\)/);
  edit('2');assert.equal(last.confirmation,null);assert.equal(dispatched.length,0);edit('3');await settle(()=>!root.querySelector('button[data-command="122"]').disabled);
  root.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));root.querySelector('[data-region="confirmation"] button').click();await settle(()=>root.dataset.phase==='error');
  assert.equal(dispatched.length,1);assert.deepEqual(dispatched[0].input,fixture.reply.input);assert.equal(dispatched[0].principal,principal);assert.equal(dispatched[0].contract,contract);
  assert.match(root.querySelector('[data-region="verdict"]').textContent,/Outcome unknown\./);assert.match(root.querySelector('[data-region="verdict"]').textContent,/sign in again and recover it/);
  assert.equal(last.delivery,'unknown');assert.equal(last.confirmation,null);
 }finally{restore.reverse().forEach(reset=>reset());}
});
