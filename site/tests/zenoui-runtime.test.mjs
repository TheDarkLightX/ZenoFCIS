import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import path from 'node:path';
import {instantiate} from '../public/demo-module.js';
import {validateAst,scopeCommands,scopeInputs} from '../public/zenoui/ui-grammar.js';
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
 const app=bundle.description,requirements=app.presentation.requirements;
 const checked=validateAst(app,requirements,app.presentation.ast,app.searchReceipt.bounds);
 assert.equal(checked.valid,true,'mounted presentation independently checked');
 const scopes=[];const visit=n=>{if(n.kind==='scope')scopes.push(n.scope);for(const c of n.children??[])visit(c);};visit(app.presentation.ast);
 assert.equal(new Set(scopes).size,scopes.length,'unique declared scopes');
 assert.equal(scopes.length,checked.summary.visibleForms,'checked form count');
 assert.equal(rootNode.querySelectorAll('form').length,scopes.length,'one mounted form per checked scope');
 const covered=scopes.flatMap(scope=>scopeCommands(app,requirements,scope));
 assert.deepEqual([...covered].sort(),app.commands.map(c=>c.id).sort(),'all commands covered exactly once');
 const formFor=scope=>{const forms=rootNode.querySelectorAll('form').filter(f=>f.dataset.scope===scope);assert.equal(forms.length,1,'one mounted form for scope '+scope);return forms[0];};
 const reviewFor=(scope,id)=>{const buttons=formFor(scope).querySelectorAll('button').filter(b=>b.type==='submit'&&b.dataset.command===id);assert.equal(buttons.length,1,'one review for selected command');return buttons[0];};
 for(const sample of samples.filter(s=>s.input.filter((_,i)=>bundle.description.fields[i].source==='state').every((v,i)=>v===bundle.description.fields.filter(f=>f.source==='state')[i].initial)&&s.reply.class==='Accept')){
  const id=sample.input[app.action],served=scopes.filter(scope=>scopeCommands(app,requirements,scope).includes(id));
  assert.equal(served.length,1,'one declared scope serves accepted command');const scope=served[0];
  if(formFor(scope).dataset.command!==id){
   const selector=formFor(scope).querySelector('#command-direction');assert.ok(selector,'explicit grouped command selector');
   selector.value=id;selector.dispatchEvent(new Event('change'));
  }
  // Select may repaint synchronously. Reacquire every subsequent form/control from its scope.
  await wait(()=>formFor(scope).dataset.command===id,'selected accepted command');
  const wanted=scopeInputs(app,requirements,scope);assert.ok(wanted,'checked scope input bindings');
  for(const field of app.fields.filter(f=>f.source==='context'||wanted.includes(f.path))){
   const input=field.source==='context'?rootNode.querySelector(`#context-field-${field.index}`):formFor(scope).querySelector(`#command-${scope}-field-${field.index}`);
   assert.ok(input,`editable local input ${field.path}`);input.value=sample.input[field.index];input.dispatchEvent(new Event('input'));
  }
  await wait(()=>!reviewFor(scope,id).disabled,'mounted native-accepted command enabled');
  formFor(scope).dispatchEvent(new Event('submit',{cancelable:true}));
  const confirmation=rootNode.querySelector('[data-region="confirmation"]');assert.ok(confirmation.textContent.includes(bundle.description.commands.find(c=>c.id===id).label));
  assert.ok(confirmation.textContent.includes('Local operator'));assert.match(confirmation.textContent,/revision 0/i);
  const confirm=confirmation.querySelector('button');assert.match(confirm.textContent,/Confirm/);confirm.click();
  await wait(()=>rootNode.dataset.phase==='done','mounted confirmation dispatched to core');
  await new Promise(r=>setTimeout(r,50));assert.match(rootNode.querySelector('[data-region=\"verdict\"]').textContent,/The core accepts this request/,'later advisory previews preserve the completed decision');
  assert.equal(rootNode.dataset.revision,'1');assert.match(rootNode.querySelector('[data-region="ledger"]').textContent,/Accepted/);
  break;
 }
 assert.equal(rootNode.dataset.phase,'done','at least one mounted command completed');
 console.log(`${name}: ${samples.length} native/Wasm decision comparisons; real shared worker, forms, confirmation, state and ledger passed`);
}
