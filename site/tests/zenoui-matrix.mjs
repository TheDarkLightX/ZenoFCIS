// Browser-only evidence: the same generic journeys run against every generated app.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {gzipSync} from 'node:zlib';
import {ORIGIN} from './asset-manifest.mjs';
import {pressKey,waitFor,snapshotExpression} from './browser-checks.mjs';
export async function runMatrix({command,sessionId,assets,root,reportPath}) {
 const output=path.join(path.dirname(reportPath),'matrix');await mkdir(output,{recursive:true});
 const evaluate=async expression=>{const r=await command('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},sessionId);if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
 const wait=(expression,label)=>waitFor(evaluate,expression,label);
 const key=value=>pressKey(command,sessionId,value);
 const click=async selector=>{const point=await evaluate(`(()=>{const b=document.querySelector(${JSON.stringify(selector)});b.scrollIntoView({block:'center'});const r=b.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);for(const type of ['mousePressed','mouseReleased'])await command('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1},sessionId);};
 const configs=[];for(const width of [390,1440])for(const theme of ['light','dark'])for(const motion of ['reduce','no-preference'])configs.push({width,theme,motion,zoom:1});configs.push({width:320,theme:'light',motion:'reduce',zoom:1},{width:1440,theme:'light',motion:'reduce',zoom:2});
 const results=[];let current=null;
 const jsCssBytes=[...assets.values()].filter(a=>/\.(js|css)$/.test(a.file)&&a.file.includes('/app/')).reduce((n,a)=>n+gzipSync(a.body).length,0)/3;
 try{
 assert.ok(jsCssBytes<=200*1024,'shared JS+CSS gzip budget');
 const performanceScript=await command('Page.addScriptToEvaluateOnNewDocument',{source:`globalThis.uiLongTasks=[];new PerformanceObserver(list=>uiLongTasks.push(...list.getEntries().map(e=>e.duration))).observe({type:'longtask',buffered:true});`},sessionId);
 for(const name of ['withdrawal-queue','inventory-reservation','wide-counter']){
  const dir=path.join(root,name),bundle=JSON.parse(await readFile(path.join(dir,'app/app.json'),'utf8')),samples=JSON.parse(await readFile(path.join(dir,'samples.json'),'utf8'));
  const app=bundle.description,stateFields=app.fields.filter(f=>f.source==='state');
  const sample=samples.find(s=>s.reply.class==='Accept'&&s.input.filter((_,i)=>app.fields[i].source==='state').every((v,i)=>v===stateFields[i].initial));assert.ok(sample,'positive genesis journey');
  const id=sample.input[app.action];
  for(const config of configs){
   const label=`${name}-${config.width}-${config.theme}-${config.motion}-${config.zoom}`;
   current={name,...config,label};
   await command('Emulation.setDeviceMetricsOverride',{width:config.width,height:1000,deviceScaleFactor:1,mobile:false},sessionId);
   await command('Emulation.setEmulatedMedia',{features:[{name:'prefers-color-scheme',value:config.theme},{name:'prefers-reduced-motion',value:config.motion}]},sessionId);
   const started=Date.now();
   await command('Page.navigate',{url:`${ORIGIN}/ZenoFCIS/${name}/app/index.html`},sessionId);
   await wait('document.querySelector("#app")?.dataset.ready==="true"',`${label}: ready`);
   const usableMs=Date.now()-started;assert.ok(usableMs<=2000,`${label}: usable view budget ${usableMs}ms`);
   // CSS zoom doubles all text and controls and exercises their reflow at the original viewport width.
   await evaluate(`document.documentElement.style.zoom=${JSON.stringify(String(config.zoom))}`);
   assert.equal(await evaluate('document.querySelectorAll("[data-region]").length'),8,'complete regions');
   assert.equal(await evaluate('document.querySelectorAll("form").length'),app.commands.length,'form for each command');
   await evaluate(`(()=>{const fields=${JSON.stringify(app.fields)},input=${JSON.stringify(sample.input)};for(const f of fields.filter(f=>f.source!=='state'&&f.index!==${app.action})){const e=document.querySelector('#command-${id}-field-'+f.index);e.value=input[f.index];e.dispatchEvent(new Event('input',{bubbles:true}));}})()`);
   const button=`button[data-command="${id}"]`;
   await wait(`!document.querySelector(${JSON.stringify(button)}).disabled`,`${label}: accepted preview`);
   const paints=await evaluate(`(async()=>{const values=[];const field=document.querySelector('#command-${id}-field-'+${app.fields.find(f=>f.source!=='state'&&f.index!==app.action).index});for(let i=0;i<8;i++){const start=performance.now();field.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));values.push(performance.now()-start);}return values;})()`);
   const p95=[...paints].sort((a,b)=>a-b).at(-1);assert.ok(p95<=100,`${label}: input-to-paint p95 ${p95}ms`);
   await wait(`!document.querySelector(${JSON.stringify(button)}).disabled`,`${label}: previews after paint samples`);
   // A real keyboard journey reaches the form submit and the native confirmation button.
   await evaluate(`document.querySelector(${JSON.stringify(button)}).focus()`);await key('Tab');
   assert.ok(await evaluate('!!document.activeElement && document.activeElement!==document.body'),'keyboard focus exists');
   await evaluate(`document.querySelector(${JSON.stringify(button)}).focus()`);await key('Enter');
   await wait('!!document.querySelector("[data-region=confirmation] button")',`${label}: keyboard review`);
   assert.ok((await evaluate('document.querySelector("[data-region=confirmation]").textContent')).includes('Local operator'),'named principal');
   const first=app.fields.find(f=>f.source!=='state'&&f.index!==app.action);
   await evaluate(`(()=>{const e=document.querySelector('#command-${id}-field-${first.index}');e.dispatchEvent(new Event('input',{bubbles:true}));})()`);
   assert.equal(await evaluate('!!document.querySelector("[data-region=confirmation] button")'),false,'edit voids consent');
   await wait(`!document.querySelector(${JSON.stringify(button)}).disabled`,`${label}: refreshed preview`);
   await evaluate(`document.querySelector(${JSON.stringify(button)}).focus()`);await key('Enter');
   await evaluate('document.querySelector("[data-region=confirmation] button").focus()');await key('Enter');
   await wait('document.querySelector("#app").dataset.revision==="1"',`${label}: keyboard dispatch`);
   const visible=await evaluate('document.querySelector("[data-region=state]").textContent');
   for(let i=0;i<stateFields.length;i++){const f=stateFields[i],value=f.options.find(o=>o.value===sample.reply.post[i])?.label??sample.reply.post[i];assert.ok(visible.includes(`${f.label}: ${value} ${f.unit}`),'native state parity and exact strings');}
   // Pointer journey through an available command on the new state.
   await wait('[...document.querySelectorAll("form button")].some(b=>!b.disabled)',`${label}: next command`);
   const pointerButton=await evaluate('(()=>{const b=[...document.querySelectorAll("form button")].find(b=>!b.disabled);return `button[data-command="${b.dataset.command}"]`;})()');
   await click(pointerButton);await click('[data-region="confirmation"] button');
   await wait('document.querySelector("#app").dataset.revision==="2"',`${label}: pointer dispatch`);
   const accessibility=await command('Accessibility.getFullAXTree',{},sessionId);
   const roles=new Set(['button','textbox','combobox']);for(const n of accessibility.nodes.filter(n=>!n.ignored&&roles.has(n.role?.value)))assert.ok(n.name?.value?.trim(),'accessible control name');
   assert.ok(accessibility.nodes.some(n=>n.role?.value==='status'),'status announcement');
   const geometry=await evaluate(`(()=>{const regions=[...document.querySelectorAll('[data-region]')].map(e=>{const r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,left:r.left,right:r.right};}).sort((a,b)=>a.top-b.top);const targets=[...document.querySelectorAll('button,input,select,summary')].map(e=>{const r=e.getBoundingClientRect();return {width:r.width,height:r.height};}).filter(r=>r.width&&r.height);return {regions,targets};})()`);
   for(let i=1;i<geometry.regions.length;i++)assert.ok(geometry.regions[i].top>=geometry.regions[i-1].bottom-1,`${label}: regions do not overlap`);
   for(const target of geometry.targets)assert.ok(target.width>=24&&target.height>=24,`${label}: target size`);
   const overflow=await evaluate('({scroll:document.documentElement.scrollWidth,width:document.documentElement.clientWidth})');assert.ok(overflow.scroll<=overflow.width+1,`${label}: horizontal reflow`);
   // A blank integer (or invalid enum value) refuses the form and preserves the draft.
   await evaluate(`(()=>{const e=document.querySelector('#command-${id}-field-${first.index}');e.value='';e.dispatchEvent(new Event('input',{bubbles:true}));})()`);
   assert.equal(await evaluate(`document.querySelector(${JSON.stringify(button)}).disabled`),true,'invalid draft unavailable');
   assert.ok((await evaluate('document.querySelector("[data-region=commands]").textContent')).includes('declared bounds'),'plain refusal feedback');
   const screenshot=await command('Page.captureScreenshot',{format:'png',captureBeyondViewport:true},sessionId);await writeFile(path.join(output,`${label}.png`),Buffer.from(screenshot.data,'base64'));
   const stalls=await evaluate('uiLongTasks');const maxStall=Math.max(0,...stalls);assert.ok(maxStall<=250,`${label}: main-thread stall ${maxStall}ms`);
   results.push({name,...config,usableMs,inputToPaintP95Ms:p95,maxMainThreadStallMs:maxStall,keyboard:true,pointer:true,accessibleNames:true,announcements:true,nativeStateParity:true,editVoidsConsent:true,reflow:true,screenshot:`${label}.png`});
   const coreUrl=`${ORIGIN}/ZenoFCIS/${name}/app/core.wasm`,coreAsset=assets.get(coreUrl);assert.ok(coreAsset);
   assets.delete(coreUrl);
   try{await command('Page.navigate',{url:`${ORIGIN}/ZenoFCIS/${name}/app/index.html`},sessionId);await wait('document.querySelector("#app")?.dataset.phase==="error"',`${label}: startup error`);assert.match(await evaluate('document.querySelector("#app").textContent'),/could not start/);}finally{assets.set(coreUrl,coreAsset);}
   const fault=await command('Page.addScriptToEvaluateOnNewDocument',{source:`const RealWorker=Worker;globalThis.Worker=class extends RealWorker{set onmessage(listener){super.onmessage=event=>{if(event.data.kind!=='dispatch')listener(event);};}};const realTimeout=setTimeout;globalThis.setTimeout=(f,ms,...args)=>realTimeout(f,ms===10000&&f.name!=='startupError'?300:ms,...args);`},sessionId);
   try{
    await command('Page.navigate',{url:`${ORIGIN}/ZenoFCIS/${name}/app/index.html`},sessionId);await wait('document.querySelector("#app")?.dataset.ready==="true"',`${label}: timeout page ready`);
    await evaluate(`(()=>{for(const f of ${JSON.stringify(app.fields.filter(f=>f.source!=='state'&&f.index!==app.action))}){const e=document.querySelector('#command-${id}-field-'+f.index);e.value=${JSON.stringify(sample.input)}[f.index];e.dispatchEvent(new Event('input',{bubbles:true}));}})()`);
    await wait(`!document.querySelector(${JSON.stringify(button)}).disabled`,`${label}: timeout preview`);
    await click(button);await click('[data-region="confirmation"] button');
    await wait('document.querySelector("#app").dataset.phase==="error"',`${label}: timeout`);
    assert.equal(await evaluate('document.querySelector("#app").dataset.revision'),'0');assert.match(await evaluate('document.querySelector("[data-region=verdict]").textContent'),/No result/);
   }finally{await command('Page.removeScriptToEvaluateOnNewDocument',{identifier:fault.identifier},sessionId);}
   results.at(-1).error=true;results.at(-1).timeout=true;
   console.log(`${label}: passed`);
  }
 }
 // The route guard is independent of any UI: unknown and external URLs must fail.
 const refused=await evaluate(`Promise.all(['${ORIGIN}/unknown','https://external.invalid/no'].map(url=>fetch(url).then(()=>false,()=>true)))`);assert.deepEqual(refused,[true,true]);
 await command('Page.removeScriptToEvaluateOnNewDocument',{identifier:performanceScript.identifier},sessionId);
 return {runs:results,jsCssGzipBytes:jsCssBytes,zoomScope:'200% CSS zoom with full text/control reflow; manual browser zoom and screen-reader audit remain separate'};
 }catch(error){
  error.matrix={runs:results,current,jsCssGzipBytes:jsCssBytes};
  error.browser={...error.browser,configuration:current,snapshot:await evaluate(snapshotExpression).catch(e=>({unavailable:e.message}))};
  throw error;
 }
}
