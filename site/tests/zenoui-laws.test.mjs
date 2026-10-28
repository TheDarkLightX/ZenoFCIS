import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import * as core from '../public/zenoui/app-state.js';
import {describe,canonical,inDomain,layoutCheck,REGIONS,display} from '../public/zenoui/app-contract.js';
const root=process.argv[2];if(!root)throw Error('evidence directory required');
const clone=structuredClone,report={format:'zenoui/laws/1',apps:{}};
const stateSource=await readFile(new URL('../public/zenoui/app-state.js',import.meta.url),'utf8');
const descriptorSource=await readFile(new URL('../public/zenoui/app-contract.js',import.meta.url),'utf8');
async function mutant(source,from,to,kind='state'){
  assert.ok(source.includes(from),'mutation site exists');
  const directory=new URL('../public/zenoui/',import.meta.url);
  const resolved=source.replace(from,to).replace(/(from\s+)(['"])(\.\/[^'"]+)\2/g,(_,prefix,quote,specifier)=>prefix+JSON.stringify(new URL(specifier,directory).href));
  return import('data:text/javascript;base64,'+Buffer.from(resolved).toString('base64'));
}
function killed(check,module,name){assert.throws(()=>check(module),assert.AssertionError,`${name}: planted defect must fail its law`);}
function tupleAt(app,ordinal){
  let n=BigInt(ordinal);const tuple=[];
  for(let i=app.fields.length-1;i>=0;i--){const f=app.fields[i],size=f.domain.kind==='int'?BigInt(f.domain.max)-BigInt(f.domain.min)+1n:BigInt(f.options.length);const at=n%size;n/=size;tuple[i]=f.domain.kind==='int'?(BigInt(f.domain.min)+at).toString():f.options[Number(at)].value;}
  return tuple;
}
function fromInput(module,bundle,input,reply){
  let s=module.initial(bundle.description,bundle.contract);const id=input[s.app.action];
  s={...s,values:s.app.fields.flatMap((f,i)=>f.source==='state'?[input[i]]:[]),drafts:{...s.drafts,[id]:input}};
  const request=module.step(s,{type:'Refresh'});s=request.state;
  const intent=request.intents.find(r=>r.binding.command===id);
  assert.ok(intent,'positive witness has a request');
  s=module.step(s,{type:'Preview',binding:intent.binding,reply}).state;
  return {s,id,binding:intent.binding};
}
for(const name of (process.argv.length>3?process.argv.slice(3):['withdrawal-queue','inventory-reservation','wide-counter'])){
  const dir=path.join(root,name),bundle=JSON.parse(await readFile(path.join(dir,'app/app.json'))),raw=JSON.parse(await readFile(path.join(dir,'raw.json'))),meta=JSON.parse(await readFile(path.join(dir,'metadata.json'))),finite=JSON.parse(await readFile(path.join(dir,'finite.json'))),samples=JSON.parse(await readFile(path.join(dir,'samples.json')));
  const app=bundle.description,positive=samples.find(x=>x.reply.class==='Accept');assert.ok(positive);
  const base=fromInput(core,bundle,positive.input,positive.reply),results={};
  function law1(m){const s=m.initial(app,bundle.contract),v=m.view(s);assert.deepEqual([...v.layout.map(r=>r.id)].sort(),[...REGIONS].sort(),'L1_complete_start');assert.equal(v.state.length,app.fields.filter(f=>f.source==='state').length);assert.equal(v.evidence.checked,'0');assert.ok(v.actions.every(a=>!a.enabled));assert.equal(v.confirmation,null);}
  law1(core);killed(law1,await mutant(stateSource,'const order=[...s.app.layout]','const order=[...s.app.layout].slice(1)'),'L1');results.L1='complete initial view; missing region killed';
  function law2(m){const {s,binding}=fromInput(m,bundle,positive.input,positive.reply);for(const key of ['epoch','run','request','revision','principal','contract']){const b={...binding,[key]:'stale'};assert.equal(m.step(s,{type:'Preview',binding:b,reply:positive.reply}).state,s,`L2_run_identity ${key}`);}const restarted=m.initial(app,bundle.contract,'9');assert.equal(m.step(restarted,{type:'Preview',binding,reply:positive.reply}).state,restarted,'L2_restart');}
  law2(core);killed(law2,await mutant(stateSource,'return canonical(a)===canonical(b);','return true;'),'L2');results.L2='epoch/run/request/subject stale replies no-op; identity bypass killed';
  function trace(m,preferences){let {s,id}=fromInput(m,bundle,positive.input,positive.reply);s=m.step(s,{type:'Preferences',value:preferences}).state;const out=[];for(const event of [{type:'Review',command:id},{type:'Confirm'},{type:'Dispatch',command:id}]){const r=m.step(s,event);s=r.state;const v=m.view(s);delete clone(v).layout;out.push({view:Object.fromEntries(Object.entries(v).filter(([k])=>k!=='layout')),intents:r.intents});}const r=m.step(s,{type:'Result',binding:s.active,reply:positive.reply});out.push({values:r.state.values,revision:r.state.revision,intents:r.intents});return out;}
  function law3(m){const expected=trace(m,{width:1440,theme:'light',motion:'normal'});for(const width of [320,390,1440])for(const theme of ['light','dark'])for(const motion of ['normal','reduce'])assert.deepEqual(trace(m,{width,theme,motion}),expected,'L3_two_run_noninterference');}
  law3(core);killed(law3,await mutant(stateSource,"return s.app.enablement[Number(accepted)*2+Number(ready)]===true;","return s.preferences.width>500 && s.app.enablement[Number(accepted)*2+Number(ready)]===true;"),'L3');results.L3='12 preference traces preserve commands/consent/decisions; width gate killed';
  layoutCheck(app.layout);const badLayout=clone(app.layout);badLayout[1].slot=badLayout[0].slot;assert.throws(()=>layoutCheck(badLayout),/L4_layout_declared_once/);results.L4='all slots covered, bounded and invertible; overlap killed';
  assert.deepEqual(describe(raw.descriptor,meta),app);
  for(const f of app.fields){for(const key of ['label','unit']){const bad=clone(meta);delete bad.fields[f.path][key];assert.throws(()=>describe(raw.descriptor,bad),new RegExp(key==='label'?'UiMissingLabel':'UiMissingUnit'));}}
  for(const r of app.reasons){const bad=clone(meta);delete bad.reasons[r.id];assert.throws(()=>describe(raw.descriptor,bad),/UiMissingReasonMessage/);}
  for(const f of app.fields)assert.equal(inDomain(f,Number(f.initial)),false,'L5 rejects Number inputs');
  function law5(m){const f={options:[],unit:'credits'};assert.equal(m.display(f,'9007199254740993'),'9007199254740993 credits','L5_exact_wide');}
  law5({display});killed(law5,await mutant(descriptorSource,'export function display(field,value) {','export function display(field,value) { value=String(Number(value));','descriptor'),'L5');results.L5='all metadata required; Number conversion and missing metadata killed';
  function law6(m){const {s,id,binding}=fromInput(m,bundle,positive.input,positive.reply);assert.throws(()=>m.step(s,{type:'Preview',binding,reply:{...positive.reply,checked:'0'}}),/UiInvalidCoreReply/,'L6_partial');let r=m.step(s,{type:'Review',command:id});r=m.step(r.state,{type:'Confirm'});r=m.step(r.state,{type:'Dispatch',command:id});const stopped=m.step(r.state,{type:'Fail',binding:r.state.active}).state;assert.equal(stopped.evidence.checked,'0','L6_timeout');assert.equal(m.step(stopped,{type:'Result',binding:r.state.active,reply:positive.reply}).state,stopped);assert.equal(m.validReply(app,binding.input,{...positive.reply,class:'proved'}),false);}
  law6(core);killed(law6,await mutant(stateSource,"reply.checked!=='1'||reply.total!=='1'||",''),'L6');results.L6='typed complete counts, timeout and unsupported claims; partial-count bypass killed';
  const witnesses={},omissions=[];
  function law7(m,full=true){
    const template=m.initial(app,bundle.contract);const size=Number(finite.domain);assert.equal(finite.classes.length,size);
    for(let ordinal=0;ordinal<size;ordinal++){
      const input=tupleAt(app,ordinal),id=input[app.action],accepted=finite.classes[ordinal]==='A';
      let s={...template,values:app.fields.flatMap((f,i)=>f.source==='state'?[input[i]]:[]),drafts:{...template.drafts,[id]:input}};
      // The binding is produced by the same shipped reducer; the decision class is independently exported from the real Authority.
      // Constructing its data directly avoids cloning the complete view for every tuple.
      const binding={epoch:s.epoch,run:s.run,request:id,command:id,contract:s.contract,revision:s.revision,principal:s.principal,input,shown:app.fields.map((f,i)=>({label:f.label,type:f.type_name,unit:f.unit,value:input[i]}))};
      s={...s,previews:{[id]:{binding,reply:{class:accepted?'Accept':finite.classes[ordinal]==='R'?'Reject':'Refused'}}}};
      const offered=m.enabled(s,id);assert.ok(!offered||accepted,`L7_enabled_implies_accepted ordinal ${ordinal}`);
      if(full){if(accepted&&!offered)omissions.push(ordinal);if(offered)witnesses[id]??={ordinal:String(ordinal),input};
        if(offered){assert.equal(m.enabled({...s,active:binding},id),false,'L7_busy');assert.equal(m.enabled({...s,revision:'99'},id),false,'L7_stale_revision');}}
    }
  }
  law7(core);killed(m=>law7(m,false),await mutant(stateSource,"const accepted=p?.reply.class==='Accept';","const accepted=p?.reply.class!=='Refused';"),'L7');
  assert.deepEqual(Object.keys(witnesses).sort(),app.commands.map(c=>c.id).sort(),'L7_positive_witness_per_action');assert.deepEqual(omissions,[]);results.L7={tuples:finite.domain,omissions,witnesses,control:'false enable killed'};
  const consequential=samples.find(x=>x.reply.class==='Accept'&&app.commands.find(c=>c.id===x.input[app.action]).consequential);
  const consentSample=consequential??positive;
  const consentBundle=consequential?bundle:{...bundle,description:{...app,commands:app.commands.map(c=>({...c,consequential:true}))}};
  function readyConsent(m){let {s,id}=fromInput(m,consentBundle,consentSample.input,consentSample.reply);s=m.step(s,{type:'Review',command:id}).state;s=m.step(s,{type:'Confirm'}).state;return {s,id};}
  function law8(m){let {s,id}=readyConsent(m);const submitted=m.step(s,{type:'Dispatch',command:id});assert.equal(submitted.intents.length,1,'L8_positive_consent');assert.equal(submitted.intents[0].kind,'dispatch');assert.equal(submitted.state.consent,null);
    const noConsent={...s,consent:null};assert.equal(m.step(noConsent,{type:'Dispatch',command:id}).intents.length,0,'L8_requires_confirmation');
    for(const f of app.fields.filter(f=>f.source!=='state'&&f.index!==app.action)){const changed=m.step(s,{type:'Edit',command:id,index:f.index,value:s.drafts[id][f.index]}).state;assert.equal(changed.consent,null,'L8_every_edit_voids');assert.equal(m.step(changed,{type:'Dispatch',command:id}).intents.length,0);}
    for(const change of [{principal:'Another person'},{contract:'different'},{revision:'1'},{drafts:{...s.drafts,[id]:s.drafts[id].map((v,i)=>i===app.action?'999999999':v)}}])assert.equal(m.step({...s,...change},{type:'Dispatch',command:id}).intents.length,0,'L8_subject_binding');
    const changedApp=clone(s.app);changedApp.fields[0].unit='other units';assert.equal(m.step({...s,app:changedApp},{type:'Dispatch',command:id}).intents.length,0,'L8_unit_binding');
    for(const value of ['',' '.repeat(4),'x'.repeat(101)]){const edited=m.step(s,{type:'Principal',value}).state;assert.equal(edited.consent,null,'L8_invalid_principal_edit_voids');assert.equal(m.step(edited,{type:'Dispatch',command:id}).intents.length,0);}
    const next=m.step(s,{type:'Snapshot',revision:'1',values:s.values}).state;assert.equal(next.consent,null);
    const wrong={...consentSample.reply,input:consentSample.input.map((v,i)=>i===app.action?'999999999':v)};assert.equal(m.validReply(app,consentSample.input,wrong),false,'L8_dispatch_reply_revalidation');}
  law8(core);killed(law8,await mutant(stateSource,"(c.consequential && (!s.consent||!same(s.consent,binding(s,id))))","false"),'L8');results.L8=consequential?'exact command/fields/units/principal/contract/revision; absent-consent bypass killed':'no consequential commands; shared consent control killed on a consequential metadata fixture';
  // Aliases cannot mutate the bound policy, input state, accepted reply or returned view.
  assert.throws(()=>{base.s.app.fields[0].unit='changed';},TypeError);assert.throws(()=>{core.view(base.s).state[0].value='changed';},TypeError);
  const inputCopy=clone(positive.reply);const r=core.step(base.s,{type:'Preview',binding:base.binding,reply:inputCopy});inputCopy.post[0]='999999999';assert.notEqual(r.state.previews[base.id].reply.post[0],inputCopy.post[0]);
  report.apps[name]=results;console.log(`${name}: laws 1-8 pass; eight planted defects killed; ${finite.domain} real Authority tuples; no conservative omissions`);
}
await writeFile(path.join(root,'laws.json'),JSON.stringify(report,null,2)+'\n');
