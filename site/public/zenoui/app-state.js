// Pure, owned presentation state. The Authority alone supplies decisions.
import {freeze,canonical,inDomain,layoutCheck,display,possibleAt,formFields,checkPresentation,amountText,minOutField,isInvalidAmount} from './app-contract.js';
const clone=v=>structuredClone(v);
const next=v=>(BigInt(v)+1n).toString();
const validPrincipal=s=>typeof s.principal==='string'&&s.principal.trim().length>0&&s.principal.length<=100;
// Server observations do not prove that a downstream effect happened.
const unsigned=v=>typeof v==='string' && v.length<=20 && /^(0|[1-9][0-9]*)$/.test(v) && BigInt(v)<=18446744073709551615n;
export const deliveryStatus=value=>value==='pending'?'pending':'unavailable';
export function pendingSummary(pending,revision) {
  if(!unsigned(pending)||!unsigned(revision))return 'The pending-delivery total at sign-in is unavailable.';
  return `At sign-in (revision ${revision}), ${pending} ${pending==='1'?'delivery was':'deliveries were'} pending in total.`;
}
export function initial(description, contract, epoch='0') {
  if(typeof contract!=='string'||!/^([0-9a-f]{64})$/.test(contract)||typeof epoch!=='string'||!/^(0|[1-9][0-9]*)$/.test(epoch))throw Error('UiInvalidIdentity');
  checkPresentation(description); const app=freeze(clone(description)); layoutCheck(app.layout);
  return freeze({app,contract,epoch,run:'0',revision:'0',principal:'Local operator',values:app.fields.filter(f=>f.source==='state').map(f=>f.initial),
    // A token amount the form shows starts empty, so nothing is proposed in its smallest unit; a
    // hidden input keeps its fixed initial value.
    drafts:Object.fromEntries(app.commands.map(c=>[c.id,app.fields.map(f=>f.index===app.action?c.id:f.decimals&&f.source==='command'&&app.relevant?.[c.id]?.includes(f.index)?'':f.initial)])),previews:{},quotes:{},selectedCommand:app.commands[0].id,consent:null,review:null,reviewProjections:null,active:null,
    preferences:{width:1440,theme:'light',motion:'reduce'},phase:'idle',message:'Waiting for the core.',evidence:{checked:'0',total:'1'},ledger:[],delivery:null,pendingAtSignIn:null,contextLocked:false});
}
// A party's view hides state values: the server sends null and supplies them itself.
const usable=(app,i,v)=>inDomain(app.fields[i],v)||(v===null&&app.fields[i]?.source==='state');
export function inputFor(s,id) {
  let i=0; return s.app.fields.map(f=>f.source==='state'?s.values[i++]:s.drafts[id][f.index]);
}
function binding(s,id) {
  const input=inputFor(s,id);
  return {epoch:s.epoch,run:s.run,request:id,command:id,contract:s.contract,revision:s.revision,principal:s.principal,input,
    shown:s.app.fields.map((f,i)=>({label:f.label,type:f.type_name,unit:f.unit,value:input[i]}))};
}
function quoteBinding(s,id) {
  const result=binding(s,id);
  result.request=`quote:${id}`;
  for(const [path,value] of Object.entries(s.app.presentation.requirements.quoteOverrides)) {
    const index=s.app.fields.find(f=>f.path===path).index;
    result.input[index]=value; result.shown[index].value=value;
  }
  return result;
}
function hasQuote(s) { return s.app.presentation?.requirements.projections.some(p=>p.kind==='accepted-decrease'); }
// A command gets a quote preview only when a quote projection is declared for it.
function quoted(s,id) { return !!s.app.presentation?.requirements.projections.some(p=>p.kind==='accepted-decrease'&&(!p.fieldByCommand||Object.hasOwn(p.fieldByCommand,id))); }
function same(a,b) { return canonical(a)===canonical(b); }
// The server sets a clock field, so replies are compared without it.
const unclocked=(app,input)=>Array.isArray(input)?input.map((v,i)=>app.fields[i]?.clock?null:v):input;
export function validReply(app, input, reply) {
  if(!reply||!same(unclocked(app,reply.input),unclocked(app,input))||reply.checked!=='1'||reply.total!=='1'||!['Accept','Reject','CommittedFailure','Refused'].includes(reply.class)) return false;
  if(!Array.isArray(reply.post))return false;
  if(reply.class==='Accept' && reply.reason!==null)return false;
  if(['Reject','CommittedFailure'].includes(reply.class) && !app.reasons.some(r=>r.id===reply.reason))return false;
  if(reply.class==='Refused' && reply.reason!==null)return false;
  const fields=app.fields.filter(f=>f.source==='state');
  if(['Reject','Refused'].includes(reply.class))return reply.post.length===0;
  const hidden=app.fields.flatMap((f,i)=>f.source==='state'?[input[i]===null]:[]);
  return reply.post.length===fields.length && fields.every((f,i)=>hidden[i]?reply.post[i]===null:inDomain(f,reply.post[i]));
}
// A minimum to receive of 0 accepts any amount. Unless the core's quote is itself 0, such a request is
// never offered for review: an empty or rounded-down minimum must not become no protection.
export function unprotected(s,id) {
  const f=minOutField(s.app,id),v=f?inputFor(s,id)[f.index]:null;
  if(!f||!/^-?(0|[1-9][0-9]*)$/.test(v??'')||BigInt(v)!==0n)return false;
  const q=project(s,id).find(p=>p.kind==='accepted-decrease')?.value??null;
  return q===null||BigInt(q)>0n;
}
export function enabled(s,id) {
  const p=s.previews[id];
  const accepted=p?.reply.class==='Accept';
  const ready=validPrincipal(s) && !s.active && !!p && same(p.binding,binding(s,id)) && inputFor(s,id).every((v,i)=>usable(s.app,i,v)) && !unprotected(s,id);
  return s.app.enablement[Number(accepted)*2+Number(ready)]===true;
}
// Availability may use a checked table if one is bound in a future profile. This
// integration has none, so every command remains available for preview.
export function available(s,id) {
  const code=possibleAt(s.app,id,inputFor(s,id));
  if(code===null||code==='+')return {available:true,reason:null};
  // No input passes here, so the reason uses its "unavailable" text when ui.json gives one.
  const r=code==='-'?null:s.app.reasons[code.charCodeAt(0)-97];
  return {available:false,reason:r?r.unavailable??r.message:'This is not possible in the current state.'};
}
function message(s,reply) {
  return reply.class==='Accept'?'The core accepts this request.':reply.class==='Refused'?'The core could not accept this request. Check the inputs and try again.':s.app.reasons.find(r=>r.id===reply.reason).message;
}
function invalidInput(s,id) {
  const input=inputFor(s,id),f=s.app.fields.find((f,i)=>f.source!=='state'&&!inDomain(f,input[i]));
  if(!f)return null;
  if(input[f.index]===''||(isInvalidAmount(input[f.index])&&input[f.index].invalidAmount===''))return f.options.length?`Choose a value for ${f.label}.`:f.unit==='unitless'&&!f.decimals?`${f.label}: enter a number.`:`${f.label}: enter an amount.`;
  return f.options.length?`Choose a value for ${f.label}.`:f.decimals?`${f.label} must be an amount from ${amountText(f,f.domain.min)} to ${amountText(f,f.domain.max)}, with at most ${f.decimals} decimal places.`:`${f.label} must be a whole number from ${f.domain.min} to ${f.domain.max}.`;
}
// What the core's preview says the reviewed command changes, in plain before-and-after values.
function changes(s,id) {
  const reply=s.previews[id]?.reply;
  if(reply?.class!=='Accept')return [];
  return s.app.fields.filter(f=>f.source==='state').flatMap((f,i)=>reply.post[i]===s.values[i]||reply.post[i]===null||s.values[i]===null?[]:[{label:f.label,before:display(f,s.values[i]),after:display(f,reply.post[i])}]);
}
function previews(s) {
  if(s.active||!validPrincipal(s))return [];
  // A quote needs only its own input to be valid: its overrides stand in for the entered limits.
  // It is asked first, so the quote is shown before the review it informs is enabled.
  const valid=input=>input.every((v,i)=>usable(s.app,i,v));
  return s.app.commands.flatMap(c=>[...(quoted(s,c.id)&&valid(quoteBinding(s,c.id).input)?[{kind:'preview',binding:quoteBinding(s,c.id)}]:[]),...(valid(inputFor(s,c.id))?[{kind:'preview',binding:binding(s,c.id)}]:[])]);
}
export function step(before,event) {
  // The bound description is immutable and shared; only the presentation state is copied.
  const {app,...rest}=before,s={...clone(rest),app}; let intents=[];
  switch(event.type) {
    case 'Refresh': s.previews={};s.quotes={};s.consent=null;s.review=null;s.reviewProjections=null;s.run=next(s.run);intents=previews(s);break;
    case 'Select':
      if(s.active || !s.app.commands.some(c=>c.id===event.command))return {state:before,intents:[]};
      s.selectedCommand=event.command;s.previews={};s.quotes={};s.consent=null;s.review=null;s.reviewProjections=null;s.run=next(s.run);intents=previews(s);break;
    case 'Edit': {
      const f=s.app.fields[event.index];
      if(!f||f.source==='state'||(s.contextLocked&&f.source==='context')||s.app.presentation?.requirements.fixedFields.includes(f.path)||event.index===s.app.action||!Object.hasOwn(s.drafts,event.command)||(typeof event.value!=='string'&&!(f.decimals&&isInvalidAmount(event.value))))return {state:before,intents:[]};
      s.drafts[event.command][event.index]=clone(event.value);s.previews={};s.quotes={};s.consent=null;s.review=null;s.reviewProjections=null;s.run=next(s.run);intents=previews(s);break;
    }
    // One "acting as" choice sets a context input for every command, like an edit of each.
    case 'Context': {
      const f=s.app.fields[event.index];
      if(!f||f.source!=='context'||s.contextLocked||typeof event.value!=='string')return {state:before,intents:[]};
      for(const c of s.app.commands)s.drafts[c.id][event.index]=event.value;
      s.previews={};s.quotes={};s.consent=null;s.review=null;s.reviewProjections=null;s.run=next(s.run);intents=previews(s);break;
    }
    case 'Principal':
      if(typeof event.value!=='string'||s.active||s.contextLocked)return {state:before,intents:[]};
      {const principal=event.value.length<=100?event.value:'';if(principal!==s.principal){s.delivery=null;s.pendingAtSignIn=null;}s.principal=principal;}
      s.message=validPrincipal(s)?'Choose a command to review.':'Enter a principal name of at most 100 characters.';s.previews={};s.quotes={};s.consent=null;s.review=null;s.reviewProjections=null;s.run=next(s.run);intents=previews(s);break;
    case 'Snapshot':
      if(typeof event.revision!=='string'||!/^(0|[1-9][0-9]*)$/.test(event.revision)||!Array.isArray(event.values)||event.values.length!==s.values.length||!s.app.fields.filter(f=>f.source==='state').every((f,i)=>event.values[i]===null||inDomain(f,event.values[i])))return {state:before,intents:[]};
      if(Object.hasOwn(event,'observation')){
        const observation=event.observation;
        if(s.active||s.pendingAtSignIn!==null||!observation||observation.contract!==s.contract||observation.principal!==s.principal||!unsigned(event.revision)||BigInt(event.revision)<BigInt(s.revision))return {state:before,intents:[]};
        s.pendingAtSignIn={revision:event.revision,count:unsigned(observation.pending)?observation.pending:null};s.contextLocked=true;
      }
      s.values=clone(event.values);s.revision=event.revision;s.active=null;s.consent=null;s.review=null;s.reviewProjections=null;s.previews={};s.quotes={};s.run=next(s.run);intents=previews(s);break;
    case 'Preview': {
      const id=event.binding?.command;
      const quote=hasQuote(s) && event.binding?.request===`quote:${id}`;
      if(!s.app.commands.some(c=>c.id===id)||!same(event.binding,quote?quoteBinding(s,id):binding(s,id))||s.active)return {state:before,intents:[]};
      if(!validReply(s.app,event.binding.input,event.reply))throw Error('UiInvalidCoreReply');
      if(quote && s.quotes[id])return {state:before,intents:[]};
      (quote?s.quotes:s.previews)[id]={binding:clone(event.binding),reply:clone(event.reply)};if(s.phase==='idle')s.message='Choose a command to review.';break;
    }
    case 'Review':
      if(!enabled(s,event.command))return {state:before,intents:[]};
      s.review=binding(s,event.command);s.reviewProjections=project(s,event.command);s.consent=null;break;
    case 'Confirm':
      if(!s.review||!enabled(s,s.review.command)||!same(s.review,binding(s,s.review.command)))return {state:before,intents:[]};
      s.consent=clone(s.review);break;
    case 'Dispatch': {
      const id=event.command,c=s.app.commands.find(c=>c.id===id);
      if(!c||!enabled(s,id)||(c.consequential && (!s.consent||!same(s.consent,binding(s,id)))))return {state:before,intents:[]};
      s.active=binding(s,id);s.consent=null;s.review=null;s.reviewProjections=null;s.phase='running';s.message='The core is checking the confirmed request.';s.evidence={checked:'0',total:'1'};s.delivery='unknown';
      intents=[{kind:'dispatch',binding:clone(s.active)}];break;
    }
    case 'Result':
      if(!s.active||!same(event.binding,s.active)||event.binding.revision!==s.revision)return {state:before,intents:[]};
      if(!validReply(s.app,s.active.input,event.reply))throw Error('UiInvalidCoreReply');
      if(['Accept','CommittedFailure'].includes(event.reply.class)){s.values=clone(event.reply.post);s.revision=next(s.revision);s.delivery=deliveryStatus(event.delivery);}
      else s.delivery=null;
      s.message=message(s,event.reply);s.phase=event.reply.class==='Accept'?'done':'refused';s.evidence={checked:'1',total:'1'};
      s.ledger.push({command:s.app.commands.find(c=>c.id===s.active.command).label,result:event.reply.class});
      s.active=null;s.previews={};s.quotes={};s.run=next(s.run);intents=previews(s);break;
    case 'Fail':
      if(s.active && same(s.active,event.binding)) {s.active=null;s.phase='error';s.evidence={checked:'0',total:'1'};s.message='The request stopped. No result is available.';s.consent=null;s.review=null;s.reviewProjections=null;s.previews={};s.quotes={};s.run=next(s.run);}
      else if(!s.active&&hasQuote(s)&&s.app.commands.some(c=>c.id===event.binding?.command)&&same(event.binding,quoteBinding(s,event.binding.command))){delete s.quotes[event.binding.command];}
      else if(!s.active&&event.binding&&s.app.commands.some(c=>c.id===event.binding.command)&&same(event.binding,binding(s,event.binding.command))){delete s.previews[event.binding.command];s.message='The core is unavailable. Try again.';}
      else return {state:before,intents:[]};
      break;
    case 'Preferences': s.preferences=clone(event.value);break;
    default:return {state:before,intents:[]};
  }
  return {state:freeze(s),intents:freeze(intents)};
}
function explain(s,c,a) {
  if(!a.available)return {tone:'unavailable',text:a.reason};
  const p=s.previews[c.id];
  if(p&&p.reply.class==='Accept'&&unprotected(s,c.id))return {tone:'unprotected',text:'The minimum to receive is 0, so this request would accept any amount.'};
  if(p)return p.reply.class==='Accept'?{tone:'ready',text:'Ready to review.'}:{tone:'refused',text:message(s,p.reply)};
  const invalid=invalidInput(s,c.id),empty=inputFor(s,c.id).some((v,i)=>v===''&&s.app.fields[i].source==='command');
  return invalid?{tone:empty?'empty':'invalid',text:invalid}:{tone:'pending',text:'Checking with the core.'};
}
export function view(s) {
  const order=[...s.app.layout].sort((a,b)=>a.slot-b.slot);
  // No semantic function reads preferences; layout adapts in the style sheet only.
  const actions=s.app.commands.map(c=>{const a=available(s,c.id),e=explain(s,c,a);
    const ready=enabled(s,c.id);
    return {id:c.id,label:c.label,enabled:ready,available:a.available,reason:a.reason,explanation:e.text,tone:e.tone,fields:formFields(s.app,c.id).map(f=>f.index),
      // This is the current bound preview, not a proof that an input is irrelevant.
      changes:ready?changes(s,c.id):[]};});
  // One primary action: the confirmation while one is open, otherwise the first possible command.
  const primary=s.review?null:(actions.find(a=>a.id===s.selectedCommand&&a.enabled)??actions.find(a=>a.enabled)??actions.find(a=>a.available))?.id??null;
  const reviewed=s.review&&s.app.commands.find(c=>c.id===s.review.command);
  const context=s.app.fields.filter(f=>f.source==='context'&&!(s.contextLocked&&f.clock)).map(f=>({index:f.index,label:f.label,value:display(f,s.drafts[s.app.commands[0].id][f.index])}));
  return freeze({layout:order,selectedCommand:s.selectedCommand,projections:project(s,s.selectedCommand),projectionsByCommand:Object.fromEntries(s.app.commands.map(c=>[c.id,project(s,c.id)])),quoteInputs:quoteInputs(s),phase:s.phase,message:s.message,evidence:clone(s.evidence),archetype:s.app.archetype,
    // A party's view hides some state values (null): they are absent, never shown as placeholders.
    state:s.app.fields.filter(f=>f.source==='state').flatMap((f,i)=>s.values[i]===null?[]:[{label:f.label,type:f.type_name,unit:f.unit,value:display(f,s.values[i]),index:f.index,role:f.role,raw:s.values[i]}]),
    actions:actions.map(a=>({...a,primary:a.id===primary})),context,
    drafts:Object.fromEntries(Object.entries(s.drafts).map(([id,values])=>[id,values.map((value,i)=>s.contextLocked&&s.app.fields[i].clock?null:clone(value))])),principal:s.principal,
    confirmation:s.review?{command:reviewed.id,label:reviewed.label,principal:s.review.principal,revision:s.review.revision,contract:s.review.contract,changes:changes(s,reviewed.id),projections:clone(s.reviewProjections??[]),quoteInputs:quoteInputs(s),
      fields:s.review.shown.flatMap((f,i)=>f.value===null||(s.contextLocked&&s.app.fields[i].clock)?[]:[{path:s.app.fields[i].path,index:i,label:f.label,type:f.type,value:display(s.app.fields[i],f.value),raw:f.value,part:i===s.app.action?'action':s.app.fields[i].source,
        used:s.app.fields[i].source==='context'||s.app.relevant[reviewed.id].includes(i)}])}:null,
    ledger:clone(s.ledger),delivery:s.delivery,pendingAtSignIn:clone(s.pendingAtSignIn)});
}

