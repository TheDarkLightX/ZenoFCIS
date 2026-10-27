import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {manifestFor,loadAssets,ORIGIN,digest} from './asset-manifest.mjs';
test('hashed_asset_snapshot_refuses_unknown_external_changed_and_escaping_inputs',async()=>{
 const dir=await mkdtemp(path.join(tmpdir(),'zenoui-assets-'));
 try {
  await mkdir(path.join(dir,'module'));await writeFile(path.join(dir,'index.html'),'<p>ready</p>');await writeFile(path.join(dir,'module/worker.js'),'postMessage("ready")');
  const manifest=await manifestFor(dir),assets=await loadAssets(dir,manifest);
  assert.equal(assets.size,2);assert.equal(assets.get(`${ORIGIN}/ZenoFCIS/index.html`).body.toString(),'<p>ready</p>');
  assert.equal(assets.has(`${ORIGIN}/unknown`),false);assert.equal(assets.has('https://external.invalid/index.html'),false);
  const bad=structuredClone(manifest);bad.entries[0].sha256='0'.repeat(64);await assert.rejects(loadAssets(dir,bad),/Invalid asset manifest/);
  bad.sha256=digest(JSON.stringify(bad.entries));await assert.rejects(loadAssets(dir,bad),/Asset hash mismatch/);
  const escaping=structuredClone(manifest);escaping.entries[0].file='../private';escaping.sha256=digest(JSON.stringify(escaping.entries));await assert.rejects(loadAssets(dir,escaping),/Invalid asset route/);
  await writeFile(path.join(dir,'index.html'),'changed');await assert.rejects(loadAssets(dir,manifest),/Asset hash mismatch/);
  assert.equal(assets.get(`${ORIGIN}/ZenoFCIS/index.html`).body.toString(),'<p>ready</p>','loaded snapshot does not reopen changed files');
 }finally{await rm(dir,{recursive:true,force:true});}
});
