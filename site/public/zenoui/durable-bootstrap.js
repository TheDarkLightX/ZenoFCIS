import {mount} from './app.js';
import {node} from './components.js';
import {DurableTransport,pendingKey} from './durable.js';
const root=document.getElementById('app');
const form=node('form','panel'),label=node('label',null,'Application credential'),input=node('input'),connect=node('button',null,'Connect'),status=node('p');
input.type='password';input.autocomplete='off';input.required=true;input.id='credential';input.maxLength=64;label.htmlFor=input.id;connect.type='submit';status.setAttribute('role','status');
form.append(label,input,connect,status);root.replaceChildren(form);
form.addEventListener('submit',async event=>{
  event.preventDefault();connect.disabled=true;status.textContent='Connecting.';
  try{
    if(!/^[0-9a-f]{64}$/.test(input.value))throw Error('Enter the 64 lowercase hexadecimal credential digits.');
    const bundleResponse=await fetch('/app.json',{cache:'no-store'});if(!bundleResponse.ok)throw Error('Application description unavailable.');const bundle=await bundleResponse.json();
    const provisional=new DurableTransport(bundle,input.value,{});
    const session=await provisional.call('/state');
    const transport=new DurableTransport(bundle,input.value,session);input.value='';
    const recover=node('button',null,'Recover original request'),progress=node('p');recover.type='button';progress.setAttribute('role','status');
    const application=node('div');root.replaceChildren(node('div','panel',recover,progress),application);
    async function show(){
      if(localStorage.getItem(pendingKey)){application.replaceChildren(node('p',null,'A request is unresolved. Recover it with its original key before continuing.'));return;}
      transport.session=await transport.call('/state');await mount(application,bundle,{session:transport.session,transport});
    }
    recover.addEventListener('click',async()=>{
      recover.disabled=true;
      try{const {result}=await transport.recover();progress.textContent=`Original request committed at revision ${result.revision}. Delivery run completed. Reload before continuing.`;application.replaceChildren();}
      catch(error){progress.textContent=error.message;}finally{recover.disabled=false;}
    });
    await show();
  }catch(error){status.textContent=error.message;connect.disabled=false;}
});
