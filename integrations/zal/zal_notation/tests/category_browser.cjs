'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
const api=require('../lens_assets/app.js');
const packet=JSON.parse(fs.readFileSync(process.env.ZAL_CATEGORY_PACKET,'utf8'));
const views=packet.subjects.flatMap(s=>s.lenses.filter(v=>v.category));
const view=(op,status='CheckedAt')=>views.find(v=>v.category.application.operation===op&&v.category.record.status===status);
const clone=x=>JSON.parse(JSON.stringify(x));
test('actual category packet binds eleven records and all six operation routes',()=>{
  assert.equal(api.validatePacket(packet),packet);assert.equal(views.length,11);
  assert.equal(new Set(views.map(v=>v.category.application.operation)).size,6);
  assert.deepEqual(new Set(views.map(v=>v.category.record.status)),new Set(['CheckedAt','RefutedAt','Authored','UNKNOWN']));
});
test('typed composition and two-sided inverse query actual recorded tables',()=>{
  const path=view('compose-path'),ops=path.operations;
  assert.equal(api.recordedCategoryComposition(ops,'01','12'),'02');
  assert.equal(api.recordedCategoryComposition(ops,...ops.selection.path),ops.result.composite);
  assert.throws(()=>api.recordedCategoryComposition(ops,'12','01'),/Incompatible typed/);
  assert.equal(api.recordedCategoryInverse(ops,'01'),null);
  const inverse=view('invertible').operations;
  assert.equal(api.recordedCategoryInverse(inverse,inverse.selection.arrow),inverse.result.inverse);
});
test('recorded functor composition exposes actual many-arrow fibers',()=>{
  const ops=view('compose-functors').operations,first=ops.result.functor.arrows[0][0];
  const result=api.recordedFunctorFiber(ops,first);
  assert.equal(result.target,ops.result.functor.arrows[0][1]);
  assert.ok(result.preimage.length>1);assert.ok(result.preimage.includes(first));
  assert.throws(()=>api.recordedFunctorFiber(ops,'absent'),/Unknown source/);
});
test('symbolic hops stay authored with exact assumptions and cumulative losses',()=>{
  const lens=view('compose-symbolic','Authored'),ops=lens.operations;
  assert.equal(api.statusLabel(lens),'Authored symbolic composition');
  assert.equal(api.recordedSymbolicHop(ops,0).id,ops.result.path[0]);
  assert.deepEqual(ops.result.cumulative_forgets,['word spelling','non-symmetric transitions']);
  assert.throws(()=>api.recordedSymbolicHop(ops,true),/No authored/);
  assert.equal(lens.category.application.interpretation.status,'Authored');
});
test('refutation and incomplete provenance never certify operations or completion',()=>{
  const refuted=views.find(v=>v.category.record.status==='RefutedAt'),incomplete=views.find(v=>v.category.record.status==='UNKNOWN');
  assert.equal(refuted.status,'refuted');assert.equal(incomplete.status,'incomplete');
  assert.equal(refuted.operations,null);assert.equal(incomplete.operations,null);
  assert.equal(incomplete.category.audit.status,'RecordedIncomplete');
  assert.match(incomplete.category.audit.static_table_audit,/not performed/);
  assert.match(refuted.category.record.refutation_scope,/unselected declarations/);
});
test('category source/approval/catalog/worker/status/operations substitutions refuse',()=>{
  const mutations=[v=>v.status='authored',v=>v.category.application.context_digest='different',
    v=>v.category.approvalDigest='different',v=>v.category.catalogDigest='different',
    v=>v.category.record.native_receipt.worker.binary_sha256='different',
    v=>v.operations.result.composite='different'];
  for(const mutate of mutations){const p=clone(packet),v=p.subjects.flatMap(s=>s.lenses).find(v=>v.category&&v.category.application.operation==='compose-path'&&v.status==='checked');mutate(v);assert.throws(()=>api.validatePacket(p));}
});
test('incomplete result fabrication and exact numeric aliases refuse in controller',()=>{
  for(const mutate of [n=>n.result={claim:'manufactured'},n=>n.work_used=true,n=>n.message='all laws proved']){
    const p=clone(packet),v=p.subjects.flatMap(s=>s.lenses).find(v=>v.category?.record.status==='UNKNOWN');mutate(v.category.record.native_receipt);assert.throws(()=>api.validatePacket(p));
  }
  const p=clone(packet),v=p.subjects.flatMap(s=>s.lenses).find(v=>v.category?.application.operation==='check-category'&&v.status==='checked');v.category.record.native_receipt.result.objects=true;assert.throws(()=>api.validatePacket(p),/exact integers/);
});
