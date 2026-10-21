// Native ingress results replay through the real shared transport and mounted UI.
// This adapter checks event/data flow; Chrome supplies layout and input evidence.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {setTimeout as delay} from 'node:timers/promises';
import {documentFrom,moduleLoader} from './browser-fixture.mjs';
import {display} from '../public/zenoui/app-contract.js';
import {DurableTransport,pendingKey,hex,unhex,envelope} from '../public/zenoui/durable.js';
const report=JSON.parse(await readFile(process.argv[2],'utf8'));
const plain=v=>JSON.parse(JSON.stringify(v));
const settle=async predicate=>{const deadline=Date.now()+5000;while(!predicate()){assert.ok(Date.now()<deadline,'mounted journey deadline');await delay(5);}};
// Execute the shipped startup module, including refused and successful sign-in.
// The endpoint replies below come from this template's native HTTP journey.
{
 const observation=report.observations[0],{bundle}=report,d=bundle.description;
 const stateFields=d.fields.filter(f=>f.source==='state'),contextFields=d.fields.filter(f=>f.source==='context');
 const session={contract:observation.contract,principal:observation.principal,revision:observation.revision,values:observation.before.slice(0,stateFields.length),context:contextFields.map(f=>observation.before[f.index])};
 const credential=hex(webcrypto.getRandomValues(new Uint8Array(32))),requests=[];let descriptionUnavailable=false;
 const storage=new Map();storage.getItem=k=>storage.get(k)??null;storage.setItem=(k,v)=>storage.set(k,v);storage.removeItem=k=>storage.delete(k);
 // Like a browser, refuse fetch invoked on any receiver other than the window.
 const fetcher=async function(path,options){
  if(this!==undefined&&this!==loader?.context)throw TypeError("Failed to execute 'fetch' on 'Window': Illegal invocation");
  requests.push(path);assert.equal(options.cache,'no-store');assert.deepEqual(Object.keys(options.headers),['Authorization']);
  if(options.headers.Authorization!=='Bearer '+credential)return {ok:false,status:401};
  if(path==='/state')return {ok:true,json:async()=>plain(session)};
  if(path==='/app.json')return descriptionUnavailable?{ok:false,status:503}:{ok:true,json:async()=>plain(bundle)};
  if(path==='/preview'){
   const input=JSON.parse(new TextDecoder().decode(options.body.slice(73)));
   return {ok:true,json:async()=>({reply:JSON.stringify(input)===JSON.stringify(observation.before)?observation.preview:{input,class:'Refused',reason:null,post:[],checked:'1',total:'1'},command:observation.command})};
  }
  assert.equal(path,'/command');assert.equal(hex(options.body.slice(73)),observation.command);
  return {ok:true,json:async()=>plain(observation.response)};
 };
 let loader;
 const open=async()=>{
  const doc=documentFrom('<div id="app"></div>');
  loader=moduleLoader({document:doc,fetch:fetcher,localStorage:storage,crypto:webcrypto,AbortController,innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})});
  await loader.evaluate('https://zenoui.test/ZenoFCIS/zenoui/durable-bootstrap.js');return doc.getElementById('app');
 };
 const root=await open(),submit=value=>{root.querySelector('#credential').value=value;root.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));};
 assert.match(root.textContent,/Sign in required/);assert.equal(root.querySelector('[data-region="commands"]'),null);assert.equal(requests.length,0);
 submit('');await settle(()=>root.textContent.includes('Sign-in refused'));
 assert.equal(root.querySelector('[data-region="commands"]'),null);assert.equal(requests.length,0,'empty credential never sends');
 submit('00'.repeat(32));await settle(()=>root.textContent.includes('Check your application credential'));
 assert.equal(root.querySelector('[data-region="commands"]'),null);assert.deepEqual(requests,['/state']);assert.equal(root.querySelector('#credential').value,'');
 descriptionUnavailable=true;submit(credential);await settle(()=>root.textContent.includes('Request refused (503)'));
 assert.equal(root.querySelector('[data-region="commands"]'),null);assert.ok(root.querySelector('[data-region="signin"]'),'startup errors remain visible on the form');
 descriptionUnavailable=false;
 submit(credential);await settle(()=>root.querySelector('[data-region="commands"]'));
 assert.deepEqual(requests.slice(1,5),['/state','/app.json','/state','/app.json'],'authenticate before description and previews');
 assert.equal(root.querySelector('#credential'),null);assert.equal(storage.size,0,'no stored credential');
 // Reload asks again even when original-key recovery is available.
 const binding={contract:session.contract,principal:session.principal,revision:session.revision,input:observation.before};
 const saved=JSON.stringify({contract:session.contract,principal:session.principal,binding,body:hex(envelope(session.contract,observation.key,session.revision,unhex(observation.command)))});
 storage.setItem(pendingKey,saved);assert.ok(!saved.includes(credential));
 const reopened=await open();assert.match(reopened.textContent,/Sign in required/);assert.equal(reopened.querySelector('[data-region="commands"]'),null);
 reopened.querySelector('#credential').value=credential;reopened.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));
 await settle(()=>reopened.textContent.includes('A request is unresolved'));assert.equal(reopened.querySelector('[data-region="commands"]'),null);
 reopened.querySelector('button').click();await settle(()=>reopened.textContent.includes('Original request committed at revision'));
 assert.equal(storage.size,0);assert.ok(requests.includes('/command'));
 console.log('durable sign-in: empty/wrong credential refused without commands; authenticated startup, memory-only credential and reload recovery passed');
}
for(const observation of report.observations){
 const {bundle}=report,d=bundle.description;
 const stateFields=d.fields.filter(f=>f.source==='state'),contextFields=d.fields.filter(f=>f.source==='context');
 const session={contract:observation.contract,principal:observation.principal,revision:observation.revision,values:observation.before.slice(0,stateFields.length),context:contextFields.map(f=>observation.before[f.index])};
 const storage=new Map();storage.getItem=k=>storage.get(k)??null;storage.setItem=(k,v)=>storage.set(k,v);storage.removeItem=k=>storage.delete(k);
 let sends=0,lost=false;
 const fetcher=async(path,options)=>{
  const body=options.body;
  assert.equal(options.headers.Authorization,'Bearer '+'ab'.repeat(32));
  assert.equal(hex(body.slice(1,33)),session.contract);assert.equal(new DataView(body.buffer,body.byteOffset).getBigUint64(65).toString(),session.revision);
  if(path==='/preview'){
   const input=JSON.parse(new TextDecoder().decode(body.slice(73)));
   const reply=JSON.stringify(input)===JSON.stringify(observation.before)?observation.preview:{input,class:'Refused',reason:null,post:[],checked:'1',total:'1'};
   return {ok:true,json:async()=>({reply,command:observation.command})};
  }
  assert.equal(path,'/command');assert.ok(storage.getItem(pendingKey),'persist precedes send');
  assert.equal(hex(body.slice(73)),observation.command);sends++;
  if(lost){lost=false;throw Error('Lost reply');}
  return {ok:true,json:async()=>observation.response};
 };
 const transport=new DurableTransport(bundle,'ab'.repeat(32),session,{fetcher,storage,random:bytes=>{bytes.set(unhex(observation.key));return bytes;}});
 const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app');let lastView;
 root.addEventListener('zenoui-view',event=>{lastView=event.detail;});
 const loader=moduleLoader({document:doc,crypto:webcrypto,innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})});
 const module=await loader.evaluate('https://zenoui.test/ZenoFCIS/zenoui/app.js');
 await module.namespace.mount(root,plain(bundle),{session:plain(session),transport});
 const command=observation.before[d.action];
 for(const f of d.fields.filter(f=>f.source==='command'&&f.index!==d.action)){
  const input=root.querySelector(`#command-${command}-field-${f.index}`);input.value=observation.before[f.index];input.dispatchEvent(new Event('input'));
 }
 await settle(()=>lastView?.actions.some(a=>a.id===command&&a.enabled));
 assert.equal(root.querySelector('#principal').disabled,true,'principal is fixed');
 for(const f of contextFields)assert.equal(root.querySelector(`#command-${command}-field-${f.index}`).disabled,true,'context is fixed');
 const form=root.querySelector(`button[data-command="${command}"]`).parent;
 form.dispatchEvent(new Event('submit',{cancelable:true}));
 assert.ok(root.querySelector('[data-region="confirmation"]').textContent.includes(session.principal));
 root.querySelector('[data-region="confirmation"] button').click();
 await settle(()=>root.dataset.phase==='done');
 assert.equal(root.dataset.revision,(BigInt(session.revision)+1n).toString());assert.equal(sends,1);assert.equal(storage.getItem(pendingKey),null);
 assert.equal(lastView.ledger.at(-1).result,'Accept');
 assert.deepEqual(plain(lastView.state.map(f=>f.value)),stateFields.map((f,i)=>display(f,observation.result.post[i])));
 // Lost reply after application: reload retains exact body and retries original key.
 const binding={contract:session.contract,principal:session.principal,revision:session.revision,input:observation.before};
 const body=envelope(session.contract,observation.key,session.revision,unhex(observation.command));
 const saved=JSON.stringify({contract:session.contract,principal:session.principal,binding,body:hex(body)});storage.setItem(pendingKey,saved);lost=true;
 await assert.rejects(()=>transport.recover(),/Lost reply/);assert.equal(storage.getItem(pendingKey),saved);
 const reopened=new DurableTransport(bundle,'ab'.repeat(32),session,{fetcher,storage});const recovered=await reopened.recover();assert.deepEqual(recovered.result,observation.response);assert.equal(storage.getItem(pendingKey),null);
 // A principal substitution cannot consume the pending request.
 storage.setItem(pendingKey,saved);const wrong=new DurableTransport(bundle,'ab'.repeat(32),{...session,principal:'00'.repeat(32)},{fetcher,storage});await assert.rejects(()=>wrong.recover(),/another principal/);assert.equal(storage.getItem(pendingKey),saved);storage.clear();
 // Planted packet and evidence defects must not consume recovery state.
 for(const change of [r=>{r.packet=r.packet.slice(0,10)+'00'.repeat(32)+r.packet.slice(74);},r=>{r.reply.checked='0';}]){
   storage.setItem(pendingKey,saved);const invalid=structuredClone(observation.response);change(invalid);
   const bad=new DurableTransport(bundle,'ab'.repeat(32),session,{storage,fetcher:async()=>({ok:true,json:async()=>invalid})});
   await assert.rejects(()=>bad.recover());assert.equal(storage.getItem(pendingKey),saved);
 }
 storage.clear();transport.terminate();await assert.rejects(()=>transport.call('/state'),/Sign in again/);
}
console.log(`durable UI: ${report.observations.length} native commits, mounted forms, consent, immutable context and original-key recovery passed`);
