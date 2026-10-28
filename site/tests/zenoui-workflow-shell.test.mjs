// Synthetic shell transport controls using one retained native preview DTO; not native/browser qualification.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import {describe} from '../public/zenoui/app-contract.js';
import {mount,sha256} from '../public/zenoui/app.js';
import {documentFrom,moduleLoader,ORIGIN} from './browser-fixture.mjs';
import {pendingKey} from '../public/zenoui/durable.js';

const fixture=JSON.parse(await readFile(new URL('./test-data/workflow-ui-native.json',import.meta.url),'utf8'));
const app=describe(fixture.descriptor,fixture.metadata),principal='01'.repeat(32);
async function bundle(){const value={description:structuredClone(app),source:fixture.source};return {...value,contract:await sha256(value)};}
const snapshot={contract:fixture.authorityContract,principal,revision:'0',values:['150',null,'0'],context:['0'],pending:'0'};
async function settle(predicate){for(let i=0;i<800;i++){if(predicate())return;await delay(5);}throw Error('workflow shell control timed out');}
function globals(values){const restore=Object.entries(values).map(([key,value])=>{const prior=Object.getOwnPropertyDescriptor(globalThis,key);Object.defineProperty(globalThis,key,{value,writable:true,configurable:true});return()=>prior?Object.defineProperty(globalThis,key,prior):delete globalThis[key];});return()=>restore.reverse().forEach(fn=>fn());}
const refused=binding=>({input:binding.input,class:'Refused',reason:null,post:[],checked:'1',total:'1'});
const isNative=binding=>JSON.stringify(binding.input)===JSON.stringify(fixture.reply.input);

test('missing delivery metadata states no confirmation and never claims no effects or success',async()=>{
 const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app'),restore=globals({document:doc,innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})});
 let last,dispatches=0;root.addEventListener('zenoui-view',e=>{last=e.detail;});
 const transport={terminate(){},postMessage(data){
  if(data.kind==='init')queueMicrotask(()=>this.onmessage({data:{kind:'ready'}}));
  else if(data.kind==='preview')queueMicrotask(()=>this.onmessage({data:{kind:'preview',binding:data.binding,reply:isNative(data.binding)?fixture.reply:refused(data.binding)}}));
  else{assert.ok(isNative(data.binding));dispatches++;queueMicrotask(()=>this.onmessage({data:{kind:'dispatch',binding:data.binding,reply:fixture.reply}}));}
 }};
 try{
  await mount(root,await bundle(),{session:snapshot,transport});await settle(()=>root.dataset.ready==='true');
  const input=root.querySelector('#command-122-field-4');input.value='3';input.dispatchEvent(new Event('input'));await settle(()=>!root.querySelector('button[data-command="122"]').disabled);
  assert.equal(dispatches,0);root.querySelector('form[data-command="122"]').dispatchEvent(new Event('submit',{cancelable:true}));
  root.querySelector('[data-region="confirmation"] button').click();await settle(()=>root.dataset.phase==='done');
  assert.equal(dispatches,1);assert.equal(last.delivery,'unavailable');
  const text=root.querySelector('[data-region="verdict"]').textContent;assert.match(text,/This response includes no delivery confirmation/);
  assert.doesNotMatch(text,/no effects|no deliveries|successfully delivered|delivery completed/i);
 }finally{restore();}
});

test('recovery is quiet without a retained request and discoverable without erasing one',async()=>{
 const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app'),data=await bundle(),storage=new Map();storage.getItem=k=>storage.get(k)??null;storage.setItem=(k,v)=>storage.set(k,v);storage.removeItem=k=>storage.delete(k);
 const credential='c'.repeat(64),fetcher=async(path,options)=>{
  assert.equal(options.headers.Authorization,'Bearer '+credential);
  if(path==='/state')return {ok:true,json:async()=>snapshot};
  if(path==='/app.json')return {ok:true,json:async()=>data};
  if(path==='/preview'){const input=JSON.parse(new TextDecoder().decode(options.body.slice(73)));return {ok:true,json:async()=>({reply:{input,class:'Refused',reason:null,post:[],checked:'1',total:'1'},command:'00'})};}
  throw Error('unexpected shell request '+path);
 };
 const loader=moduleLoader({document:doc,crypto:webcrypto,AbortController,fetch:fetcher,localStorage:storage,innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})});
 await loader.evaluate(`${ORIGIN}/ZenoFCIS/zenoui/durable-bootstrap.js`);
 root.querySelector('#credential').value=credential;root.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));
 try{await settle(()=>!!root.querySelector('.zu-recovery-foot'));}catch(error){error.message+='; sign-in view: '+root.textContent;throw error;}
 const footer=root.querySelector('.zu-recovery-foot'),recover=footer.querySelector('button');assert.equal(footer.hidden,true);assert.equal(recover.hidden,true);
 const retained='{"scope":"visibility-only fixture; recovery itself remains independently checked"}';storage.setItem(pendingKey,retained);root.dispatchEvent(new Event('zenoui-view'));
 assert.equal(footer.hidden,false);assert.equal(recover.hidden,false);assert.equal(storage.getItem(pendingKey),retained);
 storage.removeItem(pendingKey);root.dispatchEvent(new Event('zenoui-view'));assert.equal(footer.hidden,true);
});
