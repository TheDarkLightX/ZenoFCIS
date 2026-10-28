import assert from 'node:assert/strict';
import {test} from 'node:test';
import {buildFragmentCache,createUiSearchSession,searchUi} from '../public/zenoui/ui-search.js';
import {validateAst} from '../public/zenoui/ui-grammar.js';
const canonical=v=>JSON.stringify(v,(_k,x)=>x&&typeof x==='object'&&!Array.isArray(x)?Object.fromEntries(Object.keys(x).sort().map(k=>[k,x[k]])):x);
function fixture(fields=1){
  const app={fields:Array.from({length:fields},(_,i)=>({path:`command.field${i}`,source:'command',index:i+1})),action:0,commands:[{id:'1'},{id:'2'}]};
  const requirements={primaryFields:fields?['command.field0']:[],detailFields:app.fields.slice(1).map(f=>f.path),projections:[],maxVisibleForms:2,maxExpandedExpertPanels:5,maxColumns:2,groupCommands:true,collapseExpert:true,compact:true};
  return {app,requirements};
}
test('recursive search and checked fragments choose identical expanded trees across prefixes',()=>{
  for(const fields of [0,1,3])for(const maxCandidates of [1,32,256]){
    const {app,requirements}=fixture(fields),before=canonical({app,requirements});
    const direct=searchUi(app,requirements,{mode:'direct',maxCandidates}),fragments=searchUi(app,requirements,{mode:'fragments',maxCandidates});
    assert.deepEqual(fragments.ast,direct.ast);assert.deepEqual(fragments.receipt.selectedScore,direct.receipt.selectedScore);
    assert.equal(fragments.receipt.examined,direct.receipt.examined);assert.equal(fragments.receipt.feasible,direct.receipt.feasible);
    assert.equal(canonical({app,requirements}),before);
    assert.ok(validateAst(app,requirements,fragments.ast).valid);
  }
});
test('fragment bank is structural, deterministic, exact-canonical deduplicated and immutable',()=>{
  const a=buildFragmentCache(),b=buildFragmentCache();assert.deepEqual(a,b);assert.ok(a.receipt.complete);
  assert.equal(a.cache.entries.length,new Set(a.cache.entries.map(canonical)).size);
  assert.equal(a.cache.storageBytes,new TextEncoder().encode(canonical(a.cache.entries)).length);
  assert.ok(a.cache.entries.some(e=>e.tree.kind==='card'));
  assert.ok(!canonical(a.cache).includes('command.field'));
  assert.throws(()=>a.cache.entries.push({}),TypeError);
  assert.throws(()=>a.cache.entries[0].tree.kind='input',TypeError);
});
test('cache summaries and content never confer acceptance authority',()=>{
  const {app,requirements}=fixture(),cache=buildFragmentCache().cache;
  const bad=structuredClone(cache);bad.entries[0].tree={kind:'input',scope:'selected',field:'evil'};
  assert.throws(()=>searchUi(app,requirements,{cache:bad,maxCandidates:1}),/UiFragmentCacheMismatch/);
  const summary=structuredClone(cache);summary.storageBytes=0;
  assert.throws(()=>searchUi(app,requirements,{cache:summary,maxCandidates:1}),/UiFragmentCacheMismatch/);
  const good=searchUi(app,requirements,{cache,maxCandidates:1});assert.ok(good.receipt.work.cacheVerification>0);
  const tight=searchUi(app,{...requirements,maxVisibleForms:1,maxExpandedExpertPanels:0,maxColumns:1},{maxCandidates:128});
  assert.ok(tight.receipt.feasible<tight.receipt.examined);assert.ok(validateAst(app,{...requirements,maxVisibleForms:1,maxExpandedExpertPanels:0,maxColumns:1},tight.ast).valid);
});
test('prepared immutable sessions charge cold storage and disclose amortized scenario',()=>{
  const {app,requirements}=fixture(),session=createUiSearchSession({maxCandidates:256});
  const warm=session.search(app,requirements,{amortizedQueries:10}),cold=searchUi(app,requirements,{maxCandidates:256});
  assert.deepEqual(warm.ast,cold.ast);assert.equal(warm.receipt.work.preparation,0);
  assert.equal(warm.receipt.amortization.totalWork,session.receipt.work+10*warm.receipt.work.total);
  assert.equal(warm.receipt.amortization.scenario,'same-query-repeated');
  assert.equal(cold.receipt.work.total,session.receipt.work+warm.receipt.work.total);
  assert.deepEqual(session.search(app,requirements),session.search(app,requirements));
  assert.throws(()=>session.search(app,requirements,{maxDepth:1}),/UiSessionBoundChange/);
  assert.throws(()=>warm.ast.children.pop(),TypeError);
});
test('work, node, depth, cache and candidate bounds have explicit refusal accounting',()=>{
  const {app,requirements}=fixture();
  const one=searchUi(app,requirements,{mode:'direct',maxCandidates:1});
  assert.equal(one.receipt.reason,'candidate-budget');assert.equal(one.receipt.examined,1);
  const exact=searchUi(app,requirements,{mode:'direct',maxCandidates:1,maxWork:one.receipt.work.total});assert.deepEqual(exact.ast,one.ast);
  const short=searchUi(app,requirements,{mode:'direct',maxCandidates:1,maxWork:one.receipt.work.total-1});assert.equal(short.receipt.reason,'work-budget');assert.ok(short.receipt.work.total<=one.receipt.work.total-1);
  for(const options of [{maxNodes:1},{maxDepth:1}])assert.equal(searchUi(app,requirements,{mode:'direct',maxCandidates:16,...options}).ast,null);
  for(const options of [{maxCacheEntries:1},{maxCacheBytes:1},{maxWork:1}]){const r=buildFragmentCache(options);assert.equal(r.cache,null);assert.equal(r.receipt.complete,false);assert.ok(r.receipt.work<= (options.maxWork??2000000));}
  for(const options of [{maxCandidates:0},{fragmentSize:5},{maxWork:Infinity},{maxDepth:NaN}])assert.throws(()=>searchUi(app,requirements,options),/UiSearchLimit/);
});
test('semantic identities are never renamed or collapsed',()=>{
  const {app,requirements}=fixture(),a=searchUi(app,requirements,{mode:'direct',maxCandidates:64});
  const renamed=structuredClone(app);renamed.fields[0].path='command.other';
  const b=searchUi(renamed,{...requirements,primaryFields:['command.other']},{mode:'direct',maxCandidates:64});
  assert.notEqual(canonical(a.ast),canonical(b.ast));assert.ok(canonical(b.ast).includes('command.other'));
});
test('completed small grammar and larger bounded prefixes have identical full acceptance traces',()=>{
  const {app,requirements}=fixture(0);app.commands=[{id:'1'}];
  const direct=searchUi(app,requirements,{mode:'direct',maxCandidates:8192}),cached=searchUi(app,requirements,{mode:'fragments',maxCandidates:8192});
  assert.equal(direct.receipt.complete,true);assert.equal(cached.receipt.complete,true);
  assert.equal(direct.receipt.examined,56);assert.deepEqual(direct.receipt.candidateTrace,cached.receipt.candidateTrace);
  const larger=fixture(4);
  const a=searchUi(larger.app,larger.requirements,{mode:'direct',maxCandidates:512}),b=searchUi(larger.app,larger.requirements,{mode:'fragments',maxCandidates:512});
  assert.deepEqual(a.receipt.candidateTrace,b.receipt.candidateTrace);
  assert.deepEqual(a.receipt.selected,b.receipt.selected);
});
test('untrusted cache cycles, getters, excessive depth and primitive types are refused before canonical recursion',()=>{
  const {app,requirements}=fixture(),bad={};bad.self=bad;
  const getter={get entries(){throw Error('getter executed');}};
  let deep={};for(let i=0;i<40;i++)deep={child:deep};
  for(const cache of [bad,getter,deep,{entries:()=>{}}])assert.throws(()=>searchUi(app,requirements,{cache,maxCandidates:1}),/UiFragmentCacheMismatch/);
});
