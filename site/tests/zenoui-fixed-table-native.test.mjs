// Run only against a newly captured, source-bound native factory packet.
// Missing capture refuses; no synthetic fallback or skipped qualification.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {describe,canonical,checkPresentation,verifyPresentation} from '../public/zenoui/app-contract.js';
const capture=process.env.ZENO_FIXED_TABLE_NATIVE_CAPTURE;
test('real authored library native capture admits the bound table-list profile',async()=>{
 assert.ok(capture,'Native capture is required; obtain it in an approved native window.');
 const read=async name=>JSON.parse(await readFile(path.join(capture,name),'utf8'));
 const raw=await read('evidence/raw.json'),meta=await read('evidence/metadata.json'),finite=await read('evidence/finite.json'),samples=await read('evidence/samples.json'),bundle=await read('app/app.json');
 for(const [name,key] of [['project.zeno','project'],['v2/policy.json','policy']]){
  const expected=await readFile(new URL(`../../examples/contract-tables/library-limits/${name}`,import.meta.url),'utf8');
  assert.equal(raw.source[key],expected,'capture must name the exact independently supplied authored source');
 }
 const expectedMeta=JSON.parse(await readFile(new URL('../../examples/contract-tables/library-limits/ui.json',import.meta.url),'utf8'));
 assert.equal(canonical(meta),canonical(expectedMeta));
 const app=describe(raw.descriptor,meta);
 assert.equal(app.format,'zenoui/app-description/3');assert.equal(app.tables.length,1);assert.equal(app.tables[0].entries.length,6);
 assert.equal(canonical(bundle.description),canonical(app));assert.equal(canonical(bundle.source),canonical(raw.source));
 assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);
 assert.equal(finite.domain,'589824');assert.equal(finite.classes.length,589824);
 assert.ok(samples.some(s=>s.reply.class==='Accept'));assert.ok(samples.some(s=>s.reply.class==='Reject'));
 // Command/source/checker/binary provenance and actual L1-L8/runtime receipts
 // remain separate required evidence from the real existing factory route.
});
