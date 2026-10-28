// Pure view/DOM controls over a retained native preview; no fresh native/browser execution.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe} from '../public/zenoui/app-contract.js';
import {initial,step,view} from '../public/zenoui/app-state.js';
import {buildTree} from '../public/zenoui/ui-renderer.js';
import {documentFrom} from './browser-fixture.mjs';
const {presentationView}=await import(process.env.ZENO_TEST_VIEW_MODULE??'../public/zenoui/ui-view.js');
const fixture=JSON.parse(await readFile(new URL('./test-data/workflow-ui-native.json',import.meta.url),'utf8'));
const app=describe(fixture.descriptor,fixture.metadata),principal='01'.repeat(32),contract=fixture.authorityContract;
const guidance='The request outcome is unknown. If an original request was saved, sign in again and recover it.';
function dispatched(){
 let s=step(initial(app,contract),{type:'Principal',value:principal}).state;
 s=step(s,{type:'Snapshot',values:['150',null,'0'],revision:'0',observation:{contract,principal,pending:'0'}}).state;
 const edit=step(s,{type:'Edit',command:'122',index:4,value:'3'}),binding=edit.intents.find(i=>i.binding.command==='122').binding;
 s=step(edit.state,{type:'Preview',binding,reply:fixture.reply}).state;
 s=step(s,{type:'Review',command:'122'}).state;s=step(s,{type:'Confirm'}).state;
 const sent=step(s,{type:'Dispatch',command:'122'});return {s:sent.state,binding:sent.intents[0].binding};
}
function failed(){const {s,binding}=dispatched();return step(s,{type:'Fail',binding}).state;}
function status(rendered){return Object.values(rendered.nodes).find(n=>Object.hasOwn(n,'delivery')&&Object.hasOwn(n,'pending'));}

test('unknown outcome leads the DOM without cancellation and retains recovery guidance',()=>{
 const data=view(failed()),before=structuredClone(data),rendered=presentationView(app,data,{server:true,delivery:guidance});
 assert.equal(status(rendered).title,'Outcome unknown.','UNKNOWN_OUTCOME_HEADLINE');
 assert.equal(status(rendered).message,'No confirmed result is available.');assert.equal(status(rendered).delivery,guidance);
 const prior=Object.getOwnPropertyDescriptor(globalThis,'document'),doc=documentFrom('<div id="app"></div>');
 Object.defineProperty(globalThis,'document',{value:doc,configurable:true});let calls=0;
 try{
  const handlers=Object.fromEntries(['refresh','review','confirm','edit','select','context','principal'].map(k=>[k,()=>{calls++;}]));
  const tree=buildTree(app.presentation.ast,rendered,handlers);doc.getElementById('app').append(tree.element);tree.update(rendered);
  const verdict=tree.element.querySelector('[data-region="verdict"]'),text=verdict.textContent;
  assert.match(text,/Outcome unknown\./);assert.match(text,/sign in again and recover it/);
  assert.doesNotMatch(text,/stopped|cancelled|canceled|not committed|did not commit/i);
  assert.equal(verdict.querySelector('[role="status"]').getAttribute('aria-live'),'polite');
  assert.doesNotMatch(tree.element.textContent,/Repairs started/);assert.equal(calls,0);
 }finally{if(prior)Object.defineProperty(globalThis,'document',prior);else delete globalThis.document;}
 assert.deepEqual(data,before);assert.equal(data.phase,'error');assert.equal(data.delivery,'unknown');assert.equal(data.confirmation,null);
});

test('running, accepted, refused and known-error copy remain distinct',()=>{
 const sent=dispatched(),running=presentationView(app,view(sent.s),{server:true});
 assert.equal(status(running).title,'Checking.');
 const done=step(sent.s,{type:'Result',binding:sent.binding,reply:fixture.reply,delivery:'pending'}).state;
 const committed=presentationView(app,view(done),{server:true,delivery:'The server reported delivery pending for the last committed request.'});
 assert.equal(status(committed).title,'Done.');assert.match(status(committed).delivery,/delivery pending/);
 // This branch control is a constructed rejected DTO, not another native observation.
 const rejected={input:sent.binding.input,class:'Reject',reason:'200',post:[],checked:'1',total:'1'};
 const refused=step(sent.s,{type:'Result',binding:sent.binding,reply:rejected}).state;
 assert.equal(status(presentationView(app,view(refused),{server:true})).title,'Not accepted.');
 const known={...view(failed()),delivery:null};
 const ordinary=status(presentationView(app,known,{server:true}));assert.equal(ordinary.title,'Stopped.');assert.equal(ordinary.message,known.message);
});

test('only the unknown status title and message change in the view projection',()=>{
 const data=view(failed()),unknown=presentationView(app,data,{server:true,delivery:guidance});
 const ordinary=presentationView(app,{...data,delivery:null},{server:true,delivery:guidance});
 const clean=value=>structuredClone(value);
 const a=clean(unknown),b=clean(ordinary);
 for(const rendered of [a,b])for(const node of Object.values(rendered.nodes))if(Object.hasOwn(node,'delivery')){delete node.title;delete node.message;}
 assert.deepEqual(a,b);
});
