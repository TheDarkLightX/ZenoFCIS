import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import path from 'node:path';
import {instantiate} from '../public/demo-module.js';
import {documentFrom,moduleLoader,localWorkerClass,ORIGIN} from './browser-fixture.mjs';
const root=process.argv[2];if(!root)throw Error('evidence directory required');
const wait=async (test,label)=>{for(let i=0;i<1000;i++){if(test())return;await new Promise(r=>setTimeout(r,10));}throw Error(label);};
for(const name of (process.argv.length>3?process.argv.slice(3):['withdrawal-queue','inventory-reservation','wide-counter'])){
 const dir=path.join(root,name),appDir=path.join(dir,'app'),bundle=JSON.parse(await readFile(path.join(appDir,'app.json'))),samples=JSON.parse(await readFile(path.join(dir,'samples.json')));
 const api=await instantiate(await readFile(path.join(appDir,'core.wasm')));
 const source=Array.from(JSON.stringify(bundle.source));
 for(const sample of samples){api.reset();for(let i=0;i<source.length;i+=500)assert.equal(api.step({act:'ui_append',chunk:source.slice(i,i+500).join('')}).error,undefined);
  assert.deepEqual(api.step({act:'ui_decide',input:sample.input}),sample.reply,`${name}: actual core native/Wasm parity`);}
 // Preparation refusals are measured by the unchanged ABI request cap. A failed append leaves all earlier bytes intact.
 api.reset();assert.equal(api.step({act:'ui_append',chunk:'x',extra:true}).error,'UiPreparationFields');
 assert.equal(api.step({act:'ui_append',chunk:'{'}).prepared_bytes,1);
 assert.equal(api.step({act:'ui_append',chunk:4}).error,'UiPreparationChunk');
 assert.equal(api.step({act:'ui_append',chunk:'}'}).prepared_bytes,2);
 const resolveAsset=url=>{const u=new URL(url);assert.equal(u.origin,ORIGIN);assert.ok(u.pathname.startsWith('/ZenoFCIS/'));return path.join(appDir,u.pathname.slice('/ZenoFCIS/'.length));};
 const document=documentFrom('<div id="app"></div>'),rootNode=document.getElementById('app');
 const loader=moduleLoader({document,crypto:webcrypto,Worker:localWorkerClass(resolveAsset),innerWidth:390,addEventListener:()=>{},matchMedia:()=>({matches:false,addEventListener:()=>{}})},undefined,resolveAsset);
 const module=await loader.evaluate(`${ORIGIN}/ZenoFCIS/zenoui/app.js`);
 await module.namespace.mount(rootNode,bundle);
 await wait(()=>rootNode.dataset.ready==='true','mounted core ready');
 assert.equal(rootNode.querySelectorAll('[data-region]').length,8,'all initial regions');
 assert.equal(rootNode.querySelectorAll('form').length,bundle.description.commands.length,'form per command');
 for(const sample of samples.filter(s=>s.input.filter((_,i)=>bundle.description.fields[i].source==='state').every((v,i)=>v===bundle.description.fields.filter(f=>f.source==='state')[i].initial)&&s.reply.class==='Accept')){
  const id=sample.input[bundle.description.action],form=rootNode.querySelectorAll('form').find(f=>f.querySelector('button').dataset.command===id);
  for(const field of bundle.description.fields.filter(f=>f.source!=='state'&&f.index!==bundle.description.action)){
   const input=form.querySelector(`#command-${id}-field-${field.index}`);input.value=sample.input[field.index];input.dispatchEvent(new Event('input'));
  }
  await wait(()=>!form.querySelector('button').disabled,'mounted native-accepted command enabled');
  form.dispatchEvent(new Event('submit',{cancelable:true}));
  const confirmation=rootNode.querySelector('[data-region="confirmation"]');assert.ok(confirmation.textContent.includes(bundle.description.commands.find(c=>c.id===id).label));
  assert.ok(confirmation.textContent.includes('Local operator'));assert.ok(confirmation.textContent.includes('Revision: 0'));
  const confirm=confirmation.querySelector('button');assert.match(confirm.textContent,/Confirm/);confirm.click();
  await wait(()=>rootNode.dataset.phase==='done','mounted confirmation dispatched to core');
  await new Promise(r=>setTimeout(r,50));assert.match(rootNode.querySelector('[data-region=\"verdict\"]').textContent,/The core accepts this request/,'later advisory previews preserve the completed decision');
  assert.equal(rootNode.dataset.revision,'1');assert.match(rootNode.querySelector('[data-region="ledger"]').textContent,/Accept/);
  break;
 }
 assert.equal(rootNode.dataset.phase,'done','at least one mounted command completed');
 console.log(`${name}: ${samples.length} native/Wasm decision comparisons; real shared worker, forms, confirmation, state and ledger passed`);
}
