// Pure, owned presentation state. The Authority alone supplies decisions.
import {freeze,canonical,inDomain,layoutCheck,display} from './app-contract.js';
const clone=v=>structuredClone(v);
const next=v=>(BigInt(v)+1n).toString();
const validPrincipal=s=>typeof s.principal==='string'&&s.principal.trim().length>0&&s.principal.length<=100;
export function initial(description, contract, epoch='0') {
  if(typeof contract!=='string'||!/^([0-9a-f]{64})$/.test(contract)||typeof epoch!=='string'||!/^(0|[1-9][0-9]*)$/.test(epoch))throw Error('UiInvalidIdentity');
  const app=freeze(clone(description)); layoutCheck(app.layout);
  return freeze({app,contract,epoch,run:'0',revision:'0',principal:'Local operator',values:app.fields.filter(f=>f.source==='state').map(f=>f.initial),
    drafts:Object.fromEntries(app.commands.map(c=>[c.id,app.fields.map(f=>f.index===app.action?c.id:f.initial)])),previews:{},consent:null,review:null,active:null,
    preferences:{width:1440,theme:'light',motion:'reduce'},phase:'idle',message:'Waiting for the core.',evidence:{checked:'0',total:'1'},ledger:[]});
}
export function inputFor(s,id) {
  let i=0; return s.app.fields.map(f=>f.source==='state'?s.values[i++]:s.drafts[id][f.index]);
}
function binding(s,id) {
  const input=inputFor(s,id);
  return {epoch:s.epoch,run:s.run,request:id,command:id,contract:s.contract,revision:s.revision,principal:s.principal,input,
    shown:s.app.fields.map((f,i)=>({label:f.label,type:f.type_name,unit:f.unit,value:input[i]}))};
}
function same(a,b) { return canonical(a)===canonical(b); }
export function validReply(app, input, reply) {
  if(!reply||!same(reply.input,input)||reply.checked!=='1'||reply.total!=='1'||!['Accept','Reject','CommittedFailure','Refused'].includes(reply.class)) return false;
  if(!Array.isArray(reply.post))return false;
  if(reply.class==='Accept' && reply.reason!==null)return false;
  if(['Reject','CommittedFailure'].includes(reply.class) && !app.reasons.some(r=>r.id===reply.reason))return false;
  if(reply.class==='Refused' && reply.reason!==null)return false;
  const fields=app.fields.filter(f=>f.source==='state');
  if(['Reject','Refused'].includes(reply.class))return reply.post.length===0;
  return reply.post.length===fields.length && fields.every((f,i)=>inDomain(f,reply.post[i]));
}
export function enabled(s,id) {
  const p=s.previews[id];
  const accepted=p?.reply.class==='Accept';
  const ready=validPrincipal(s) && !s.active && !!p && same(p.binding,binding(s,id)) && inputFor(s,id).every((v,i)=>inDomain(s.app.fields[i],v));
  return s.app.enablement[Number(accepted)*2+Number(ready)]===true;
}
function message(s,reply) {
  return reply.class==='Accept'?'The core accepts this request.':reply.class==='Refused'?'The core could not accept this request. Check the inputs and try again.':s.app.reasons.find(r=>r.id===reply.reason).message;
}
function previews(s) {
  if(s.active||!validPrincipal(s))return [];
  return s.app.commands.filter(c=>inputFor(s,c.id).every((v,i)=>inDomain(s.app.fields[i],v))).map(c=>({kind:'preview',binding:binding(s,c.id)}));
}
export function step(before,event) {
  const s=clone(before); let intents=[];
  switch(event.type) {
    case 'Refresh': s.previews={};s.consent=null;s.review=null;s.run=next(s.run);intents=previews(s);break;
    case 'Edit': {
      const f=s.app.fields[event.index];
      if(!f||f.source==='state'||event.index===s.app.action||!Object.hasOwn(s.drafts,event.command)||typeof event.value!=='string')return {state:before,intents:[]};
      s.drafts[event.command][event.index]=event.value;s.previews={};s.consent=null;s.review=null;s.run=next(s.run);intents=previews(s);break;
    }
    case 'Principal':
      if(typeof event.value!=='string')return {state:before,intents:[]};
      s.principal=event.value.length<=100?event.value:'';s.message=validPrincipal(s)?'Choose a command to review.':'Enter a principal name of at most 100 characters.';s.previews={};s.consent=null;s.review=null;s.run=next(s.run);intents=previews(s);break;
    case 'Snapshot':
      if(typeof event.revision!=='string'||!/^(0|[1-9][0-9]*)$/.test(event.revision)||!Array.isArray(event.values)||event.values.length!==s.values.length||!s.app.fields.filter(f=>f.source==='state').every((f,i)=>inDomain(f,event.values[i])))return {state:before,intents:[]};
      s.values=clone(event.values);s.revision=event.revision;s.active=null;s.consent=null;s.review=null;s.previews={};s.run=next(s.run);intents=previews(s);break;
    case 'Preview': {
      const id=event.binding?.command;
      if(!s.app.commands.some(c=>c.id===id)||!same(event.binding,binding(s,id))||s.active)return {state:before,intents:[]};
      if(!validReply(s.app,event.binding.input,event.reply))throw Error('UiInvalidCoreReply');
      s.previews[id]={binding:clone(event.binding),reply:clone(event.reply)};if(s.phase==='idle')s.message='Choose a command to review.';break;
    }
    case 'Review':
      if(!enabled(s,event.command))return {state:before,intents:[]};
      s.review=binding(s,event.command);s.consent=null;break;
    case 'Confirm':
      if(!s.review||!enabled(s,s.review.command)||!same(s.review,binding(s,s.review.command)))return {state:before,intents:[]};
      s.consent=clone(s.review);break;
    case 'Dispatch': {
      const id=event.command,c=s.app.commands.find(c=>c.id===id);
      if(!c||!enabled(s,id)||(c.consequential && (!s.consent||!same(s.consent,binding(s,id)))))return {state:before,intents:[]};
      s.active=binding(s,id);s.consent=null;s.review=null;s.phase='running';s.message='The core is checking the confirmed request.';s.evidence={checked:'0',total:'1'};
      intents=[{kind:'dispatch',binding:clone(s.active)}];break;
    }
    case 'Result':
      if(!s.active||!same(event.binding,s.active)||event.binding.revision!==s.revision)return {state:before,intents:[]};
      if(!validReply(s.app,s.active.input,event.reply))throw Error('UiInvalidCoreReply');
      if(['Accept','CommittedFailure'].includes(event.reply.class)){s.values=clone(event.reply.post);s.revision=next(s.revision);}
      s.message=message(s,event.reply);s.phase=event.reply.class==='Accept'?'done':'refused';s.evidence={checked:'1',total:'1'};
      s.ledger.push({command:s.app.commands.find(c=>c.id===s.active.command).label,result:event.reply.class});
      s.active=null;s.previews={};s.run=next(s.run);intents=previews(s);break;
    case 'Fail':
      if(s.active && same(s.active,event.binding)) {s.active=null;s.phase='error';s.evidence={checked:'0',total:'1'};s.message='The request stopped. No result is available.';s.consent=null;s.review=null;s.previews={};s.run=next(s.run);}
      else if(!s.active&&event.binding&&s.app.commands.some(c=>c.id===event.binding.command)&&same(event.binding,binding(s,event.binding.command))){delete s.previews[event.binding.command];s.message='The core is unavailable. Try again.';}
      else return {state:before,intents:[]};
      break;
    case 'Preferences': s.preferences=clone(event.value);break;
    default:return {state:before,intents:[]};
  }
  return {state:freeze(s),intents:freeze(intents)};
}
export function view(s) {
  const order=[...s.app.layout].sort((a,b)=>a.slot-b.slot);
  // Adaptation changes order only. No semantic function reads preferences.
  if(s.preferences.width<500) [order[0],order[1]]=[order[1],order[0]];
  return freeze({layout:order,phase:s.phase,message:s.message,evidence:clone(s.evidence),
    state:s.app.fields.filter(f=>f.source==='state').map((f,i)=>({label:f.label,type:f.type_name,unit:f.unit,value:display(f,s.values[i])})),
    actions:s.app.commands.map(c=>({id:c.id,label:c.label,enabled:enabled(s,c.id),explanation:s.previews[c.id]?message(s,s.previews[c.id].reply):inputFor(s,c.id).every((v,i)=>inDomain(s.app.fields[i],v))?'Waiting for the core.':'Enter values within the declared bounds.'})),
    confirmation:s.review?{label:s.app.commands.find(c=>c.id===s.review.command).label,principal:s.review.principal,revision:s.review.revision,contract:s.review.contract,fields:s.review.shown.map((f,i)=>({label:f.label,type:f.type,value:display(s.app.fields[i],f.value)}))}:null,
    ledger:clone(s.ledger)});
}
