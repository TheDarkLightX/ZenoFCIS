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
 storage.clear();transport.terminate();
}
console.log(`durable UI: ${report.observations.length} native commits, mounted forms, consent, immutable context and original-key recovery passed`);
