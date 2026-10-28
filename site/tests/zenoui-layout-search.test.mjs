import assert from 'node:assert/strict';
import {test} from 'node:test';
import {describe,canonical} from '../public/zenoui/app-contract.js';
import {synthesizePresentation,verifyPresentation} from '../public/zenoui/layout-search.js';
import {searchUi} from '../public/zenoui/ui-search.js';
import {validateAst} from '../public/zenoui/ui-grammar.js';
import {initial,step,view,project} from '../public/zenoui/app-state.js';

// Non-swap inventory workflow uses the same grammar and pure projection primitives.
const raw={fields:[
  {path:'pre.stock',index:0,source:'state',type_name:'Stock',initial:'10',domain:{kind:'int',min:'0',max:'20'}},
  {path:'command.action',index:1,source:'command',type_name:'Action',initial:'1',domain:{kind:'sum'},options:[{value:'1'},{value:'2'}]},
  {path:'command.quantity',index:2,source:'command',type_name:'Quantity',initial:'3',domain:{kind:'int',min:'0',max:'10'}},
  {path:'command.limit',index:3,source:'command',type_name:'Limit',initial:'2',domain:{kind:'int',min:'0',max:'10'}},
  {path:'context.fixed',index:4,source:'context',type_name:'Context',initial:'0',domain:{kind:'int',min:'0',max:'0'}}
],reasons:[{id:'9'}],domain:'4620',enablement:{table:[false,false,false,true]}};
const requirements={format:'zenoui/requirements/2',primaryFields:['command.quantity'],detailFields:['command.limit'],fixedFields:['context.fixed'],contextFields:[],groups:[],groupCommands:true,collapseExpert:true,compact:true,maxVisibleForms:2,maxExpandedExpertPanels:0,maxColumns:2,
  projections:[{id:'capacity',label:'Remaining capacity',kind:'state-complement',fieldByCommand:{1:'pre.stock',2:'pre.stock'},total:'20'},
    {id:'outgoing',label:'Stock released by preview',kind:'accepted-decrease',fieldByCommand:{1:'pre.stock',2:'pre.stock'}}],quoteOverrides:{'command.limit':'0'}};
const metadata={format:'zenoui/metadata/2',title:'Inventory operations',action:'command.action',fields:Object.fromEntries(raw.fields.map(f=>[f.path,{label:f.path==='command.quantity'?'Quantity':f.path,unit:f.source==='state'?'items':'units'}])),variants:{1:'Ship',2:'Release'},commands:{1:{label:'Ship',consequential:true},2:{label:'Release',consequential:true}},reasons:{9:'Unavailable'},requirements};
const app=()=>describe(structuredClone(raw),structuredClone(metadata));
const contract='a'.repeat(64);
const response=(intent,kind='Accept',post=['7'])=>({type:'Preview',binding:intent.binding,reply:{input:intent.binding.input,checked:'1',total:'1',class:kind,reason:kind==='Reject'?'9':null,post:kind==='Reject'||kind==='Refused'?[]:post}});
function ready(){let r=step(initial(app(),contract),{type:'Refresh'});const normal=r.intents.find(i=>i.binding.request==='1'),quote=r.intents.find(i=>i.binding.request==='quote:1');let s=step(r.state,response(normal)).state;return {s,normal,quote};}