// Projections never decide a core result. The minimum-output guard may use a
// quote projection to withhold review of an unprotected request.
export function project(s,id) {
  if(!s.app.presentation)return [];
  const fields=s.app.fields.filter(f=>f.source==='state');
  return s.app.presentation.requirements.projections.filter(p=>Object.hasOwn(p.fieldByCommand,id)).map(p=>{
    const index=fields.findIndex(f=>f.path===p.fieldByCommand[id]);
    let value=null;
    if(s.values[index]===null)return {id:p.id,label:p.label,unit:fields[index].unit,kind:p.kind,value:null};
    if(p.kind==='state-complement')value=(BigInt(p.total)-BigInt(s.values[index])).toString();
    else {
      const q=s.quotes[id];
      if(q && !s.active && q.reply.class==='Accept' && q.reply.post[index]!==null && same(q.binding,quoteBinding(s,id))) {
        const difference=BigInt(s.values[index])-BigInt(q.reply.post[index]);
        if(difference>=0n)value=difference.toString();
      }
    }
    return {id:p.id,label:p.label,unit:fields[index].unit,kind:p.kind,value};
  });
}

function quoteInputs(s) {
  return Object.entries(s.app.presentation?.requirements.quoteOverrides??{}).map(([path,value])=>{
    const f=s.app.fields.find(f=>f.path===path);return {path,label:f.label,value,unit:f.unit};
  });
}
