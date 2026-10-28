import {mount} from './app.js';
import {deliveryStatus,pendingSummary} from './app-state.js';
import {node} from './components.js';
import {DurableTransport,pendingKey} from './durable.js';
const root=document.getElementById('app');
const form=node('form','zu-signin zu-region'),label=node('label','zu-label','Application credential'),input=node('input','zu-input'),connect=node('button','zu-button zu-button--primary','Sign in'),status=node('p','zu-help','Sign in to review the requests allowed by your access.');
form.dataset.region='signin';form.noValidate=true;
input.type='password';input.autocomplete='off';input.required=true;input.id='credential';input.maxLength=64;label.htmlFor=input.id;connect.type='submit';status.setAttribute('role','status');status.dataset.region='signin-status';
form.append(node('h1',null,'Sign in'),label,input,connect,status);root.replaceChildren(form);
form.addEventListener('submit',async event=>{
  event.preventDefault();
  if(connect.disabled)return;
  connect.disabled=true;status.textContent='Connecting.';
  let transport;
  try{
    if(!/^[0-9a-f]{64}$/.test(input.value))throw Error('Sign-in refused. Enter the 64 lowercase hexadecimal credential digits.');
    // Capture and clear before awaiting: credentials belong only to this transport.
    transport=new DurableTransport(null,input.value,{});input.value='';
    transport.session=await transport.call('/state');
    const bundle=await transport.call('/app.json');transport.bundle=bundle;
    const recover=node('button','zu-button','Recover original request'),progress=node('p','zu-help');recover.type='button';progress.setAttribute('role','status');
    const application=node('div');
    if(localStorage.getItem(pendingKey))application.replaceChildren(node('p',null,'A request is unresolved. Recover it with its original key before continuing.'),node('p','help',pendingSummary(transport.session.pending,transport.session.revision)));
    else await mount(application,bundle,{session:transport.session,transport});
    // Keep the sign-in status attached until authentication and mounting succeed.
    const footer=node('div','zu-recovery-foot',recover,progress);
    const updateRecovery=()=>{const retained=!!localStorage.getItem(pendingKey);recover.hidden=!retained;footer.hidden=!retained&&!progress.textContent;};
    root.replaceChildren(application,footer);
    root.addEventListener('zenoui-view',updateRecovery);updateRecovery();
    recover.addEventListener('click',async()=>{
      recover.disabled=true;
      try{const {result}=await transport.recover();const delivery=deliveryStatus(result.delivery)==='pending'?'The server reported delivery pending for this request.':'The recovered response includes no delivery confirmation.';progress.textContent=`Original request committed at revision ${result.revision}. ${delivery} Reload before continuing.`;application.replaceChildren();}
      catch(error){progress.textContent=error.message;}finally{recover.disabled=false;updateRecovery();}
    });
  }catch(error){transport?.terminate();input.value='';status.textContent=error.message;connect.disabled=false;}
});
