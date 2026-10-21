// Untrusted transport. Every invocation resets preparation and calls the real core.
import {instantiate} from '../demo-module.js';
let api,source;
self.onmessage=async ({data})=>{
  try {
    if(data.kind==='init'){
      source=structuredClone(data.source);
      api=await instantiate(await (await fetch(new URL('../core.wasm',import.meta.url))).arrayBuffer());
      self.postMessage({kind:'ready'});return;
    }
    if(!api||!source)throw Error('UiCoreNotReady');
    api.reset();
    const text=Array.from(JSON.stringify(source));
    for(let i=0;i<text.length;i+=500){const r=api.step({act:'ui_append',chunk:text.slice(i,i+500).join('')});if(r.error)throw Error(r.error);}
    const reply=api.step({act:'ui_decide',input:data.binding.input});if(reply.error)throw Error(reply.error);
    self.postMessage({kind:data.kind,binding:data.binding,reply});
  }catch(error){self.postMessage({kind:'error',binding:data.binding,message:String(error.message)});}
};
