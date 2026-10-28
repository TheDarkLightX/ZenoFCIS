// Retained native observations exercise actual shared factory modules; this is not a fresh native/browser result.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe,checkPresentation,verifyPresentation,formFields} from '../public/zenoui/app-contract.js';
import {initial,step,view,inputFor} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {buildTree} from '../public/zenoui/ui-renderer.js';
import {buildFragmentCache,searchUi} from '../public/zenoui/ui-search.js';
import {documentFrom} from './browser-fixture.mjs';
import {approvalFixture} from './idiom-fixtures.mjs';

const fixture=JSON.parse(await readFile(new URL('./test-data/workflow-ui-native.json',import.meta.url),'utf8'));
const app=describe(fixture.descriptor,fixture.metadata),contract=fixture.authorityContract,principal='01'.repeat(32);
function ready(){
 let s=initial(app,contract);s=step(s,{type:'Principal',value:principal}).state;
 s=step(s,{type:'Snapshot',values:['150',null,'0'],revision:'0',observation:{contract,principal,pending:'0'}}).state;
 const edited=step(s,{type:'Edit',command:'122',index:4,value:'3'}),binding=edited.intents.find(i=>i.binding.command==='122').binding;
 assert.deepEqual(binding.input,fixture.reply.input);
 return {s:step(edited.state,{type:'Preview',binding,reply:fixture.reply}).state,binding};
}
function withDocument(fn){
 const previous=Object.getOwnPropertyDescriptor(globalThis,'document'),doc=documentFrom('<div id="app"></div>');
 Object.defineProperty(globalThis,'document',{value:doc,writable:true,configurable:true});
 try{return fn(doc);}finally{if(previous)Object.defineProperty(globalThis,'document',previous);else delete globalThis.document;}
}
function treeFor(doc,data,handlers={}){
 const rendered=presentationView(app,data,{server:true}),tree=buildTree(app.presentation.ast,rendered,{review(){},select(){},refresh(){},edit(){},context(){},principal(){},confirm(){},...handlers});
 doc.getElementById('app').append(tree.element);tree.update(rendered);return {rendered,tree};
}

test('current native accepted preview leads navigation without hiding command inputs',()=>withDocument(doc=>{
 const {s}=ready(),data=view(s);assert.equal(data.actions.find(a=>a.primary)?.id,'122');
 const calls=[],{rendered,tree}=treeFor(doc,data,{review(){calls.push('review');},select(){calls.push('select');},confirm(){calls.push('confirm');}});
 const commands=Object.values(rendered.nodes).find(n=>n.next);assert.equal(commands.next.command,'122');
 assert.equal(tree.element.querySelectorAll('form').length,5);
 for(const command of app.commands){assert.deepEqual(formFields(app,command.id).map(f=>f.index),[4]);assert.ok(tree.element.querySelector(`#command-${command.id}-field-4`));}
 const link=tree.element.querySelector('.zu-next-action');assert.equal(link.textContent,'Go to Check in item');link.click();
 assert.equal(doc.activeElement?.id,'action-122-title');assert.deepEqual(calls,[],'navigation never reviews, confirms or dispatches');
 const form=tree.element.querySelector('form[data-command="122"]');assert.equal(form.dataset.ready,'true');
 const preview=form.querySelector('.zu-action-preview').textContent;assert.match(preview,/Waiting for repair/);assert.match(preview,/3 units/);assert.doesNotMatch(preview,/Repairs started/);
 assert.equal(tree.element.querySelectorAll('form[data-ready="true"]').length,1);
 assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);
}));

test('editing or stale/wrong previews cannot retain ready navigation or visible changes',()=>{
 const {s,binding}=ready(),edited=step(s,{type:'Edit',command:'122',index:4,value:'2'}).state;
 for(const key of ['run','request','contract','principal','revision']){
  const bad={...binding,[key]:'stale'};assert.equal(step(edited,{type:'Preview',binding:bad,reply:fixture.reply}).state,edited);
 }
 assert.equal(step(edited,{type:'Preview',binding,reply:fixture.reply}).state,edited);
 const data=view(edited);assert.ok(data.actions.every(a=>a.changes.length===0));
 assert.ok(Object.values(presentationView(app,data,{server:true}).nodes).filter(n=>Object.hasOwn(n,'next')).every(n=>n.next===null));
 assert.throws(()=>step(s,{type:'Preview',binding,reply:{...fixture.reply,checked:'0'}}),/UiInvalidCoreReply/);
});

