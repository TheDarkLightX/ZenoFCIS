// Shared HTTP transport. Saved bytes and key survive reload; credentials do not.
import {canonical} from './app-contract.js';
import {validReply} from './app-state.js';
export const pendingKey='zeno-fcis/pending-request/1';
export const hex=bytes=>[...bytes].map(v=>v.toString(16).padStart(2,'0')).join('');
export function unhex(text){if(typeof text!=='string'||text.length%2||!/^[0-9a-f]*$/.test(text))throw Error('Invalid request bytes');return Uint8Array.from(text.match(/../g)??[],v=>parseInt(v,16));}
export function envelope(contract,key,revision,command){
  const hash=unhex(contract),retry=unhex(key);if(hash.length!==32||retry.length!==32)throw Error('Invalid request identity');
  const version=BigInt(revision);if(version<0n||version>18446744073709551615n)throw Error('Invalid revision');
  const out=new Uint8Array(73+command.length);out[0]=1;out.set(hash,1);out.set(retry,33);new DataView(out.buffer).setBigUint64(65,version);out.set(command,73);return out;
}
const join=(...parts)=>{const out=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let offset=0;for(const p of parts){out.set(p,offset);offset+=p.length;}return out;};
const digest=async bytes=>new Uint8Array(await crypto.subtle.digest('SHA-256',bytes));
export async function checkPacket(result,saved,description){
  if(!result||!validReply(description,saved.binding.input,result.reply)||!['Accept','CommittedFailure'].includes(result.reply.class)
    ||result.revision!==(BigInt(saved.binding.revision)+1n).toString())throw Error('Invalid recovered result. Keep the original request.');
  const body=unhex(saved.body),packet=unhex(result.packet),text=new TextEncoder();
  if(body.length<74||body[0]!==1||hex(body.slice(1,33))!==saved.contract)throw Error('Invalid retained request.');
  const key=await digest(join(text.encode('zeno-fcis/ingress-key/1'),unhex(saved.principal),unhex(saved.contract),body.slice(33,65)));
  const request=await digest(join(text.encode('zeno-fcis/ingress-request/1'),key,body.slice(65)));
  if(new TextDecoder().decode(packet.slice(0,5))!=='ZFIP1'||hex(packet.slice(5,37))!==hex(request)||hex(packet.slice(37,69))!==hex(key))throw Error('Original request identity differs.');
  let offset=69;
  for(let i=0;i<2;i++){
    if(offset+8>packet.length)throw Error('Incomplete original packet.');
    const length=new DataView(packet.buffer,packet.byteOffset+offset,8).getBigUint64(0);offset+=8;
    if(length>BigInt(packet.length-offset))throw Error('Incomplete original packet.');offset+=Number(length);
  }
  if(offset+40!==packet.length||new DataView(packet.buffer,packet.byteOffset+offset,8).getBigUint64(0).toString()!==result.revision)throw Error('Original packet revision differs.');
}
export class DurableTransport {
  #credential;
  // Browsers refuse fetch called as a method of another object; keep the window receiver.
  constructor(bundle,credential,session,{fetcher=(input,init)=>fetch(input,init),storage=localStorage,random=bytes=>crypto.getRandomValues(bytes)}={}){
    this.bundle=bundle;this.#credential=credential;this.session=structuredClone(session);this.fetcher=fetcher;this.storage=storage;this.random=random;this.retired=false;
  }
  async call(path,body){
    const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),9000);
    try{
      if(this.retired)throw Error('Sign in again to continue.');
      const response=await this.fetcher(path,{method:body?'POST':'GET',headers:{Authorization:`Bearer ${this.#credential}`},body,cache:'no-store',signal:controller.signal});
      if(response.status===401)throw Error('Sign-in refused. Check your application credential and try again. Keep any original request for recovery.');
      if(!response.ok)throw Error(`Request refused (${response.status}). Keep the original request for recovery.`);
      return await response.json();
    }finally{clearTimeout(timer);}
  }
  async recover(){
    const retained=this.storage.getItem(pendingKey),saved=JSON.parse(retained??'null');if(!saved)throw Error('No pending request.');
    if(saved.contract!==this.session.contract||saved.principal!==this.session.principal)throw Error('Pending request belongs to another principal or contract.');
    const result=await this.call('/command',unhex(saved.body));
    await checkPacket(result,saved,this.bundle.description);
    if(this.storage.getItem(pendingKey)!==retained)throw Error('Pending request changed during recovery.');
    this.storage.removeItem(pendingKey);return {saved,result};
  }
  postMessage(data){
    if(this.retired)return;
    if(data.kind==='init'){queueMicrotask(()=>this.onmessage?.({data:{kind:'ready'}}));return;}
    (async()=>{
      if(this.storage.getItem(pendingKey))throw Error('Recover the pending request first.');
      const b=data.binding;
      if(b.contract!==this.session.contract||b.principal!==this.session.principal)throw Error('Request binding changed.');
      const prepared=await this.call('/preview',envelope(b.contract,'00'.repeat(32),b.revision,new TextEncoder().encode(JSON.stringify(b.input))));
      let reply=prepared.reply;
      if(data.kind==='dispatch'){
        if(reply.class!=='Accept'||!validReply(this.bundle.description,b.input,reply))throw Error('Core no longer accepts the confirmed request.');
        const key=hex(this.random(new Uint8Array(32))),body=envelope(b.contract,key,b.revision,unhex(prepared.command));
        // Persist before sending. Storage failure prevents the send.
        this.storage.setItem(pendingKey,JSON.stringify({contract:b.contract,principal:b.principal,binding:b,body:hex(body)}));
        const recovered=await this.recover();reply=recovered.result.reply;
      }
      if(!this.retired)this.onmessage?.({data:{kind:data.kind,binding:b,reply}});
    })().catch(()=>{if(!this.retired)this.onmessage?.({data:{kind:'error',binding:data.binding}});});
  }
  terminate(){this.retired=true;this.#credential='';}
}
