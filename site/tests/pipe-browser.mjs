// Socket-free server replacement. Chrome itself may still require IPC sockets.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {setTimeout as delay} from 'node:timers/promises';
import {loadAssets,ORIGIN} from './asset-manifest.mjs';
const [chrome,profile,root,manifestPath,nativePath,reportPath]=process.argv.slice(2);
if(!reportPath)throw new Error('usage: pipe-browser.mjs CHROME PROFILE PUBLIC MANIFEST NATIVE REPORT');
const manifest=JSON.parse(await readFile(manifestPath,'utf8')),assets=await loadAssets(root,manifest);
const expected=JSON.parse(await readFile(nativePath,'utf8')).search;
const args=['--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--disable-extensions','--disable-background-networking','--disable-component-update','--disable-sync','--disable-crashpad-for-testing','--remote-debugging-pipe',`--user-data-dir=${profile}`];
if(process.getuid?.()===0||process.env.SITE_CHROME_NO_SANDBOX==='1')args.push('--no-sandbox');
const browser=spawn(chrome,args,{stdio:['ignore','ignore','pipe','pipe','pipe']});
const pending=new Map(),sessions=new Map(),targetSessions=new Map(),fulfilled=[],denied=[];let sequence=0,buffer='',stderr='',stopped,workerAttached=0;
function stop(error){stopped??=error;for(const p of pending.values())p.reject(stopped);pending.clear();}
browser.stderr.setEncoding('utf8');browser.stderr.on('data',data=>{stderr=(stderr+data).slice(-12000);});
browser.on('error',stop);const closed=new Promise(resolve=>browser.on('close',(code,signal)=>{stop(new Error(`Chrome closed (${code??signal}): ${stderr}`));resolve();}));
for(const pipe of [browser.stdio[3],browser.stdio[4]])pipe.on('error',stop);
function command(method,params={},sessionId,timeout=15000){if(stopped)return Promise.reject(stopped);const id=++sequence;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`DevTools timeout: ${method}`));},timeout);pending.set(id,{resolve:value=>{clearTimeout(timer);resolve(value);},reject:error=>{clearTimeout(timer);reject(error);}});browser.stdio[3].write(JSON.stringify({id,method,params,sessionId})+'\0');});}
async function event(message){
 const session=message.sessionId,p=message.params;
 if(message.method==='Target.attachedToTarget'){
  targetSessions.set(p.targetInfo.targetId,p.sessionId);
  const ready=(async()=>{
   if(p.targetInfo.type==='worker')workerAttached++;
   await command('Fetch.enable',{patterns:[{urlPattern:'*',requestStage:'Request'}]},p.sessionId);
   await command('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:true,flatten:true},p.sessionId);
   await command('Runtime.runIfWaitingForDebugger',{},p.sessionId);
  })();sessions.set(p.sessionId,ready);await ready;
 }
 if(message.method==='Fetch.requestPaused'){
  const asset=p.request.method==='GET'?assets.get(p.request.url):null;
  if(!asset){denied.push(p.request.url);await command('Fetch.failRequest',{requestId:p.requestId,errorReason:'BlockedByClient'},session);return;}
  fulfilled.push({url:asset.url,sha256:asset.sha256});
  await command('Fetch.fulfillRequest',{requestId:p.requestId,responseCode:200,responseHeaders:[{name:'Content-Type',value:asset.type},{name:'Cache-Control',value:'no-store'}],body:asset.body.toString('base64')},session);
 }
}
browser.stdio[4].setEncoding('utf8');browser.stdio[4].on('data',data=>{buffer+=data;let end;while((end=buffer.indexOf('\0'))>=0){const raw=buffer.slice(0,end);buffer=buffer.slice(end+1);try{const message=JSON.parse(raw);if(message.id){const p=pending.get(message.id);pending.delete(message.id);if(p){if(message.error)p.reject(new Error(JSON.stringify(message.error)));else p.resolve(message.result);}}else event(message).catch(stop);}catch(error){stop(error);}}});
let parity=false,errorText=null;
try{
 await command('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:true,flatten:true});
 const {targetId}=await command('Target.createTarget',{url:'about:blank'});
 const attachmentDeadline=Date.now()+5000;
 while(!targetSessions.has(targetId)){if(stopped)throw stopped;if(Date.now()>attachmentDeadline)throw new Error('Page auto-attachment missing');await delay(20);}
 const sessionId=targetSessions.get(targetId);await sessions.get(sessionId);
 await command('Page.enable',{},sessionId);
 await command('Page.navigate',{url:`${ORIGIN}/ZenoFCIS/index.html`},sessionId);
 const deadline=Date.now()+20000;
 while(true){
  const r=await command('Runtime.evaluate',{returnByValue:true,expression:'document.querySelectorAll("#factory [data-region]").length === 16'},sessionId);
  if(r.result?.value)break;if(stopped)throw stopped;if(Date.now()>deadline)throw new Error('Page not ready');await delay(50);
 }
 const r=await command('Runtime.evaluate',{returnByValue:true,awaitPromise:true,expression:`new Promise((resolve,reject)=>{
   const root=document.querySelector('#factory');
   root.addEventListener('factory-result',event=>{if(event.detail.result.act==='search')resolve(event.detail.result)},{once:true});
   setTimeout(()=>reject(new Error('Act timeout')),12000);
   root.querySelector('button[data-act="search"]').click();
 })`},sessionId);
 if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));assert.deepEqual(r.result.value,expected,'browser worker / Wasm / native parity');
 assert.ok(workerAttached>0,'module worker attached');
 assert.ok(fulfilled.some(x=>x.url.endsWith('/factory.wasm')),'Wasm fulfilled from hashed manifest');
 const negative=await command('Runtime.evaluate',{returnByValue:true,awaitPromise:true,expression:`Promise.all(['${ORIGIN}/unknown-asset','https://external.invalid/no'].map(url=>fetch(url).then(()=>false,()=>true)))`},sessionId);
 assert.deepEqual(negative.result.value,[true,true],'unknown and external routes refused');parity=true;
}catch(error){errorText=error.message;process.exitCode=1;console.error(errorText);}
finally{
 await command('Browser.close',{},undefined,1000).catch(()=>{});const timer=setTimeout(()=>browser.kill('SIGKILL'),1000);await closed;clearTimeout(timer);
 const report={arguments:args,format:'zenoui/browser-spike/1',manifest:manifest.sha256,parity,workerAttached,fulfilled,denied,error:errorText,stderr};await writeFile(reportPath,JSON.stringify(report,null,2)+'\n');
 console.log(JSON.stringify({parity,workerAttached,fulfilled:fulfilled.length,denied:denied.length,report:reportPath}));
}
