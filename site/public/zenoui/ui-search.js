import {validateAst, scoreAst, CHECKER, scopeInputs, scopeCommands, applies} from './ui-grammar.js';
import {displayPlan, swapFits, editFits, approvalFits} from './ui-idioms.js';

// This is an ordered, bounded subgrammar, not an enumeration of arbitrary UIs.
// Scope/authority boundaries are fixed. Within each obligation list, productions
// are a flat stack or every ordered binary partition under stack, card or grid.
// Exact structure is the ONLY quotient: semantic IDs and child order survive.
export const SEARCH_GRAMMAR = 'zenoui/ordered-interval-ui/3';
export const FRAGMENT_FORMAT = 'zenoui/structural-fragments/1';
const constructors = ['stack','card','grid'];
const evidence = ['state','metrics','comparison','ledger','record'];
const canonical = value => JSON.stringify(value, (_k,v) => v && typeof v === 'object' && !Array.isArray(v)
  ? Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])) : v);
const freeze = value => { if(value && typeof value==='object'){Object.values(value).forEach(freeze);Object.freeze(value);}return value; };
const copy = value => JSON.parse(JSON.stringify(value));
const size = value => 1 + (value.children || []).reduce((n,v)=>n+size(v),0);
const utf8 = text => new TextEncoder().encode(text).length;
const defaults = {maxCandidates:2048,maxNodes:128,maxDepth:10,maxWork:2000000,fragmentSize:4,maxCacheEntries:4096,maxCacheBytes:1048576};
const maxima = {maxCandidates:8192,maxNodes:512,maxDepth:16,maxWork:20000000,fragmentSize:4,maxCacheEntries:16384,maxCacheBytes:4194304};
function limits(options={}) {
  const out={};
  for(const [key,value] of Object.entries(defaults)){
    out[key]=options[key] ?? value;
    if(!Number.isSafeInteger(out[key]) || out[key]<1 || out[key]>maxima[key])throw Error(`UiSearchLimit:${key}`);
  }
  return out;
}
class Exhausted extends Error {}
function meter(bound) {
  return {used:0,categories:{},take(kind,n=1){
    if(this.used+n>bound)throw new Exhausted('work-budget');
    this.used+=n;this.categories[kind]=(this.categories[kind]||0)+n;
  }};
}
// Templates contain only locally indexed holes. No field, command, effect,
// authority, DOM handler, or semantic identity is synthesized in a fragment.
function* templates(n, offset, work) {
  if(n===1){work.take('templateNodes');yield {kind:'hole',index:offset};work.take('templateNodes',2);yield {kind:'card',children:[{kind:'hole',index:offset}]};return;}
  work.take('templateNodes',n+1);
  yield {kind:'stack',children:Array.from({length:n},(_,i)=>({kind:'hole',index:offset+i}))};
  for(let split=1;split<n;split++)for(const left of templates(split,offset,work))for(const right of templates(n-split,offset+split,work))for(const kind of constructors){
    work.take('templateNodes');yield {kind,children:[left,right]};
  }
}
function cacheIdentity(bound){return {grammar:SEARCH_GRAMMAR,checker:CHECKER,fragmentSize:bound.fragmentSize,maxNodes:bound.maxNodes,maxDepth:bound.maxDepth};}
function prepare(bound,work) {
  const entries=[];let bytes=0;
  for(let holes=1;holes<=bound.fragmentSize;holes++){
    const seen=new Set();
    for(const tree of templates(holes,0,work)){
      const key=canonical(tree);work.take('canonicalNodes',size(tree));
      if(seen.has(key))continue;
      seen.add(key);const entry={holes,tree};const length=utf8(canonical(entry));
      if(entries.length===bound.maxCacheEntries || bytes+length>bound.maxCacheBytes)throw new Exhausted('cache-budget');
      bytes+=length;entries.push(entry);work.peakCacheBytes=bytes;work.cacheEntries=entries.length;
    }
  }
  return freeze({format:FRAGMENT_FORMAT,identity:cacheIdentity(bound),entries,storageBytes:utf8(canonical(entries))});
}
export function buildFragmentCache(options={}) {
  const bound=limits(options),work=meter(bound.maxWork);
  try {const cache=prepare(bound,work);if(cache.storageBytes>bound.maxCacheBytes)throw new Exhausted('cache-budget');return freeze({cache,receipt:{complete:true,work:work.used,categories:work.categories,storageBytes:cache.storageBytes,entries:cache.entries.length}});}
  catch(error){if(!(error instanceof Exhausted))throw error;return freeze({cache:null,receipt:{complete:false,reason:error.message,work:work.used,categories:work.categories,storageBytes:0,entries:0,peakPreparationBytes:work.peakCacheBytes??0,preparedEntries:work.cacheEntries??0}});}
}
function bind(tree, leaves, work,offset=0) {
  work.take('bindingNodes');
  if(tree.kind==='hole')return leaves[tree.index-offset];
  return {kind:tree.kind,children:tree.children.map(child=>bind(child,leaves,work,offset))};
}
function* shapes(leaves, mode, bank, work) {
  if(!leaves.length){work.take('bindingNodes');yield {kind:'stack',children:[]};return;}
  if(mode==='fragments' && leaves.length<=bank.identity.fragmentSize){
    for(const entry of bank.entries)if(entry.holes===leaves.length){work.take('fragmentLookups');yield bind(entry.tree,leaves,work);}return;
  }
  // Recursing across a larger interval uses exactly the same productions as
  // direct mode; only small structural subtree construction is replaced.
  if(mode==='direct' && leaves.length<=bank.identity.fragmentSize){
    const seen=new Set();
    for(const tree of templates(leaves.length,0,work)){const key=canonical(tree);work.take('canonicalNodes',size(tree));if(seen.has(key))continue;seen.add(key);yield bind(tree,leaves,work);}return;
  }
  work.take('bindingNodes');yield {kind:'stack',children:leaves};
  for(let split=1;split<leaves.length;split++)for(const left of shapes(leaves.slice(0,split),mode,bank,work))for(const right of shapes(leaves.slice(split),mode,bank,work))for(const kind of constructors){work.take('bindingNodes');yield {kind,children:[left,right]};}
}
// A scope's leaves in protocol order: the command selector, primary inputs, projections, details,
// then review. Inputs are exactly the scope's relevant ones (scopeInputs); projections apply only to
// the commands they are declared for.
function obligations(app,req,scope,details) {
  const input=field=>({kind:'input',scope,field});
  const commands=scopeCommands(app,req,scope)??[],wanted=scopeInputs(app,req,scope)??[...req.primaryFields,...req.detailFields];
  const primary=req.primaryFields.filter(p=>wanted.includes(p)),detail=req.detailFields.filter(p=>wanted.includes(p));
  const children=[...(scope==='selected'||commands.length>1?[{kind:'selector',scope}]:[]),...primary.map(input),...req.projections.filter(p=>applies(p,commands)).map(p=>({kind:'projection',scope,projection:p.id}))];
  if(detail.length)children.push(...(details?[{kind:'disclosure',label:'Details',children:detail.map(input)}]:detail.map(input)));
  children.push({kind:'review',scope});return children;
}
// The swap card is one alternative production for a scope that fits its signature; generic
// shapes remain alternatives, and the objective ranks idiom fit first.
function* scopeShapes(app,req,scope,details,mode,bank,work) {
  const commands=scopeCommands(app,req,scope)??[],wanted=scopeInputs(app,req,scope);
  if(req.format==='zenoui/requirements/2'&&wanted&&editFits(app,req,commands,wanted,scope)){
    const stateField=app.fields.find(f=>f.path===wanted[0]).stateField;
    work.take('bindingNodes',2);yield {kind:'idiom',idiom:'scalar-edit',scope,children:[{kind:'display',idiom:'facts',fields:[stateField]},...obligations(app,req,scope,false)]};
  }
  if(req.format==='zenoui/requirements/2'&&wanted&&approvalFits(app,req,commands,wanted)){
    const status=app.fields.find(f=>f.source==='state'&&f.role==='approval-status').path;
    work.take('bindingNodes',2);yield {kind:'idiom',idiom:'approval-decision',scope,children:[{kind:'display',idiom:'facts',fields:[status]},...obligations(app,req,scope,false)]};
  }
  if(req.format==='zenoui/requirements/2'&&wanted&&swapFits(app,req,commands,wanted)){
    // Swap card order: what you pay, the direction switch, what you receive, more options, the button.
    const leaves=obligations(app,req,scope,true),selector=leaves.filter(l=>l.kind==='selector');
    const rest=leaves.filter(l=>l.kind!=='selector'),amount=rest.findIndex(l=>l.kind==='input');
    work.take('bindingNodes');yield {kind:'idiom',idiom:'swap-card',scope,children:[...rest.slice(0,amount+1),...selector,...rest.slice(amount+1)]};
  }
  yield* shapes(obligations(app,req,scope,details),mode,bank,work);
}
function* scopeProducts(app,req,scopes,details,mode,bank,work,index=0,prior=[]) {
  if(index===scopes.length){yield prior;return;}
  const scope=scopes[index];
  for(const tree of scopeShapes(app,req,scope,details,mode,bank,work)){
    work.take('bindingNodes');
    yield* scopeProducts(app,req,scopes,details,mode,bank,work,index+1,[...prior,{kind:'scope',scope,children:[tree]}]);
  }
}
function scopesFor(app,req,grouped) {
  if(!grouped)return app.commands.map(c=>c.id);
  if(req.format!=='zenoui/requirements/2'||!req.groups.length)return ['selected'];
  const grouped_=new Set(req.groups.flatMap(g=>g.commands));
  // Each group takes the place of its first command; ungrouped commands keep their own forms.
  return app.commands.flatMap(c=>{const g=req.groups.find(g=>g.commands[0]===c.id);return g?[`group-${g.id}`]:grouped_.has(c.id)?[]:[c.id];});
}
function* candidates(app,req,grouped,details,collapsed,first,reverseProjections,generic,mode,bank,work){
  const scopes=scopesFor(app,req,grouped),v2=req.format==='zenoui/requirements/2';
  const ordered={...req,projections:reverseProjections?[...req.projections].reverse():req.projections};
  for(const forms of scopeProducts(app,ordered,scopes,details,mode,bank,work)){
    // State observations moved into an idiom remain counted exactly once.
    const moved=new Set();
    const collect=node=>{if(node.kind==='idiom'&&['scalar-edit','approval-decision'].includes(node.idiom))moved.add(node.children[0].fields[0]);for(const child of node.children??[])collect(child);};
    forms.forEach(collect);
    const visible={...app,fields:app.fields.filter(f=>!moved.has(f.path))};
    const state=visible.fields.filter(f=>f.source==='state').map(f=>f.path);
    const plan=!v2||!state.length?[]:generic?[{kind:'display',idiom:'facts',fields:state}]:displayPlan(visible);
    const displays=plan.length>1?[{kind:'card',children:plan}]:plan;
    const panels=evidence.map(region=>({kind:'evidence',region}));
    const panel={kind:collapsed?'disclosure':'stack',...(collapsed?{label:'Evidence'}:{}),children:panels};
    const commands={kind:'commands',children:forms};
    const children=v2
      ?(first?[{kind:'acting'},...displays,commands,{kind:'confirmation'},{kind:'status'},panel]:[{kind:'acting'},commands,{kind:'confirmation'},{kind:'status'},...displays,panel])
      :(first?[commands,{kind:'confirmation'},{kind:'status'},panel]:[panel,commands,{kind:'confirmation'},{kind:'status'}]);
    work.take('bindingNodes',10);yield {kind:'page',children};
  }
}
// A compact replay aid, NOT a cryptographic identity or authenticity check.
export function structuralDigest(text){let hash=2166136261;for(let i=0;i<text.length;i++)hash=Math.imul(hash^text.charCodeAt(i),16777619);return (hash>>>0).toString(16).padStart(8,'0');}
function inspectUntrustedCache(value,bound,work){
  const pending=[{value,depth:0}],seen=new Set();let bytes=0,nodes=0;
  while(pending.length){
    const entry=pending.pop(),v=entry.value;if(entry.exit){seen.delete(v);continue;}work.take('cacheInspectionNodes');nodes++;
    if(nodes>bound.maxCacheEntries*32||entry.depth>32)throw Error('UiFragmentCacheMismatch');
    if(v && typeof v==='object'){
      if(seen.has(v)||(!Array.isArray(v)&&Object.getPrototypeOf(v)!==Object.prototype))throw Error('UiFragmentCacheMismatch');seen.add(v);pending.push({value:v,exit:true});
      const descriptors=Object.getOwnPropertyDescriptors(v);
      for(const [key,descriptor] of Object.entries(descriptors)){
        if(key==='length'&&Array.isArray(v))continue;
        if(!Object.hasOwn(descriptor,'value'))throw Error('UiFragmentCacheMismatch');
        bytes+=utf8(key);pending.push({value:descriptor.value,depth:entry.depth+1});
      }
    }else if(typeof v==='string'){if(v.length>bound.maxCacheBytes)throw Error('UiFragmentCacheMismatch');bytes+=utf8(v);}else if(!['number','boolean'].includes(typeof v)&&v!==null)throw Error('UiFragmentCacheMismatch');
    if(bytes>bound.maxCacheBytes+4096)throw Error('UiFragmentCacheMismatch');
  }
}
const compare=(a,b)=>{for(let i=0;i<Math.max(a.length,b.length);i++){const d=(a[i]||0)-(b[i]||0);if(d)return d;}return 0;};
export function searchUi(app,requirements,options={}) {
  return runSearch(app,requirements,options,null);
}
// A preparation handle captures one immutable, internally checked bank. It has
// no mutable query history, no effects, and no decision/publication authority.
// This is distinct from the checked decision; each invocation revalidates its
// expanded whole tree. No caller-owned object aliases the retained bank.
export function createUiSearchSession(options={}) {
  const bound=limits(options),prepared=buildFragmentCache(bound);
  if(!prepared.cache)return freeze({receipt:prepared.receipt,search:null});
  return Object.freeze({receipt:prepared.receipt,search:(app,requirements,query={})=>{
    if(Object.keys(query).some(k=>k!=='amortizedQueries'))throw Error('UiSessionBoundChange');
    return runSearch(app,requirements,{...bound,...query,mode:'fragments'},prepared);
  }});
}
function runSearch(app,requirements,options,prepared) {
  const bound=limits(options),mode=options.mode??'fragments';
  if(!['direct','fragments'].includes(mode))throw Error('UiSearchMode');
  const queries=options.amortizedQueries??1;
  if(!Number.isSafeInteger(queries)||queries<1||queries>1000000)throw Error('UiSearchAmortization');
  const work=meter(bound.maxWork),seen=new Set(),candidateTrace=[];let bank=null,preparationWork=0,cacheVerificationWork=0,reason=null,complete=false,chosen=null,examined=0,feasible=0,duplicates=0;
  try {
    if(mode==='fragments'){
      if(options.cache)inspectUntrustedCache(options.cache,bound,work);
      const preparationStart=work.used;
      bank=prepared?.cache??prepare(bound,work);preparationWork=work.used-preparationStart;
      if(bank.storageBytes>bound.maxCacheBytes)throw new Exhausted('cache-budget');
      if(options.cache){
        // Public cache data is untrusted. Verify the complete bank, not a summary,
        // and use the freshly checked local value. This cost is NOT hidden.
        const supplied=canonical(options.cache),expected=canonical(bank);
        work.take('cacheVerificationBytes',utf8(supplied)+utf8(expected));cacheVerificationWork=work.used-preparationWork;
        if(supplied!==expected)throw Error('UiFragmentCacheMismatch');
      }
    }else {if(options.cache)throw Error('UiUnusedFragmentCache');bank={identity:{fragmentSize:bound.fragmentSize},entries:[],storageBytes:0};}
    // Round robin avoids spending a small budget solely on one form strategy.
    const streams=[];
    const v2=requirements.format==='zenoui/requirements/2';
    for(const grouped of [true,false])for(const details of [true,false])for(const collapsed of (v2?[true]:[true,false]))for(const first of [true,false])for(const reverseProjections of (requirements.projections.length>1?[false,true]:[false]))for(const generic of (v2?[false,true]:[false]))streams.push(candidates(app,requirements,grouped,details,collapsed,first,reverseProjections,generic,mode,bank,work));
    let active=streams;
    while(active.length && examined<bound.maxCandidates){
      const next=[];
      for(const stream of active){
        if(examined===bound.maxCandidates){next.push(stream);continue;}
        const item=stream.next();if(item.done)continue;next.push(stream);
        const ast=item.value,nodes=size(ast);work.take('canonicalNodes',nodes);const key=canonical(ast);
        if(seen.has(key)){duplicates++;continue;}seen.add(key);
        work.take('validationNodes',nodes);examined++;
        const checked=validateAst(app,requirements,ast,{maxNodes:bound.maxNodes,maxDepth:bound.maxDepth});
        const accepted=checked.valid && nodes<=bound.maxNodes && checked.summary.depth<=bound.maxDepth;
        const score=accepted?scoreAst(app,requirements,ast,checked.summary):null;
        candidateTrace.push({ordinal:examined-1,structuralDigest:structuralDigest(key),nodes,depth:checked.summary.depth,accepted,failures:checked.failures.map(f=>f.code),score});
        if(!accepted)continue;
        feasible++;
        if(!chosen || compare(score,chosen.score)<0 || (compare(score,chosen.score)===0 && key<chosen.key))chosen={ast,score,key,ordinal:examined-1};
      }
      active=next;
    }
    complete=active.length===0;if(!complete)reason='candidate-budget';
  }catch(error){if(!(error instanceof Exhausted))throw error;reason=error.message;}
  const onlineWork=work.used-preparationWork;
  const initialPreparation=prepared?.receipt.work??preparationWork;
  const repeatedQueryWork=work.used;
  const amortizedTotal=prepared?initialPreparation+queries*repeatedQueryWork:queries*work.used;
  // Amortization is a declared repeated-query scenario, not a measured history.
  // Only an immutable prepared session may exclude repeated cold construction.
  const receipt={format:'zenoui/ui-search/1',grammar:SEARCH_GRAMMAR,checker:CHECKER,mode,bounds:bound,complete,reason,examined,feasible,duplicates,selectedScore:chosen?.score??null,selected:chosen?{ordinal:chosen.ordinal,structuralDigest:structuralDigest(chosen.key)}:null,candidateTrace,traceDigestKind:'fnv1a32-utf16-nonauthoritative',work:{total:work.used,preparation:preparationWork,cacheVerification:cacheVerificationWork,online:onlineWork,categories:work.categories},cache:{entries:bank?.entries.length??0,storageBytes:bank?.storageBytes??0,peakPreparationBytes:work.peakCacheBytes??0},amortization:{queries,scenario:'same-query-repeated',preparedSession:!!prepared,initialPreparation,coldWork:initialPreparation+onlineWork,checkedReuseWorkPerQuery:repeatedQueryWork,totalWork:amortizedTotal,workPerQuery:amortizedTotal/queries},nonclaims:['Bounded prefix only; not globally optimal.','Structural canonical equality only; no behavioral equivalence quotient.','Work counters are not wall-clock performance.','Cache preparation is stateful optimization, never decision authority.']};
  return freeze({ast:chosen?copy(chosen.ast):null,receipt});
}
