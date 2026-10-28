// The imperative shell of a synthesized UI: identity checks, the worker or server transport, timers
// and events. The reducer (app-state.js) and the view (ui-view.js) are pure; the renderer maps.
import {initial,pendingSummary,step,view} from './app-state.js';
import {canonical,freeze,inDomain,layoutCheck,checkPresentation,parseAmount} from './app-contract.js';
import {presentationView} from './ui-view.js';
import {buildTree} from './ui-renderer.js';
import {scopeInputs} from './ui-grammar.js';
import {node} from './components.js';
export async function sha256(value) {
  const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(canonical(value)));
  return [...new Uint8Array(bytes)].map(b=>b.toString(16).padStart(2,'0')).join('');
}
const SLIPPAGE=[{label:'0.5',bps:50n},{label:'1',bps:100n},{label:'5',bps:500n}];
export async function mount(root,bundle,options={}) {
  const {description,source,contract}=bundle;
  if(contract!==await sha256({description,source}))throw Error('UiContractIdentity');
  freeze(bundle);layoutCheck(description.layout);checkPresentation(description);
  const app=description,server=!!options.session,req=app.presentation.requirements;
  let state=initial(description,options.session?.contract??contract),ready=false,retired=false;
  if(options.session){
    const session=options.session;
    const contextFields=description.fields.filter(f=>f.source==='context');
    if(typeof session.principal!=='string'||!session.principal.trim()||session.principal.length>100||!Array.isArray(session.context)||session.context.length!==contextFields.length||!contextFields.every((f,i)=>inDomain(f,session.context[i])))throw Error('UiInvalidSession');
    state=structuredClone(state);state.principal=session.principal;
    for(const command of description.commands){let i=0;for(const f of description.fields)if(f.source==='context')state.drafts[command.id][f.index]=session.context[i++];}
    state=step(state,{type:'Snapshot',values:session.values,revision:session.revision,observation:{contract:session.contract,principal:session.principal,pending:session.pending}}).state;
    if(!state.contextLocked)throw Error('UiInvalidSession');
  }
  // The swap helper's maximum slippage: a preference of this page, never part of a request. It fills
  // the minimum-out input, which is then shown and checked exactly as entered.
  let slippage='0.5';
  const timers=new Map(),swapScopes=[];
  (function scan(n){if(n.kind==='idiom'&&n.idiom==='swap-card')swapScopes.push(n.scope);for(const c of n.children??[])scan(c);})(app.presentation.ast);
  const commandOf=scope=>scope==='selected'||scope.startsWith('group-')?state.selectedCommand:scope;
  const minOut=scope=>(scopeInputs(app,req,scope)??[]).map(p=>app.fields.find(f=>f.path===p)).find(f=>f.role==='min-out');
  const handlers={
    refresh:()=>dispatch({type:'Refresh'}),
    review:scope=>dispatch({type:'Review',command:commandOf(scope)}),
    confirm:()=>{const id=state.review?.command;dispatch({type:'Confirm'});dispatch({type:'Dispatch',command:id});},
    // A decimal amount is entered in tokens and sent in its smallest units; text that is not an exact
    // amount is retained as invalid text, so it cannot be reinterpreted as base units.
    edit:(scope,index,value)=>{if(minOut(scope)?.index===index)slippage='custom';const f=app.fields[index];dispatch({type:'Edit',command:commandOf(scope),index,value:f.decimals?parseAmount(f,value)??{invalidAmount:value}:value});},
    select:command=>dispatch({type:'Select',command}),
    principal:value=>dispatch({type:'Principal',value}),
    context:(index,value)=>dispatch({type:'Context',index,value}),
    slippage:value=>{if(SLIPPAGE.some(o=>o.label===value)){slippage=value;dispatch({type:'Refresh'});}},
  };
  const render=()=>{
    const data=view(state),delivery=!server?'':data.delivery==='pending'?'The server reported delivery pending for the last committed request.':data.delivery==='unavailable'?'This response includes no delivery confirmation.':data.delivery==='unknown'?'The request outcome is unknown. If an original request was saved, sign in again and recover it.':'';
    const pending=!server?'':data.pendingAtSignIn?pendingSummary(data.pendingAtSignIn.count,data.pendingAtSignIn.revision):'The pending-delivery total at sign-in is unavailable.';
    return presentationView(app,data,{server,delivery,pending,revision:state.revision,slippage:{value:slippage,options:SLIPPAGE.map(o=>o.label)},record:{contract,revision:state.revision,epoch:state.epoch,run:state.run,phase:state.phase}});
  };
  const first=render(),tree=buildTree(app.presentation.ast,first,handlers);
  root.className='zu-app';
  root.replaceChildren(node('header','zu-header',node('div','zu-heading',node('h1','zu-title',first.title),node('p','zu-lede',first.lede))),tree.element);
  // Fill a swap's minimum from its quote and the chosen slippage; an input, so it is computed here.
  function fillMinimum(data){
    if(slippage==='custom'||state.active||state.review)return;
    for(const scope of swapScopes){
      const f=minOut(scope),id=commandOf(scope),q=data.projectionsByCommand[id]?.find(p=>p.kind==='accepted-decrease')?.value;
      if(!f||q==null)continue;
      const target=(BigInt(q)*(10000n-SLIPPAGE.find(o=>o.label===slippage).bps)/10000n).toString();
      if(state.drafts[id][f.index]!==target){dispatch({type:'Edit',command:id,index:f.index,value:target});return;}
    }
  }
  let lastPhase=state.phase;
  function paint(){
    const data=view(state),v=render();root.dataset.phase=data.phase;root.dataset.revision=state.revision;root.dataset.archetype=data.archetype;
    tree.update(v);
    // After a committed or unresolved server request, move keyboard focus to
    // its visible status rather than leaving it on a removed confirmation.
    if(server&&lastPhase==='running'&&['done','refused','error'].includes(data.phase))tree.element.querySelector('[data-region="verdict"]')?.focus?.();
    // Preserve the existing event fields while exposing the recursive view to
    // consumers that inspect the rendered tree.
    lastPhase=data.phase;root.dataset.ready=String(ready);root.dispatchEvent(new CustomEvent('zenoui-view',{detail:{...data,data,view:v},bubbles:true}));
    return data;
  }
  function dispatch(event){
    const previous=state,result=step(state,event);state=result.state;let data;
    try{data=paint();}catch(error){
      state=previous;
      const failed=step(previous,{type:'Fail',binding:previous.active??event.binding});state=failed.state;
      try{paint();}catch{root.textContent='The view could not be displayed. Reload to try again.';root.dataset.phase='error';}
      return;
    }
    for(const intent of result.intents){
      if(!ready)continue;
      const key=canonical(intent.binding);
      if(timers.has(key))clearTimeout(timers.get(key));
      timers.set(key,setTimeout(()=>{
        timers.delete(key);
        // A late durable result must not consume the original request after its view timed out.
        if(options.session&&intent.kind==='dispatch'){retired=true;ready=false;worker.terminate();}
        dispatch({type:'Fail',binding:intent.binding});
      },10000));
      worker.postMessage(intent);
    }
    fillMinimum(data);
  }
  const worker=options.transport??new Worker(new URL('./app-worker.js',import.meta.url),{type:'module'});
  const startupError=()=>{retired=true;worker.terminate();root.textContent='The core could not start. Reload to try again.';root.dataset.phase='error';};
  const startupTimer=setTimeout(startupError,10000);
  worker.onmessage=({data})=>{
    if(retired)return;
    if(data.kind==='error'&&!data.binding){clearTimeout(startupTimer);startupError();return;}
    try{
      if(data.kind==='ready'){clearTimeout(startupTimer);ready=true;dispatch({type:'Refresh'});return;}
      const key=canonical(data.binding);clearTimeout(timers.get(key));timers.delete(key);
      dispatch({type:data.kind==='error'?'Fail':data.kind==='preview'?'Preview':'Result',binding:data.binding,reply:data.reply,delivery:data.delivery});
    }catch{dispatch({type:'Fail',binding:data.binding});}
  };
  worker.onerror=()=>{if(!ready){clearTimeout(startupTimer);startupError();return;}for(const key of timers.keys())dispatch({type:'Fail',binding:JSON.parse(key)});};
  const preferences=()=>dispatch({type:'Preferences',value:{width:innerWidth,theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light',motion:matchMedia('(prefers-reduced-motion: reduce)').matches?'reduce':'normal'}});
  addEventListener('resize',preferences);for(const q of ['(prefers-color-scheme: dark)','(prefers-reduced-motion: reduce)'])matchMedia(q).addEventListener('change',preferences);
  preferences();paint();worker.postMessage({kind:'init',source});
}
