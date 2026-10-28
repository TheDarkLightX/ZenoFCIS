// A confirmed request is sent only when the server's fresh preview decides the
// same inputs; a preview for other inputs never reaches /command.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {test} from 'node:test';
import {setTimeout as delay} from 'node:timers/promises';
import {describe} from '../public/zenoui/app-contract.js';
import {initial,pendingSummary,step,view} from '../public/zenoui/app-state.js';
import {mount,sha256} from '../public/zenoui/app.js';
import {DurableTransport,envelope,hex,pendingKey} from '../public/zenoui/durable.js';
import {documentFrom} from './browser-fixture.mjs';

const raw={fields:[
  {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'9'},options:[],initial:'0'},
  {path:'command.101.120',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'},
  {path:'command.101.121',source:'command',index:2,domain:{kind:'int',min:'1',max:'9'},options:[],initial:'1'}],
  reasons:[],enablement:{table:[false,false,false,true]},domain:'90'};
const meta={format:'zenoui/metadata/1',title:'Amounts',action:'command.101.120',
  fields:{'pre.100.110':{label:'Count',unit:'items'},'command.101.120':{label:'Command',unit:'unitless'},'command.101.121':{label:'Amount',unit:'items'}},
  variants:{'120':'Add'},commands:{'120':{label:'Add',consequential:true}},reasons:{}};
const bundle={description:describe(raw,meta)};
const session={contract:'a'.repeat(64),principal:'b'.repeat(64)};
const binding={...session,revision:'0',input:['0','120','2']};

