import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe,checkPresentation,verifyPresentation,formFields} from '../public/zenoui/app-contract.js';
import {initial,step,view} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {editFixture,approvalFixture} from './idiom-fixtures.mjs';

const clone=structuredClone,contract='a'.repeat(64),principal='b'.repeat(64);
function legacyFixture(){
  const {raw,meta}=editFixture();
  const fields=Object.fromEntries(Object.entries(meta.fields).map(([k,v])=>[k,{label:v.label,unit:v.unit}]));
  const commands=Object.fromEntries(Object.entries(meta.commands).map(([k,v])=>[k,{label:v.label,consequential:v.consequential}]));
  return {raw,meta:{format:'zenoui/metadata/1',title:meta.title,action:meta.action,fields,variants:meta.variants,commands,reasons:meta.reasons}};
}
function find(node,pred){if(pred(node))return node;for(const child of node.children??[]){const found=find(child,pred);if(found)return found;}}
function setup(app,values){
  let state=initial(app,contract);state=step(state,{type:'Principal',value:principal}).state;
  const fact=app.fields.find(f=>f.source==='context'&&!f.clock);
  if(fact)state=step(state,{type:'Context',index:fact.index,value:'1'}).state;
  const r=step(state,{type:'Snapshot',revision:'0',values,observation:{contract,principal,pending:'0'}});
  return r;
}
function accept(intent,post){return {input:intent.binding.input,class:'Accept',reason:null,post,checked:'1',total:'1'};}
function previewAll(start,post){let s=start.state;for(const intent of start.intents)s=step(s,{type:'Preview',binding:intent.binding,reply:accept(intent,post)}).state;return s;}

test('metadata/1 still generates a checked generic descriptor and rejects metadata/2 fields',()=>{
  const {raw,meta}=legacyFixture(),fresh=describe(raw,meta);
  assert.equal(checkPresentation(fresh),true);assert.equal(verifyPresentation(fresh),true);
  assert.notEqual(find(fresh.presentation.ast,n=>n.kind==='idiom')?.idiom,'scalar-edit');
  for(const change of [m=>{m.commands['120'].uiRole='update';},m=>{m.fields['command.101.121'].stateField='pre.100.110';},
                        m=>{m.fields['command.101.121'].role='requested-value';}]){
    const bad=clone(meta);change(bad);assert.throws(()=>describe(raw,bad),/UiInvalid/);
  }
});

test('old, future and missing receipt versions are refused at fast mount and full replay',()=>{
  const apps=[editFixture(),approvalFixture(),legacyFixture()].map(({raw,meta})=>describe(raw,meta));
  const variants={oldGrammar:a=>{a.searchReceipt.grammar='zenoui/ordered-interval-ui/1';},
    futureGrammar:a=>{a.searchReceipt.grammar='zenoui/ordered-interval-ui/999';},
    oldChecker:a=>{a.searchReceipt.checker='zenoui/component-checker/2';},
    futureChecker:a=>{a.searchReceipt.checker='zenoui/component-checker/999';},
    oldReceipt:a=>{a.searchReceipt.format='zenoui/ui-search/0';},
    futureReceipt:a=>{a.searchReceipt.format='zenoui/ui-search/999';},
    missingReceipt:a=>{delete a.searchReceipt.format;}};
  const accepted=[];
  for(const [index,app] of apps.entries())for(const [name,mutate] of Object.entries(variants)){
    const changed=clone(app);mutate(changed);
    for(const [label,check] of [['fast',checkPresentation],['full',verifyPresentation]]){
      try{if(check(changed))accepted.push(`${index}:${name}:${label}`);}catch(e){assert.match(e.message,/UiPresentationReceiptMismatch/);}
    }
  }
  assert.deepEqual(accepted,[],'unknown or old receipt schema must never mount');
});

