import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm,chmod} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import {manifestFor,ORIGIN} from './asset-manifest.mjs';
const here=fileURLToPath(new URL('.',import.meta.url));
async function run(corrupt=false,workerFetch=false){
  const dir=await mkdtemp(path.join(tmpdir(),'zenoui-pipe-test-'));
  try{
    const manifestPath=path.join(dir,'assets.json'),reportPath=path.join(dir,'browser.json');
    const root=path.resolve(here,'../public'),manifest=await manifestFor(root);
    await writeFile(manifestPath,JSON.stringify(manifest));
    const peer=path.join(dir,'peer');await writeFile(peer,await readFile(path.join(here,'test-data/devtools-peer.py')));await chmod(peer,0o700);
    let runner=path.join(here,'pipe-browser.mjs');
    if(workerFetch){
      runner=path.join(dir,'runner.mjs');
      let source=await readFile(path.join(here,'pipe-browser.mjs'),'utf8');
      source=source.replace("if(p.targetInfo.type==='page'){","if(true){");
      source=source.replace("'./browser-checks.mjs'",JSON.stringify(new URL('./browser-checks.mjs',import.meta.url).href));
      source=source.replace("'./asset-manifest.mjs'",JSON.stringify(new URL('./asset-manifest.mjs',import.meta.url).href));
      source=source.replace("new URL('./exec_chrome.py',import.meta.url)",JSON.stringify(new URL('./exec_chrome.py',import.meta.url).href));
      await writeFile(runner,source);
    }
    const child=spawn(process.execPath,[runner,peer,path.join(dir,'profile'),root,manifestPath,path.join(here,'harness/factory-native.json'),reportPath],{
      env:{...process.env,ZENOUI_TEST_NATIVE:path.join(here,'harness/factory-native.json'),ZENOUI_TEST_CORRUPT_RESULT:corrupt?'1':''},stdio:['ignore','pipe','pipe']});
    let output='';for(const pipe of [child.stdout,child.stderr])pipe.on('data',data=>output+=data);
    const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('close',resolve);});
    return {code,output,report:JSON.parse(await readFile(reportPath,'utf8')),manifest};
  }finally{await rm(dir,{recursive:true,force:true});}
}
test('actual_pipe_runner_uses_page_fetch_resumes_flattened_worker_and_refuses_routes',async()=>{
  const {code,output,report,manifest}=await run();assert.equal(code,0,output);
  assert.equal(report.parity,true);assert.equal(report.workerAttached,1);
  for(const asset of report.fulfilled)assert.equal(asset.sha256,manifest.entries.find(e=>e.url===asset.url).sha256);
  assert.ok(report.fulfilled.some(e=>e.url.endsWith('/factory.wasm')));
  assert.deepEqual(report.denied,[`${ORIGIN}/unknown-asset`,'https://external.invalid/no']);
});
test('full_result_parity_catches_a_nested_witness_mismatch',async()=>{
  const {code,report}=await run(true);assert.equal(code,1);assert.equal(report.parity,false);
  assert.match(report.error,/browser worker \/ Wasm \/ native parity/);
  assert.equal(report.failure.name,'AssertionError');
  assert.equal(report.failure.operator,'deepStrictEqual');
  assert.equal(report.failure.actual.first_difference.ordinal,report.failure.expected.first_difference.ordinal+1);
});
test('planted_worker_fetch_domain_call_is_rejected',async()=>{
  const {code,report}=await run(false,true);assert.equal(code,1);assert.equal(report.parity,false);
  assert.match(report.error,/'Fetch.enable' wasn't found/);
});