async function dispatch(decided) {
  const requests=[],storage=new Map();
  storage.getItem=k=>storage.get(k)??null;storage.setItem=(k,v)=>storage.set(k,v);storage.removeItem=k=>storage.delete(k);
  const fetcher=async(path)=>{
    requests.push(path);
    if(path==='/preview')return {ok:true,json:async()=>({reply:{input:decided,class:'Accept',reason:null,post:['2'],checked:'1',total:'1'},command:'00'})};
    return {ok:false,status:503};
  };
  const transport=new DurableTransport(bundle,'c'.repeat(64),session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const message=new Promise(resolve=>{transport.onmessage=({data})=>resolve(data);});
  transport.postMessage({kind:'dispatch',binding});
  const data=await message;
  return {requests,data,pending:storage.get(pendingKey)};
}

test('a_preview_of_other_inputs_is_never_dispatched',async()=>{
  for(const decided of [['0','120','3'],['1','120','2'],['0','120','2','0']]){
    const {requests,data,pending}=await dispatch(decided);
    assert.equal(data.kind,'error');
    assert.deepEqual(requests,['/preview'],`a changed preview was dispatched: ${decided}`);
    assert.equal(pending,undefined,'a changed request was saved for recovery');
  }
});
test('a_preview_of_the_confirmed_inputs_is_dispatched',async()=>{
  const {requests}=await dispatch(['0','120','2']);
  assert.deepEqual(requests,['/preview','/command'],'the confirmed request was not sent');
});

const accepted={input:binding.input,class:'Accept',reason:null,post:['2'],checked:'1',total:'1'};
const credential='c'.repeat(64);
function memory() {
  const items=new Map();
  items.getItem=k=>items.get(k)??null;items.setItem=(k,v)=>items.set(k,v);items.removeItem=k=>items.delete(k);
  return items;
}
const hash=(...parts)=>createHash('sha256').update(Buffer.concat(parts)).digest();
function packet(body) {
  const key=hash(Buffer.from('zeno-fcis/ingress-key/1'),Buffer.from(session.principal,'hex'),Buffer.from(session.contract,'hex'),body.subarray(33,65));
  const request=hash(Buffer.from('zeno-fcis/ingress-request/1'),key,body.subarray(65));
  const tail=Buffer.alloc(56);tail.writeBigUInt64BE(1n,16);
  return Buffer.concat([Buffer.from('ZFIP1'),request,key,tail]).toString('hex');
}
function response(body,metadata={}) {return {reply:accepted,packet:packet(Buffer.from(body)),revision:'1',...metadata};}
function retained() {
  const body=envelope(session.contract,'07'.repeat(32),'0',new Uint8Array([0]));
  return JSON.stringify({...session,binding,body:hex(body)});
}
async function resultFor(metadata) {
  const storage=memory(),writes=[];
  storage.setItem=(k,v)=>{writes.push(v);storage.set(k,v);};
  const fetcher=async(path,options)=>{
    if(path==='/preview')return {ok:true,json:async()=>({reply:accepted,command:'00'})};
    assert.equal(path,'/command');assert.ok(storage.getItem(pendingKey),'saved before sending');
    return {ok:true,json:async()=>response(options.body,metadata)};
  };
  const transport=new DurableTransport(bundle,credential,session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const message=new Promise(resolve=>transport.onmessage=({data})=>resolve(data));
  transport.postMessage({kind:'dispatch',binding});
  const data=await message;transport.terminate();
  assert.equal(data.kind,'dispatch');assert.equal(storage.getItem(pendingKey),null);
  assert.ok(writes.every(value=>!value.includes(credential)),'credential never enters retained request');
  assert.ok(!JSON.stringify(data).includes(credential),'credential never enters UI message');
  return data;
}
test('only_explicit_pending_survives_a_validated_commit',async()=>{
  assert.equal((await resultFor({delivery:'pending'})).delivery,'pending');
  for(const metadata of [{},{delivery:null},{delivery:'delivered'},{delivery:'PENDING'},{delivery:0},{delivery:{status:'pending'}}])
    assert.equal((await resultFor(metadata)).delivery,'unavailable','no missing or malformed field implies delivery');
});
test('lost_reply_reuses_original_request_and_wrong_principals_cannot_recover',async()=>{
  const storage=memory(),sent=[];let lost=true;
  const fetcher=async(path,options)=>{
    if(path==='/preview')return {ok:true,json:async()=>({reply:accepted,command:'00'})};
    sent.push(hex(options.body));
    if(lost){lost=false;throw Error('Lost reply');}
    return {ok:true,json:async()=>response(options.body,{delivery:'pending'})};
  };
  const transport=new DurableTransport(bundle,credential,session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const message=new Promise(resolve=>transport.onmessage=({data})=>resolve(data));
  transport.postMessage({kind:'dispatch',binding});
  assert.equal((await message).kind,'error');
  const saved=storage.getItem(pendingKey);assert.ok(saved);assert.ok(!saved.includes(credential));
  for(const other of [{...session,principal:'d'.repeat(64)},{...session,contract:'e'.repeat(64)}]){
    const wrong=new DurableTransport(bundle,credential,other,{fetcher,storage});
    await assert.rejects(()=>wrong.recover(),/another principal or contract/);
    assert.equal(storage.getItem(pendingKey),saved);
  }
  const recovered=await transport.recover();
  assert.equal(recovered.result.delivery,'pending');assert.equal(storage.getItem(pendingKey),null);
  assert.deepEqual(sent,[JSON.parse(saved).body,JSON.parse(saved).body],'recovery repeats the identical request');
  transport.terminate();
});
test('retired_or_rebound_recovery_does_not_consume_the_original_request',async()=>{
  for(const change of ['retired','principal','contract']){
    const storage=memory(),saved=retained();storage.setItem(pendingKey,saved);
    let entered,release;
    const started=new Promise(resolve=>entered=resolve),gate=new Promise(resolve=>release=resolve);
    const fetcher=async(_path,options)=>{entered();await gate;return {ok:true,json:async()=>response(options.body,{delivery:'pending'})};};
    const transport=new DurableTransport(bundle,credential,session,{fetcher,storage});
    const recovering=transport.recover();await started;
    if(change==='retired')transport.terminate();else transport.session[change]=(change==='principal'?'d':'e').repeat(64);
    release();await assert.rejects(recovering);
    assert.equal(storage.getItem(pendingKey),saved,'late result cannot consume the request');
    transport.terminate();
  }
});
test('retired_dispatch_cannot_emit_a_late_delivery_observation',async()=>{
  const storage=memory(),messages=[];let entered,release,finished;
  const started=new Promise(resolve=>entered=resolve),gate=new Promise(resolve=>release=resolve),done=new Promise(resolve=>finished=resolve);
  const fetcher=async(path,options)=>{
    if(path==='/preview')return {ok:true,json:async()=>({reply:accepted,command:'00'})};
    entered();await gate;return {ok:true,json:async()=>response(options.body,{delivery:'pending'})};
  };
  const transport=new DurableTransport(bundle,credential,session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const recover=transport.recover.bind(transport);
  transport.recover=async()=>{try{return await recover();}finally{finished();}};
  transport.onmessage=({data})=>messages.push(data);
  transport.postMessage({kind:'dispatch',binding});await started;
  const saved=storage.getItem(pendingKey);assert.ok(saved);
  transport.terminate();release();await done;await Promise.resolve();
  assert.deepEqual(messages,[],'retired transport sends no late status');
  assert.equal(storage.getItem(pendingKey),saved,'retirement preserves the original request');
});
function activeState(authenticated=true) {
  let s=structuredClone(initial(bundle.description,session.contract));s.principal=session.principal;
  const snapshot={type:'Snapshot',values:['0'],revision:'0'};
  if(authenticated)snapshot.observation={...session,pending:'2'};
  s=step(s,snapshot).state;
  s=step(s,{type:'Edit',command:'120',index:2,value:'2'}).state;
  const refresh=step(s,{type:'Refresh'});s=refresh.state;
  s=step(s,{type:'Preview',binding:refresh.intents[0].binding,reply:accepted}).state;
  s=step(s,{type:'Review',command:'120'}).state;s=step(s,{type:'Confirm'}).state;
  return step(s,{type:'Dispatch',command:'120'}).state;
}
test('delivery_observations_follow_the_current_binding',()=>{
  const fresh=structuredClone(initial(bundle.description,session.contract));fresh.principal=session.principal;
  for(const key of ['principal','contract']){
    const observation={...session,[key]:'d'.repeat(64),pending:'2'};
    assert.equal(step(fresh,{type:'Snapshot',values:['0'],revision:'0',observation}).state,fresh,`initial ${key} mismatch is ignored`);
  }
  const malformed=step(fresh,{type:'Snapshot',values:['0'],revision:'0',observation:{...session,pending:{count:2}}}).state;
  assert.equal(view(malformed).pendingAtSignIn.count,null,'invalid aggregate metadata is unavailable');
  const active=activeState();assert.equal(view(active).delivery,'unknown');
  assert.deepEqual(view(active).pendingAtSignIn,{revision:'0',count:'2'});
  for(const key of ['epoch','run','request','revision','principal','contract']){
    const stale={...active.active,[key]:'stale'};
    assert.equal(step(active,{type:'Result',binding:stale,reply:accepted,delivery:'pending'}).state,active,`${key} mismatch is ignored`);
  }
  assert.equal(step(active,{type:'Principal',value:'another'}).state,active,'principal cannot change during dispatch');
  assert.equal(step(active,{type:'Snapshot',values:['0'],revision:'0',observation:{...session,pending:'0'}}).state,active,'a late snapshot cannot interrupt a dispatch');
  const failed=step(active,{type:'Fail',binding:active.active}).state;
  assert.equal(view(failed).delivery,'unknown');
  assert.equal(step(failed,{type:'Result',binding:active.active,reply:accepted,delivery:'pending'}).state,failed,'late result after failure is ignored');
  const committed=step(active,{type:'Result',binding:active.active,reply:accepted,delivery:'pending'}).state;
  assert.equal(view(committed).delivery,'pending');assert.equal(committed.revision,'1');
  assert.equal(step(committed,{type:'Result',binding:active.active,reply:accepted,delivery:'pending'}).state,committed,'duplicate result is ignored');
  assert.equal(view(step(active,{type:'Result',binding:active.active,reply:accepted,delivery:'delivered'}).state).delivery,'unavailable');
  assert.equal(view(step(active,{type:'Result',binding:active.active,reply:accepted}).state).delivery,'unavailable');
  assert.equal(step(committed,{type:'Principal',value:'Another person'}).state,committed,'authenticated principal remains locked');
  const localActive=activeState(false);
  const localCommitted=step(localActive,{type:'Result',binding:localActive.active,reply:accepted,delivery:'pending'}).state;
  const changedPrincipal=step(localCommitted,{type:'Principal',value:'Another person'}).state;
  assert.equal(view(changedPrincipal).delivery,null);assert.equal(view(changedPrincipal).pendingAtSignIn,null);
  const wrong=step(committed,{type:'Snapshot',values:['2'],revision:'2',observation:{...session,principal:'d'.repeat(64),pending:'0'}}).state;
  assert.equal(wrong,committed,'wrong-principal snapshot is ignored');
  const stale=step(committed,{type:'Snapshot',values:['2'],revision:'0',observation:{...session,pending:'0'}}).state;
  assert.equal(stale,committed,'stale snapshot is ignored');
  const repeated=step(committed,{type:'Snapshot',values:['2'],revision:'1',observation:{...session,pending:'0'}}).state;
  assert.equal(repeated,committed,'sign-in observation cannot be replaced later');
  assert.match(pendingSummary('0','3'),/0 deliveries were pending in total/);
  assert.match(pendingSummary('1','3'),/1 delivery was pending in total/);
  for(const count of [undefined,null,0,'00','-1','18446744073709551616','x'.repeat(64)])assert.match(pendingSummary(count,'0'),/unavailable/);
});
async function settle(predicate) {
  const deadline=Date.now()+2000;while(!predicate()){assert.ok(Date.now()<deadline,'DOM journey deadline');await delay(5);}
}
function globalValue(name,value) {
  const prior=Object.getOwnPropertyDescriptor(globalThis,name);
  Object.defineProperty(globalThis,name,{configurable:true,writable:true,value});
  return ()=>prior?Object.defineProperty(globalThis,name,prior):delete globalThis[name];
}
test('bootstrap_recovers_pending_or_unavailable_without_claiming_delivery',async()=>{
  for(const [index,metadata] of [{delivery:'pending'},{}].entries()){
    const storage=memory(),saved=retained();storage.setItem(pendingKey,saved);
    const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app');
    const snapshot={...session,revision:'0',values:['0'],context:[],pending:'2'};
    const fetcher=async(path,options)=>{
      assert.equal(options.headers.Authorization,`Bearer ${credential}`);
      if(path==='/state')return {ok:true,json:async()=>snapshot};
      if(path==='/app.json')return {ok:true,json:async()=>bundle};
      assert.equal(path,'/command');assert.equal(hex(options.body),JSON.parse(saved).body);
      return {ok:true,json:async()=>response(options.body,metadata)};
    };
    const restore=[globalValue('document',doc),globalValue('localStorage',storage),globalValue('fetch',fetcher)];
    try{
      await import(`../public/zenoui/durable-bootstrap.js?effect-test=${index}`);
      root.querySelector('#credential').value=credential;
      root.querySelector('form').dispatchEvent(new Event('submit',{cancelable:true}));
      await settle(()=>root.textContent.includes('A request is unresolved'));
      assert.match(root.textContent,/2 deliveries were pending in total/,'aggregate snapshot is shown separately');
      root.querySelector('button').click();
      await settle(()=>root.textContent.includes('Original request committed'));
      assert.match(root.textContent,index===0?/server reported delivery pending for this request/:/The recovered response includes no delivery confirmation/);
      assert.doesNotMatch(root.textContent,/Delivery run completed|delivery succeeded/i);
      assert.equal(storage.getItem(pendingKey),null);
      assert.ok(!root.textContent.includes(credential),'credential is not rendered');
    }finally{restore.reverse().forEach(reset=>reset());}
  }
});
test('mounted_durable_page_shows_bound_delivery_observations',async()=>{
  for(const [index,delivery] of ['pending',undefined,'delivered'].entries()){
    const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app');
    const restore=[globalValue('document',doc),globalValue('innerWidth',390),globalValue('addEventListener',()=>{}),globalValue('matchMedia',()=>({matches:false,addEventListener:()=>{}}))];
    let submitted,lastView;
    root.addEventListener('zenoui-view',event=>{lastView=event.detail;});
    const transport={terminate(){},postMessage(data){
      if(data.kind==='init')queueMicrotask(()=>this.onmessage({data:{kind:'ready'}}));
      else if(data.kind==='preview')queueMicrotask(()=>this.onmessage({data:{kind:'preview',binding:data.binding,reply:{...accepted,input:data.binding.input}}}));
      else submitted=data.binding;
    }};
    try{
      const description=structuredClone(bundle.description),source='fixture',contract=await sha256({description,source});
      await mount(root,{description,source,contract},{session:{...session,revision:'0',values:['0'],context:[],pending:'0'},transport});
      await settle(()=>root.dataset.ready==='true'&&!root.querySelector('button[data-command="120"]').disabled);
      const region=root.querySelector('[data-region="verdict"]');
      assert.equal(root.querySelectorAll('[data-region]').length,8);
      assert.match(region.textContent,/0 deliveries were pending in total/);
      assert.equal(region.querySelectorAll('[role="status"]').length,2,'delivery changes use a live status');
      root.querySelector('form[data-command="120"]').dispatchEvent(new Event('submit',{cancelable:true}));
      root.querySelector('[data-region="confirmation"] button').click();
      await settle(()=>!!submitted);
      assert.match(region.textContent,/request outcome is unknown/i);
      transport.onmessage({data:{kind:'dispatch',binding:{...submitted,principal:'d'.repeat(64)},reply:{...accepted,input:submitted.input},delivery:'pending'}});
      assert.equal(lastView.delivery,'unknown','wrong-principal result cannot change the observation');
      transport.onmessage({data:{kind:'dispatch',binding:submitted,reply:{...accepted,input:submitted.input},delivery}});
      await settle(()=>root.dataset.phase==='done');
      assert.match(region.textContent,index===0?/server reported delivery pending for the last committed request/:/This response includes no delivery confirmation/);
      assert.match(region.textContent,/At sign-in \(revision 0\), 0 deliveries were pending in total/,'aggregate is explicitly historical');
      assert.doesNotMatch(region.textContent,/delivery succeeded|delivery complete/i);
      await delay(5);
    }finally{restore.reverse().forEach(reset=>reset());}
  }
});
test('timed_out_mounted_dispatch_retains_the_request_against_a_late_commit',async()=>{
  const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app'),storage=memory();
  const timers=new Map(),realSet=globalThis.setTimeout,realClear=globalThis.clearTimeout;
  const schedule=(fn,ms,...args)=>{if(ms!==10000)return realSet(fn,ms,...args);const handle={};timers.set(handle,()=>fn(...args));return handle;};
  const clear=handle=>{if(!timers.delete(handle))realClear(handle);};
  const restore=[globalValue('document',doc),globalValue('innerWidth',390),globalValue('addEventListener',()=>{}),globalValue('matchMedia',()=>({matches:false,addEventListener:()=>{}})),globalValue('setTimeout',schedule),globalValue('clearTimeout',clear)];
  let entered,release,finished,lastView;
  const started=new Promise(resolve=>entered=resolve),gate=new Promise(resolve=>release=resolve),done=new Promise(resolve=>finished=resolve);
  const fetcher=async(path,options)=>{
    if(path==='/preview'){
      const input=JSON.parse(new TextDecoder().decode(options.body.slice(73)));
      return {ok:true,json:async()=>({reply:{...accepted,input},command:'00'})};
    }
    entered();await gate;return {ok:true,json:async()=>response(options.body,{delivery:'pending'})};
  };
  const transport=new DurableTransport(bundle,credential,session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const recover=transport.recover.bind(transport);
  transport.recover=async()=>{try{return await recover();}finally{finished();}};
  root.addEventListener('zenoui-view',event=>{lastView=event.detail;});
  try{
    const description=structuredClone(bundle.description),source='timeout-fixture',contract=await sha256({description,source});
    await mount(root,{description,source,contract},{session:{...session,revision:'0',values:['0'],context:[],pending:'0'},transport});
    const input=root.querySelector('#command-120-field-2');input.value='2';input.dispatchEvent(new Event('input'));
    await settle(()=>root.dataset.ready==='true'&&!root.querySelector('button[data-command="120"]').disabled);
    root.querySelector('form[data-command="120"]').dispatchEvent(new Event('submit',{cancelable:true}));
    root.querySelector('[data-region="confirmation"] button').click();await started;
    const saved=storage.getItem(pendingKey);assert.ok(saved);
    assert.equal(timers.size,1,'only the dispatch deadline remains');
    const [handle,expire]=timers.entries().next().value;timers.delete(handle);expire();
    assert.equal(root.dataset.phase,'error');assert.equal(root.dataset.ready,'false');
    assert.equal(lastView.delivery,'unknown');assert.equal(transport.retired,true);
    release();await done;await Promise.resolve();
    assert.equal(storage.getItem(pendingKey),saved,'late response cannot consume the original request');
    assert.equal(root.dataset.revision,'0');assert.equal(lastView.delivery,'unknown');
  }finally{release();transport.terminate();restore.reverse().forEach(reset=>reset());}
});