test('receipt schema guard kills an otherwise accepted mutated receipt',async()=>{
  const file=new URL('../public/zenoui/layout-search.js',import.meta.url),source=await readFile(file,'utf8');
  const guard="receipt.format!=='zenoui/ui-search/1'||";assert.ok(source.includes(guard));
  const resolved=source.replace(guard,'').replace(/(from\s+)(['"])(\.\/[^'"]+)\2/g,(_,prefix,quote,specifier)=>prefix+JSON.stringify(new URL(specifier,file).href));
  const mutant=await import('data:text/javascript;base64,'+Buffer.from(resolved).toString('base64'));
  const {raw,meta}=editFixture(),app=clone(describe(raw,meta));app.searchReceipt.format='zenoui/ui-search/999';
  assert.throws(()=>checkPresentation(app),/UiPresentationReceiptMismatch/);
  assert.equal(mutant.checkPresentation(app),true,'removing the guard exposes the invalid receipt');
});

test('approval selection after consent clears it and stale replies cannot revive it',()=>{
  const {raw,meta}=approvalFixture(),app=describe(raw,meta),start=setup(app,['0']);
  let s=previewAll(start,['1']);const old=start.intents[0].binding;
  s=step(s,{type:'Review',command:'220'}).state;s=step(s,{type:'Confirm'}).state;assert.ok(s.consent);
  const switched=step(s,{type:'Select',command:'221'});s=switched.state;
  assert.equal(s.consent,null);assert.equal(s.review,null);
  assert.equal(step(s,{type:'Preview',binding:old,reply:accept({binding:old},['1'])}).state,s);
  assert.equal(step(s,{type:'Dispatch',command:'220'}).intents.length,0);
  assert.equal(step(s,{type:'Dispatch',command:'221'}).intents.length,0);
  s=previewAll(switched,['2']);s=step(s,{type:'Review',command:'221'}).state;s=step(s,{type:'Confirm'}).state;
  const sent=step(s,{type:'Dispatch',command:'221'});assert.equal(sent.intents.length,1);
  const binding=sent.state.active,reply=accept({binding},['2']);
  for(const key of ['epoch','run','request','command','revision','principal','contract']){
    const stale={...binding,[key]:'stale'};assert.equal(step(sent.state,{type:'Result',binding:stale,reply}).state,sent.state);
  }
  assert.throws(()=>step(sent.state,{type:'Result',binding,reply:{...reply,input:['0','220','1']}}),/UiInvalidCoreReply/);
  assert.throws(()=>step(sent.state,{type:'Result',binding,reply:{...reply,checked:'0'}}),/UiInvalidCoreReply/);
  const finished=step(sent.state,{type:'Result',binding,reply,delivery:'pending'}).state;
  assert.deepEqual(finished.values,['2']);assert.equal(finished.revision,'1');assert.equal(finished.delivery,'pending');
  assert.equal(step(finished,{type:'Result',binding,reply}).state,finished,'duplicate result is inert');
});

test('edit review shows the real core preview rather than the requested number',()=>{
  const {raw,meta}=editFixture(),app=describe(raw,meta);let s=previewAll(setup(app,['2']),['4']);
  const edited=step(s,{type:'Edit',command:'120',index:2,value:'7'});s=previewAll(edited,['4']);
  s=step(s,{type:'Review',command:'120'}).state;
  assert.deepEqual(view(s).confirmation.changes,[{label:'Current stock',before:'2 items',after:'4 items'}]);
  assert.equal(view(s).confirmation.fields.find(f=>f.index===2).value,'7 items');
  assert.deepEqual(formFields(app,'120').map(f=>f.index),[2]);
});

test('hidden status remains absent with masked replies and server context stays noneditable',()=>{
  const {raw,meta}=approvalFixture(),app=describe(raw,meta),start=setup(app,[null]);
  let s=previewAll(start,[null]);assert.deepEqual(view(s).state,[]);assert.deepEqual(formFields(app,'220'),[]);
  assert.equal(step(s,{type:'Context',index:2,value:'0'}).state,s,'server-owned context is locked');
  s=step(s,{type:'Review',command:'220'}).state;s=step(s,{type:'Confirm'}).state;
  const sent=step(s,{type:'Dispatch',command:'220'});assert.equal(sent.intents.length,1);
  const binding=sent.state.active,finished=step(sent.state,{type:'Result',binding,reply:accept({binding},[null]),delivery:'pending'}).state;
  assert.deepEqual(view(finished).state,[]);
  const rendered=presentationView(app,view(finished),{server:true});
  for(const node of Object.values(rendered.nodes))if(node.rows)assert.ok(node.rows.every(row=>row.label!=='Request status'));
});

test('ambiguous multiple approval subjects fall back instead of selecting the specialized idiom',()=>{
  const {raw,meta}=approvalFixture(),other={...clone(raw.fields[0]),path:'pre.200.211',index:3};raw.fields.push(other);raw.domain='36';
  meta.fields[other.path]={label:'Other request',unit:'unitless',role:'approval-status',values:{'0':'Pending','1':'Approved','2':'Declined'}};
  const app=describe(raw,meta);assert.notEqual(find(app.presentation.ast,n=>n.kind==='idiom')?.idiom,'approval-decision');
  assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);
});
