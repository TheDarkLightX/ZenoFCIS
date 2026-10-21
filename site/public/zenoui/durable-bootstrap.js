import {mount} from './app.js';
import {node} from './components.js';
import {DurableTransport,pendingKey} from './durable.js';
const root=document.getElementById('app');
const form=node('form','panel'),label=node('label',null,'Application credential'),input=node('input'),connect=node('button',null,'Sign in'),status=node('p',null,'Sign in required. Enter your application credential to see commands.');
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
    const recover=node('button',null,'Recover original request'),progress=node('p');recover.type='button';progress.setAttribute('role','status');
    const application=node('div');
    if(localStorage.getItem(pendingKey))application.replaceChildren(node('p',null,'A request is unresolved. Recover it with its original key before continuing.'));
    else await mount(application,bundle,{session:transport.session,transport});
    // Keep the sign-in status attached until authentication and mounting succeed.
    root.replaceChildren(node('div','panel',recover,progress),application);
    recover.addEventListener('click',async()=>{
      recover.disabled=true;
      try{const {result}=await transport.recover();progress.textContent=`Original request committed at revision ${result.revision}. Delivery run completed. Reload before continuing.`;application.replaceChildren();}
      catch(error){progress.textContent=error.message;}finally{recover.disabled=false;}
    });
  }catch(error){transport?.terminate();input.value='';status.textContent=error.message;connect.disabled=false;}
});