test('recursive component search selects a checked AST with replayable bounded search',()=>{
  const a=app();verifyPresentation(a);assert.equal(a.searchReceipt.examined,2048);assert.ok(a.searchReceipt.feasible>0);
  assert.equal(a.presentation.ast.kind,'page');assert.equal(a.presentation.summary.visibleForms,1);assert.equal(a.presentation.summary.expandedExpertPanels,0);
  assert.equal(validateAst(a,requirements,a.presentation.ast).valid,true);assert.equal(canonical(app()),canonical(a));
  const alternative={...requirements,groupCommands:false,compact:false};
  const other=synthesizePresentation(a,alternative);assert.notEqual(canonical(other.presentation.ast),canonical(a.presentation.ast));
  const constrained=synthesizePresentation(a,{...alternative,maxVisibleForms:1,maxExpandedExpertPanels:0,maxColumns:1});
  assert.equal(constrained.presentation.summary.visibleForms,1);assert.equal(constrained.presentation.summary.expandedExpertPanels,0);assert.equal(constrained.presentation.summary.columns,1);
  const direct=searchUi(a,requirements,{mode:'direct',maxCandidates:128});const fragments=searchUi(a,requirements,{mode:'fragments',maxCandidates:128});
  assert.equal(validateAst(a,requirements,direct.ast).valid,true);assert.equal(validateAst(a,requirements,fragments.ast).valid,true);
});
test('receipt or chosen component tree cannot be forged independently of recomputation',()=>{
  for(const mutate of [a=>a.searchReceipt.examined=0,a=>a.searchReceipt.selectedScore[0]=99,a=>a.presentation.ast.children.pop(),a=>a.presentation.summary.nodes=0,a=>a.layout.reverse(),a=>a.searchReceipt.grammar='untrusted/1']){
    const a=structuredClone(app());mutate(a);assert.throws(()=>verifyPresentation(a),/UiPresentationReceiptMismatch|UiCompositionRefused/);assert.throws(()=>initial(a,contract),/UiPresentationReceiptMismatch|UiCompositionRefused/);
  }
});
test('closed grammar rejects injection, bad binding, duplicate coverage, limits and malformed metadata',()=>{
  const mutations=[r=>r.script='alert(1)',r=>r.format='future/2',r=>r.primaryFields.push('command.quantity'),r=>r.detailFields=[],r=>r.fixedFields=['command.limit'],r=>r.projections[0].fieldByCommand[1]='command.quantity',r=>r.projections[0].fieldByCommand[3]='pre.stock',r=>r.projections[0].total='19',r=>r.projections[0].total='1'.repeat(41),r=>r.projections[0].label='<script>',r=>r.projections[0].unit='USD',r=>r.projections.push(structuredClone(r.projections[0])),r=>r.quoteOverrides={'command.quantity':'10'},r=>r.quoteOverrides={'command.action':'2'},r=>r.quoteOverrides={'context.fixed':'0'},r=>r.quoteOverrides={'command.limit':'11'},r=>r.quoteOverrides={'command.limit':0},r=>r.maxVisibleForms=0,r=>r.maxVisibleForms=17,r=>r.maxExpandedExpertPanels=6,r=>r.maxColumns=3,r=>r.compact='yes'];
  for(const change of mutations){const m=structuredClone(metadata);change(m.requirements);assert.throws(()=>describe(raw,m),/^Error: Ui/);}
  for(const key of ['html','css','events'])assert.throws(()=>describe(raw,{...metadata,[key]:'x'}),/UiInvalidMetadata/);
  const m=structuredClone(metadata);m.requirements.projections=Array.from({length:17},(_,i)=>({...requirements.projections[0],id:`capacity-${i}`}));assert.throws(()=>describe(raw,m),/UiPresentationLimit/);
});
test('display projection is exact across complete bounded inventory domain and uses source units',()=>{
  for(let stock=0;stock<=20;stock++){
    const s={...initial(app(),contract),values:[String(stock)]};const p=project(s,'1');assert.equal(p[0].value,String(20-stock));assert.equal(p[0].unit,'items');assert.equal(p[1].value,null);
  }
});
test('quote-only checked reply cannot enable dispatch and never mutates entered minimum',()=>{
  let r=step(initial(app(),contract),{type:'Refresh'});const q=r.intents.find(i=>i.binding.request==='quote:1');
  assert.equal(q.binding.input[3],'0');assert.equal(r.state.drafts['1'][3],'2');
  const s=step(r.state,response(q)).state;assert.equal(view(s).projections[1].value,'3');assert.equal(view(s).actions[0].enabled,false);
  assert.equal(step(s,{type:'Dispatch',command:'1'}).intents.length,0);assert.equal(view(s).quoteInputs[0].value,'0');
  assert.equal(step(s,response(q,'Accept',['6'])).state,s,'duplicate quote cannot change projection');
});
test('refused, rejected, partial, increasing, and stale replies never appear as receive output',()=>{
  for(const kind of ['Reject','Refused']){const {s,quote}=ready();assert.equal(project(step(s,response(quote,kind)).state,'1')[1].value,null);}
  {const {s,quote}=ready();assert.equal(project(step(s,response(quote,'Accept',['11'])).state,'1')[1].value,null);}
  for(const key of ['epoch','run','request','revision','principal','contract']){
    const {s,quote}=ready();const event=response(quote);event.binding={...event.binding,[key]:'stale'};assert.equal(step(s,event).state,s);
  }
  const {s,quote}=ready();const incomplete=response(quote);incomplete.reply.checked='0';assert.throws(()=>step(s,incomplete),/UiInvalidCoreReply/);
});
test('review projection is immutable even with a late quote; edits, direction, principal and snapshots invalidate consent',()=>{
  const {s,quote}=ready();let reviewed=step(s,{type:'Review',command:'1'}).state;
  assert.equal(view(reviewed).confirmation.projections[1].value,null);
  reviewed=step(reviewed,response(quote)).state;assert.equal(view(reviewed).projections[1].value,'3');assert.equal(view(reviewed).confirmation.projections[1].value,null);
  reviewed=step(reviewed,{type:'Review',command:'1'}).state;assert.equal(view(reviewed).confirmation.projections[1].value,'3');
  const consent=step(reviewed,{type:'Confirm'}).state;
  const sent=step(consent,{type:'Dispatch',command:'1'});assert.equal(sent.intents.length,1);assert.equal(sent.intents[0].binding.input[3],'2');
  for(const event of [{type:'Edit',command:'1',index:2,value:'4'},{type:'Edit',command:'1',index:3,value:'1'},{type:'Select',command:'2'},{type:'Principal',value:'Other'},{type:'Snapshot',revision:'1',values:['10']},{type:'Refresh'}]){
    const changed=step(consent,event).state;assert.equal(changed.review,null);assert.equal(changed.reviewProjections,null);assert.equal(changed.consent,null);assert.equal(project(changed,'1')[1].value,null);assert.equal(step(changed,{type:'Dispatch',command:'1'}).intents.length,0);
  }
  assert.equal(step(consent,{type:'Edit',command:'1',index:4,value:'1'}).state,consent,'fixed context cannot be changed by hidden event');
  assert.throws(()=>{view(reviewed).confirmation.projections[0].value='999';},TypeError);
});
test('presentation preferences preserve exact decision and consent bindings',()=>{
  const {s,quote}=ready();const prepared=step(s,response(quote)).state;
  const run=preferences=>{let current=step(prepared,{type:'Preferences',value:preferences}).state;for(const event of [{type:'Review',command:'1'},{type:'Confirm'}])current=step(current,event).state;return step(current,{type:'Dispatch',command:'1'}).intents;};
  assert.deepEqual(run({width:360,theme:'dark',motion:'reduce'}),run({width:1440,theme:'light',motion:'normal'}));
});
