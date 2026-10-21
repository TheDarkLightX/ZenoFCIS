// Host-only Chrome journey over the running Rust server; no asset substitution.
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {waitFor,failureDetails,pressKey} from './browser-checks.mjs';
const [chrome,profile,url,credentialFile,fixtureFile,reportFile]=process.argv.slice(2);
if(!reportFile)throw Error('usage: app-factory-browser.mjs CHROME PROFILE URL CREDENTIAL FIXTURE REPORT');
const credential=(await readFile(credentialFile,'utf8')).trim(),fixture=JSON.parse(await readFile(fixtureFile,'utf8'));
const browser=spawn('python3',[fileURLToPath(new URL('./exec_chrome.py',import.meta.url)),chrome,'--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--disable-extensions','--disable-background-networking','--disable-component-update','--disable-sync','--remote-debugging-pipe',`--user-data-dir=${profile}`],{stdio:['ignore','ignore','pipe','pipe','pipe']});
let sequence=0,buffer='',stderr='',stopped,lost=false,dropped=0,session;
const pending=new Map(),requests=[],responses=[],pageErrors=[],authentication=[];
const redact=text=>String(text).split(credential).join('[credential]');
function stop(error){stopped??=error;for(const p of pending.values())p.reject(stopped);pending.clear();}
const closed=new Promise(resolve=>browser.on('close',(code,signal)=>{stop(Error(`Chrome closed (${code??signal})`));resolve();}));
browser.on('error',stop);browser.stderr.setEncoding('utf8');browser.stderr.on('data',data=>{stderr=(stderr+data).slice(-8000);});
function command(method,params={},sessionId=session){if(stopped)return Promise.reject(stopped);const id=++sequence;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(Error(`DevTools timeout: ${method}`));},15000);pending.set(id,{resolve:v=>{clearTimeout(timer);resolve(v);},reject:e=>{clearTimeout(timer);reject(e);}});browser.stdio[3].write(JSON.stringify({id,method,params,sessionId:method.startsWith("Browser.")||method.startsWith("Target.")?undefined:sessionId})+'\0');});}
async function event(message){
 if(message.method==='Runtime.exceptionThrown'){pageErrors.push(redact(message.params.exceptionDetails.exception?.description??message.params.exceptionDetails.text));return;}
 if(message.method!=='Fetch.requestPaused')return;
 const p=message.params;
 if(p.responseStatusCode!==undefined){
  responses.push({path:new URL(p.request.url).pathname,status:p.responseStatusCode});
  if(lost&&p.request.url===url+'/command'&&p.responseStatusCode===200){lost=false;dropped++;await command('Fetch.failRequest',{requestId:p.requestId,errorReason:'ConnectionClosed'});return;}
  await command('Fetch.continueResponse',{requestId:p.requestId});return;
 }
 if(!p.request.url.startsWith(url+'/')){await command('Fetch.failRequest',{requestId:p.requestId,errorReason:'BlockedByClient'});return;}
 const parsed=new URL(p.request.url),path=parsed.pathname;
 assert.equal(parsed.search,'','credentials never travel in a URL');
 requests.push(path);
 if(['/state','/app.json','/preview','/command'].includes(path)){
  const auth=Object.entries(p.request.headers).filter(([name])=>name.toLowerCase()==='authorization');
  const authenticated=auth.length===1&&auth[0][1]==='Bearer '+credential;
  authentication.push({path,authenticated});
  if(['/app.json','/preview','/command'].includes(path))assert.ok(authenticated,'application requests carry the signed-in credential');
 }
 await command('Fetch.continueRequest',{requestId:p.requestId});
}
browser.stdio[4].setEncoding('utf8');browser.stdio[4].on('data',data=>{buffer+=data;let end;while((end=buffer.indexOf('\0'))>=0){const raw=buffer.slice(0,end);buffer=buffer.slice(end+1);try{const m=JSON.parse(raw);if(m.id){const p=pending.get(m.id);pending.delete(m.id);if(p){if(m.error)p.reject(Error(m.error.message));else p.resolve(m.result);}}else event(m).catch(stop);}catch(error){stop(error);}}});
const evaluate=async expression=>{const r=await command('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.text);return r.result.value;};
let failure=null,diagnostic=null,negativeCases=false;
try{
 const target=await command('Target.createTarget',{url:'about:blank'},undefined);const attached=await command('Target.attachToTarget',{targetId:target.targetId,flatten:true},undefined);session=attached.sessionId;
 await command('Page.enable');await command('Runtime.enable');
 await command('Fetch.enable',{patterns:[{urlPattern:'*',requestStage:'Request'},{urlPattern:'*',requestStage:'Response'}]});
 const login=async()=>{
  await waitFor(evaluate,`!!document.querySelector('#credential')`,'credential form');
  await evaluate(`document.querySelector('#credential').focus()`);await command('Input.insertText',{text:credential});await pressKey(command,session,'Enter');
 };
 await command('Page.navigate',{url:url+'/'});
 await waitFor(evaluate,`!!document.querySelector('#credential')`,'credential form');
 assert.ok(await evaluate(`document.body.textContent.includes('Sign in required')&&!document.querySelector('[data-region=commands]')`),'signed-out browser offers no commands');
 assert.equal(await evaluate(`fetch('/state',{cache:'no-store'}).then(r=>r.status)`),401,'no credential is refused by ingress');
 await evaluate(`document.querySelector('#credential').focus()`);await pressKey(command,session,'Enter');
 await waitFor(evaluate,`document.querySelector('[data-region=signin-status]').textContent.includes('Sign-in refused')`,'empty sign-in refusal');
 assert.equal(await evaluate(`!!document.querySelector('[data-region=commands]')`),false);
 const wrong=(credential[0]==='0'?'1':'0')+credential.slice(1);
 await evaluate(`document.querySelector('#credential').focus()`);await command('Input.insertText',{text:wrong});await pressKey(command,session,'Enter');
 await waitFor(evaluate,`document.querySelector('[data-region=signin-status]').textContent.includes('Check your application credential')`,'wrong-credential sign-in refusal');
 assert.equal(await evaluate(`!!document.querySelector('[data-region=commands]')`),false);negativeCases=true;
 await login();
 await waitFor(evaluate,`!!document.querySelector('[data-region=commands]')`,'server snapshot and commands');
 const first=fixture.steps[0];
 await evaluate(`(()=>{const command=${JSON.stringify(first.command[0])},values=${JSON.stringify(first.command)};const form=document.querySelector('button[data-command="'+command+'"]').closest('form');const inputs=[...form.querySelectorAll('input,select')].filter(e=>!e.disabled);for(let i=0;i<inputs.length;i++){inputs[i].value=values[i+1];inputs[i].dispatchEvent(new Event('input',{bubbles:true}));}})()`);
 await waitFor(evaluate,`!document.querySelector('button[data-command="${first.command[0]}"]').disabled`,'core accepts first command');
 await evaluate(`document.querySelector('button[data-command="${first.command[0]}"]').focus()`);await pressKey(command,session,'Enter');
 await waitFor(evaluate,`!!document.querySelector('[data-region=confirmation] button')`,'bound confirmation');
 lost=true;await evaluate(`document.querySelector('[data-region=confirmation] button').focus()`);await pressKey(command,session,'Enter');
 await waitFor(evaluate,`document.querySelector('[data-phase=error]')!==null`,'lost response remains unresolved');assert.equal(dropped,1);
 await command('Page.reload');await login();
 await waitFor(evaluate,`document.body.textContent.includes('A request is unresolved')`,'reload retains original key');
 await evaluate(`document.querySelector('button').focus()`);await pressKey(command,session,'Enter');
 await waitFor(evaluate,`document.body.textContent.includes('Original request committed at revision 1')`,'original-key recovery');
 await command('Page.reload');await login();
 await waitFor(evaluate,`!!document.querySelector('[data-revision="1"]')`,'durable snapshot after reload');
 assert.ok(await evaluate(`document.querySelectorAll('[data-region=commands] button[data-command]:disabled').length>0`),'core-refused command stays disabled');
 assert.ok(requests.includes('/preview')&&requests.filter(p=>p==='/command').length===2,'original request was retried once');
 assert.equal(await evaluate(`Object.keys(localStorage).some(k=>localStorage.getItem(k).includes(${JSON.stringify(credential)}))||Object.keys(sessionStorage).some(k=>sessionStorage.getItem(k).includes(${JSON.stringify(credential)}))`),false,'credential never enters browser storage');
 assert.deepEqual(pageErrors,[],'no page exceptions');
 await command('Page.captureScreenshot',{format:'png'}).then(r=>writeFile(reportFile+'.png',Buffer.from(r.data,'base64')));
}catch(error){failure=failureDetails(error);diagnostic=await evaluate(`({text:document.querySelector('#app')?.textContent.slice(0,1500),phase:document.querySelector('#app')?.dataset,signIn:document.querySelector('[data-region=signin-status]')?.textContent})`).catch(()=>null);process.exitCode=1;}
finally{
 await command('Browser.close',{},undefined).catch(()=>{});const timer=setTimeout(()=>browser.kill('SIGKILL'),1000);await closed;clearTimeout(timer);
 await writeFile(reportFile,redact(JSON.stringify({format:'zeno-fcis/app-browser/2',pass:!failure,negativeCases,dropped,requests,responses,authentication,pageErrors,diagnostic,failure,stderr},null,2))+'\n');
 if(failure)console.error(failure.message);else console.log('app browser: PASS');
}