test('confirmation calls the masked subset visible and keeps exact request binding',()=>withDocument(doc=>{
 let {s}=ready();assert.equal(step(s,{type:'Dispatch',command:'122'}).intents.length,0);
 s=step(s,{type:'Review',command:'122'}).state;const data=view(s),{tree,rendered}=treeFor(doc,data);
 assert.ok(!data.confirmation.fields.some(f=>f.part==='state'&&f.index===1));assert.equal(data.confirmation.fields.length,5);
 assert.equal(inputFor(s,'122').length,6);assert.equal(inputFor(s,'122')[1],null);
 assert.match(tree.element.querySelector('[data-region="confirmation"]').textContent,/Visible request fields \(5\)/);
 assert.doesNotMatch(tree.element.querySelector('[data-region="confirmation"]').textContent,/Complete request: all/);
 const acting=Object.values(rendered.nodes).find(n=>n.label==='Your access');assert.ok(acting);assert.match(acting.text,/Technician permission: No/);
 assert.ok(Object.values(rendered.nodes).filter(n=>Object.hasOwn(n,'next')).every(n=>n.next===null));
 s=step(s,{type:'Confirm'}).state;const sent=step(s,{type:'Dispatch',command:'122'});assert.equal(sent.intents.length,1);assert.deepEqual(sent.intents[0].binding.input,fixture.reply.input);
}));

test('status/facts lead metrics and changed search receipts/caches require regeneration',()=>{
 const displays=[];const visit=n=>{if(n.kind==='display')displays.push(n);for(const c of n.children??[])visit(c);};visit(app.presentation.ast);
 assert.equal(displays[0].idiom,'facts');assert.ok(displays[0].fields.includes('pre.100.110'));
 assert.equal(app.searchReceipt.grammar,'zenoui/ordered-interval-ui/3');
 for(const grammar of ['zenoui/ordered-interval-ui/1','zenoui/ordered-interval-ui/2','zenoui/ordered-interval-ui/999']){
  const bad=structuredClone(app);bad.searchReceipt.grammar=grammar;assert.throws(()=>checkPresentation(bad),/UiPresentationReceiptMismatch/);assert.throws(()=>verifyPresentation(bad),/UiPresentationReceiptMismatch/);
 }
 const {cache}=buildFragmentCache({fragmentSize:2});assert.ok(cache);
 for(const grammar of ['zenoui/ordered-interval-ui/2','zenoui/ordered-interval-ui/999']){
  const bad=structuredClone(cache);bad.identity.grammar=grammar;assert.throws(()=>searchUi(app,app.presentation.requirements,{cache:bad,fragmentSize:2,maxCandidates:1}),/UiFragmentCacheMismatch/);
 }
});

// Both choices can have accepted previews. Navigation follows the user's current choice.
test('ready navigation preserves a selected paired decision',()=>{
 const {raw,meta}=approvalFixture(),decision=describe(raw,meta),selected=step(initial(decision,contract),{type:'Select',command:'221'});let s=selected.state;
 for(const {binding} of selected.intents)s=step(s,{type:'Preview',binding,reply:{input:binding.input,class:'Accept',reason:null,post:[binding.command==='220'?'1':'2'],checked:'1',total:'1'}}).state;
 const data=view(s);assert.equal(data.selectedCommand,'221');assert.equal(data.actions.find(a=>a.primary)?.id,'221');
 const next=Object.values(presentationView(decision,data,{server:true}).nodes).find(n=>Object.hasOwn(n,'next')).next;assert.equal(next.command,'221');
 assert.equal(step(s,{type:'Dispatch',command:'221'}).intents.length,0,'navigation does not create consent');
});
