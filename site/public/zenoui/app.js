import {initial,step,view} from './app-state.js';
import {canonical,freeze,layoutCheck} from './app-contract.js';
import {node,metrics,control,verdict,comparison,ledger,record} from './components.js';
export async function sha256(value) {
  const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(canonical(value)));
  return [...new Uint8Array(bytes)].map(b=>b.toString(16).padStart(2,'0')).join('');
}
export async function mount(root,bundle) {
  const {description,source,contract}=bundle;
  if(contract!==await sha256({description,source}))throw Error('UiContractIdentity');
  freeze(bundle);layoutCheck(description.layout);
  let state=initial(description,contract),ready=false,retired=false;
  const regions=new Map(),buttons=new Map(),controls=[],timers=new Map();let lastConfirmation;
  const title=node('h1',null,description.title),intro=node('p',null,'Review a command, confirm its details, then let the core decide. This local session keeps its state only while the page is open.');
  const header=node('header','banner',title,intro);
  const main=node('main');
  for(const {id} of description.layout){const region=node('section','panel');region.dataset.region=id;regions.set(id,region);main.append(region);}
  root.replaceChildren(header,main);
  const principal=control({label:'Principal for this local session',unit:'unitless',domain:{kind:'text',min:'1',max:'100'},options:[]},'principal',state.principal);
  principal.input.inputMode='text';principal.input.maxLength=100;principal.input.removeAttribute('pattern');principal.input.removeAttribute('required');
  principal.input.addEventListener('input',()=>dispatch({type:'Principal',value:principal.input.value}));
  const retry=node('button',null,'Check commands again');retry.type='button';retry.addEventListener('click',()=>dispatch({type:'Refresh'}));
  regions.get('commands').append(node('h2',null,'Commands'),node('p','help','Context values below are supplied locally; they are not authenticated credentials.'),principal.wrapper,retry);
  for(const command of description.commands){
    const form=node('form','controls');form.setAttribute('aria-label',command.label);
    form.append(node('h3',null,command.label));
    for(const field of description.fields.filter(f=>f.source!=='state'&&f.index!==description.action)){
      const c=control(field,`command-${command.id}-field-${field.index}`,state.drafts[command.id][field.index]);
      c.input.addEventListener('input',()=>dispatch({type:'Edit',command:command.id,index:field.index,value:c.input.value}));
      controls.push(c.input);form.append(c.wrapper);
    }
    const button=node('button',null,`Review ${command.label}`);button.type='submit';button.disabled=true;button.dataset.command=command.id;
    const feedback=node('p','help','Waiting for the core.');feedback.id=`reason-${command.id}`;button.setAttribute('aria-describedby',feedback.id);
    form.append(button,feedback);form.addEventListener('submit',event=>{event.preventDefault();dispatch({type:'Review',command:command.id});});
    buttons.set(command.id,{button,feedback});regions.get('commands').append(form);
  }
  const live=verdict('Waiting for the core.');live.className='verdict status';live.setAttribute('role','status');live.setAttribute('aria-live','polite');live.setAttribute('aria-atomic','true');regions.get('verdict').append(live);
  const confirm=node('button');confirm.type='button';confirm.addEventListener('click',()=>{
    const command=state.review?.command;dispatch({type:'Confirm'});dispatch({type:'Dispatch',command});
  });
  function paint(){
    const data=view(state);root.dataset.phase=data.phase;root.dataset.revision=state.revision;
    if([...main.children].some((n,i)=>n!==regions.get(data.layout[i].id))){const focus=document.activeElement;for(const {id} of data.layout)main.append(regions.get(id));focus?.focus?.({preventScroll:true});}
    regions.get('state').replaceChildren(node('h2',null,'Current state'),...data.state.map(f=>node('p',null,`${f.label}: ${f.value} (${f.type}).`)));
    for(const a of data.actions){const c=buttons.get(a.id);c.button.disabled=!a.enabled;c.feedback.textContent=a.explanation;}
    for(const c of controls)c.disabled=!!state.active;principal.input.disabled=!!state.active;retry.disabled=!ready||!!state.active;
    const confirmation=regions.get('confirmation');
    const confirmationKey=canonical(data.confirmation);
    if(confirmationKey!==lastConfirmation){
    lastConfirmation=confirmationKey;confirmation.replaceChildren(node('h2',null,'Confirmation'));
    if(data.confirmation){
      const c=data.confirmation;confirmation.append(node('h3',null,c.label),...c.fields.map(f=>node('p',null,`${f.label}: ${f.value} (${f.type}).`)),node('p',null,`Principal: ${c.principal}. Revision: ${c.revision}.`),node('p',null,`Contract: ${description.title}.`),record({contract:c.contract}));
      confirm.textContent=`Confirm ${c.label}`;confirmation.append(confirm);
    }else confirmation.append(node('p','help','No command awaiting confirmation.'));
    }
    live.textContent=data.message;
    regions.get('metrics').replaceChildren(metrics([`${data.evidence.checked} of ${data.evidence.total}`,'Requests checked']));
    regions.get('comparison').replaceChildren(comparison('The core rechecks every confirmed request before returning its decision.'));
    regions.get('ledger').replaceChildren(node('h2',null,'Local decisions'),ledger(data.ledger));
    regions.get('record').replaceChildren(record({contract,revision:state.revision,epoch:state.epoch,run:state.run,phase:state.phase}));
    root.dataset.ready=String(ready);root.dispatchEvent(new CustomEvent('zenoui-view',{detail:data}));
  }
  function dispatch(event){
    const previous=state,result=step(state,event);state=result.state;
    try{paint();}catch(error){
      state=previous;
      const failed=step(previous,{type:'Fail',binding:previous.active??event.binding});state=failed.state;
      try{paint();}catch{root.textContent='The view could not be displayed. Reload to try again.';root.dataset.phase='error';}
      return;
    }
    for(const intent of result.intents){
      if(!ready)continue;
      const key=canonical(intent.binding);
      if(timers.has(key))clearTimeout(timers.get(key));
      timers.set(key,setTimeout(()=>{timers.delete(key);dispatch({type:'Fail',binding:intent.binding});},10000));
      worker.postMessage(intent);
    }
  }
  const worker=new Worker(new URL('./app-worker.js',import.meta.url),{type:'module'});
  const startupError=()=>{retired=true;worker.terminate();root.textContent='The core could not start. Reload to try again.';root.dataset.phase='error';};
  const startupTimer=setTimeout(startupError,10000);
  worker.onmessage=({data})=>{
    if(retired)return;
    if(data.kind==='error'&&!data.binding){clearTimeout(startupTimer);startupError();return;}
    try{
      if(data.kind==='ready'){clearTimeout(startupTimer);ready=true;dispatch({type:'Refresh'});return;}
      const key=canonical(data.binding);clearTimeout(timers.get(key));timers.delete(key);
      dispatch({type:data.kind==='error'?'Fail':data.kind==='preview'?'Preview':'Result',binding:data.binding,reply:data.reply});
    }catch{dispatch({type:'Fail',binding:data.binding});}
  };
  worker.onerror=()=>{if(!ready){clearTimeout(startupTimer);startupError();return;}for(const key of timers.keys())dispatch({type:'Fail',binding:JSON.parse(key)});};
  const preferences=()=>dispatch({type:'Preferences',value:{width:innerWidth,theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light',motion:matchMedia('(prefers-reduced-motion: reduce)').matches?'reduce':'normal'}});
  addEventListener('resize',preferences);for(const q of ['(prefers-color-scheme: dark)','(prefers-reduced-motion: reduce)'])matchMedia(q).addEventListener('change',preferences);
  preferences();paint();worker.postMessage({kind:'init',source});
}
