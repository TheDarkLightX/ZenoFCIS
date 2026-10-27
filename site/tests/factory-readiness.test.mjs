import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';
import {ORIGIN,documentFrom,moduleLoader,localWorkerClass} from './browser-fixture.mjs';
const native=JSON.parse(await readFile(new URL('./harness/factory-native.json',import.meta.url)));
const page=await readFile(new URL('../public/index.html',import.meta.url),'utf8');
async function mount(transform){
  const document=documentFrom(page);
  assert.ok(document.querySelector('script[src="factory.js"]'),'test exercises the actual page entry module');
  const media=()=>({matches:false,addEventListener(){},removeEventListener(){}});
  const loader=moduleLoader({document,matchMedia:media,innerWidth:1440,Worker:localWorkerClass()},transform);
  const module=await loader.evaluate(`${ORIGIN}/ZenoFCIS/factory.js`);
  return {document,module,root:document.getElementById('factory')};
}
function completion(root,act,timeout=1000){
  return new Promise((resolve,reject)=>{
    const done=event=>{clearTimeout(timer);resolve(event.detail);};
    const timer=setTimeout(()=>{root.removeEventListener('factory-result',done);reject(new Error('Missing real page factory-result signal'));},timeout);
    root.addEventListener('factory-result',done,{once:true});root.querySelector(`button[data-act="${act}"]`).click();
  });
}
test('real_page_mount_click_worker_wasm_render_emits_completion',async()=>{
  const {root}=await mount();
  assert.equal(root.querySelectorAll('[data-region]').length,16,'initial page regions rendered');
  assert.equal(root.querySelector('[data-factory-act="search"] .factory-output').dataset.complete,undefined,'not ready before a result');
  for(const act of ['search','synthesis','money']){
    const detail=await completion(root,act);
    assert.deepEqual(JSON.parse(JSON.stringify(detail.result)),native[act],'actual worker / Wasm / native result');
    assert.equal(root.querySelector(`[data-factory-act="${act}"] .factory-output`).dataset.complete,'true','event follows completed rendering');
    assert.deepEqual(JSON.parse(JSON.stringify(detail.identity)),{epoch:'0',run:String(['search','synthesis','money'].indexOf(act)+1),request:String(['search','synthesis','money'].indexOf(act)+1)});
  }
});
test('readiness_check_refuses_a_real_page_with_the_completion_signal_removed',async()=>{
  const {root}=await mount((url,source)=>url.endsWith('/factory.js')?source.replace("CustomEvent('factory-result'","CustomEvent('lost-factory-result'"):source);
  await assert.rejects(completion(root,'search',200),/Missing real page factory-result signal/);
  assert.equal(root.querySelector('[data-factory-act="search"] .factory-output').dataset.complete,'true','planted defect loses the signal after actual rendering');
});
test('factory_harness_modules_resolve_only_within_the_actual_served_routes',async()=>{
  const loader=moduleLoader({});
  await loader.load(`${ORIGIN}/harness/factory.js`);
  await loader.load(`${ORIGIN}/ZenoFCIS/factory-layout.js`);
  await assert.rejects(loader.load(`${ORIGIN}/public/factory-layout.js`),/Unserved asset/,'old filesystem-relative import is not a served route');
});
test('actual_harness_resolves_deployed_geometry_before_waiting_for_the_page',async()=>{
  const document=documentFrom(await readFile(new URL('./harness/factory.html',import.meta.url),'utf8'));
  let fetched=false;
  const loader=moduleLoader({document,location:{href:`${ORIGIN}/harness/factory.html?base=/ZenoFCIS/`,search:'?base=/ZenoFCIS/'},
    fetch:async()=>{fetched=true;throw new Error('fixture stop after deployed geometry import');}});
  await loader.evaluate(`${ORIGIN}/harness/factory.js`);
  assert.equal(fetched,true,'real harness passes its geometry import');
  const result=JSON.parse(document.getElementById('results').textContent);
  assert.deepEqual(result.errors,['fixture stop after deployed geometry import']);
});
test('real_factory_harness_bootstrap_emits_failed_readiness_on_import_error',async()=>{
  const html=await readFile(new URL('./harness/factory.html',import.meta.url),'utf8');
  const document=documentFrom(html),script=html.match(/<script type="module">([\s\S]*?)<\/script>/)[1];
  const loader=moduleLoader({document});
  const bootstrap=new (await import('node:vm')).SourceTextModule(script,{context:loader.context,
    importModuleDynamically:async()=>{throw new Error('planted unavailable harness module');}});
  await bootstrap.link(()=>{throw new Error('unexpected static import');});await bootstrap.evaluate();await delay(0);
  const result=JSON.parse(document.getElementById('results').textContent);
  assert.deepEqual(result.results,{});assert.match(result.errors[0],/planted unavailable harness module/);
});
