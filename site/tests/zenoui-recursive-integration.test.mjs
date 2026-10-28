import assert from 'node:assert/strict';
import {test} from 'node:test';
import {describe,checkPresentation,verifyPresentation} from '../public/zenoui/app-contract.js';
import {deriveRequirements,validateRequirements} from '../public/zenoui/layout-search.js';
import {scopeInputs} from '../public/zenoui/ui-grammar.js';
import {initial,inputFor,project,step,view} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {mount,sha256} from '../public/zenoui/app.js';

const field=(path,index,source,domain,initial,extra={})=>({path,index,source,domain,initial,options:[],type_name:path,...extra});
const raw=()=>({fields:[
  field('pre.100.110',0,'state',{kind:'int',min:'0',max:'3'},'0'),
  field('command.101.120',1,'command',{kind:'sum',type_id:120,variants:[1,2]},'1',{options:[{value:'1'},{value:'2'}]}),
  field('command.101.121',2,'command',{kind:'int',min:'0',max:'3'},'0'),
  field('context.102.130',3,'context',{kind:'bool'},'0'),
  field('context.102.131',4,'context',{kind:'int',min:'0',max:'5'},'0',{clock:true}),
],reasons:[],enablement:{table:[false,false,false,true]},domain:'384'});
const metadata=()=>({format:'zenoui/metadata/1',title:'Orders',action:'command.101.120',
  fields:{'pre.100.110':{label:'Stock',unit:'items'},'command.101.120':{label:'Action',unit:'unitless'},
    'command.101.121':{label:'Quantity',unit:'items'},'context.102.130':{label:'Manager',unit:'unitless'},
    'context.102.131':{label:'Clock',unit:'seconds'}},variants:{1:'Add',2:'Remove'},
  commands:{1:{label:'Add',consequential:true},2:{label:'Remove',consequential:true}},reasons:{}});
const contract='a'.repeat(64);
const walk=(node,visit)=>{visit(node);for(const child of node.children??[])walk(child,visit);};

test('metadata/1 produces a checked recursive tree with conservative inputs and commands',()=>{
  const app=describe(raw(),metadata());
  assert.equal(app.format,'zenoui/app-description/2');
  assert.equal(verifyPresentation(app),true);
  assert.deepEqual(app.relevant,{'1':[2,3,4],'2':[2,3,4]});
  assert.equal(app.possible,null);assert.equal(app.situation,null);assert.equal(app.bounded,false);
  const scopes=[];walk(app.presentation.ast,n=>{if(n.kind==='scope')scopes.push(n.scope);});
  assert.ok(scopes.length>0);
  for(const scope of scopes)assert.deepEqual(scopeInputs(app,app.presentation.requirements,scope),['command.101.121']);
  const state=initial(app,contract),refresh=step(state,{type:'Refresh'});
  assert.deepEqual(refresh.intents.map(i=>i.binding.command).sort(),['1','2']);
  for(const intent of refresh.intents)assert.equal(intent.binding.input.length,app.fields.length);
});

test('unbound semantic/table claims and forged relevance are refused',()=>{
  const model=raw(),meta=metadata();
  assert.throws(()=>describe({...model,semantics:{}},meta),/UiUnsupportedUnboundSemanticsOrTables/);
  assert.throws(()=>describe({...model,tables:[]},meta),/UiUnsupportedUnboundSemanticsOrTables/);
  const app=describe(model,meta),missing=structuredClone(app);
  missing.relevant['1']=[2,3];assert.throws(()=>checkPresentation(missing),/UiInvalidFallbackRelevance/);
  const invented=structuredClone(app);invented.possible={'1':'+'};
  assert.throws(()=>checkPresentation(invented),/UiUnsupportedUnboundSemanticsOrTables/);
  const legacy=structuredClone(deriveRequirements(app));legacy.format='zenoui/requirements/1';
  assert.throws(()=>describe(model,{...meta,format:'zenoui/metadata/2',requirements:legacy}),/UiUnsupportedRequirementsVersion/);
  const forged=structuredClone(app);forged.presentation.requirements.format='zenoui/requirements/1';
  assert.throws(()=>checkPresentation(forged),/UiUnsupportedRequirementsVersion/);
});

test('a shallow frozen presentation cannot reuse a stale nested check',()=>{
  const app=Object.freeze(structuredClone(describe(raw(),metadata())));
  assert.equal(checkPresentation(app),true);
  app.presentation.ast.children.pop();
  assert.throws(()=>checkPresentation(app),/UiCompositionRefused|UiPresentationReceiptMismatch/);
});

test('fast mount check rejects a mismatched selected receipt entry',()=>{
  const app=describe(raw(),metadata());
  for(const change of [a=>a.searchReceipt.selected.structuralDigest='00000000',a=>a.searchReceipt.selected.ordinal=-1,
    a=>a.searchReceipt.candidateTrace[a.searchReceipt.selected.ordinal].accepted=false]){
    const changed=structuredClone(app);change(changed);
    assert.throws(()=>checkPresentation(changed),/UiPresentationReceiptMismatch/);
  }
});

test('authenticated context is locked and server clock is omitted from the shown review',()=>{
  const app=describe(raw(),metadata());let state=structuredClone(initial(app,contract));state.principal='Server principal';
  state=step(state,{type:'Snapshot',values:['1'],revision:'0',observation:{contract,principal:'Server principal',pending:'3'}}).state;
  assert.equal(state.contextLocked,true);assert.deepEqual(state.pendingAtSignIn,{revision:'0',count:'3'});
  for(const event of [{type:'Context',index:3,value:'1'}, {type:'Edit',command:'1',index:4,value:'5'}, {type:'Principal',value:'Other'}])
    assert.equal(step(state,event).state,state);
  let refreshed=step(state,{type:'Refresh'});const intent=refreshed.intents.find(i=>i.binding.command==='1');
  assert.equal(intent.binding.input[4],'0');
  assert.equal(view(state).drafts['1'][4],null,'server clock is not exposed in the public view drafts');
  const reply={input:intent.binding.input,checked:'1',total:'1',class:'Accept',reason:null,post:['2']};
  state=step(refreshed.state,{type:'Preview',binding:intent.binding,reply}).state;
  state=step(state,{type:'Review',command:'1'}).state;
  assert.ok(view(state).confirmation);assert.ok(!view(state).confirmation.fields.some(f=>f.index===4));
  assert.ok(!view(state).context.some(f=>f.index===4));
  const rendered=presentationView(app,view(state),{server:true});
  assert.ok(Object.values(rendered.nodes).filter(n=>n.server).every(n=>n.fields.length===0),'server acting context has no editable fields');
});

test('malformed server context or snapshot is refused before mounting',async()=>{
  const description=describe(raw(),metadata()),source='fixture',identity=await sha256({description,source});
  const bundle={description,source,contract:identity};
  const session={contract,principal:'Server principal',revision:'0',values:['1'],context:['0','0'],pending:'0'};
  for(const bad of [{...session,context:['0']},{...session,context:['2','0']},{...session,revision:'-1'},{...session,values:['99']},{...session,principal:''}])
    await assert.rejects(mount({},bundle,{session:bad}),/UiInvalidSession/);
});

test('hidden party state is absent from displays and projections',()=>{
  const base=describe(raw(),metadata()),req=structuredClone(deriveRequirements(base));
  req.projections=[{id:'remaining',label:'Remaining',kind:'state-complement',fieldByCommand:{1:'pre.100.110',2:'pre.100.110'},total:'3'}];
  const app=describe(raw(),{...metadata(),format:'zenoui/metadata/2',requirements:req});
  let state=structuredClone(initial(app,contract));state.values=[null];
  assert.equal(project(state,'1')[0].value,null);assert.deepEqual(view(state).state,[]);
  assert.equal(inputFor(state,'1')[0],null);
});

test('projection labels use the same visible-text boundary as descriptor labels',()=>{
  const model=raw(),meta=metadata(),app=describe(model,meta),base=deriveRequirements(app);
  const requirements=label=>({...structuredClone(base),projections:[{id:'remaining',label,kind:'state-complement',fieldByCommand:{1:'pre.100.110'},total:'3'}]});
  for(const label of ['a\u007fb','a\u0085b','a\u00adb','a\u034fb','a\u200bb','a\u202eb','a\ue000b','a\ufeffb'])
    assert.throws(()=>validateRequirements(app,requirements(label)),/UiProjectionLabel/);
  for(const label of ['Inventory','库存','Disponible'])assert.equal(validateRequirements(app,requirements(label)),true);
  assert.throws(()=>describe(model,{...meta,format:'zenoui/metadata/2',requirements:requirements('a\u202eb')}),/UiProjectionLabel/);
  assert.doesNotThrow(()=>describe(model,{...meta,format:'zenoui/metadata/2',requirements:requirements('Inventory')}));
});

test('server pending status remains in the recursive pure view',()=>{
  const app=describe(raw(),metadata()),state=initial(app,contract);
  const rendered=presentationView(app,view(state),{server:true,delivery:'Delivery pending.',pending:'Three pending at sign-in.'});
  let statusPath=null;const visit=(n,path)=>{if(n.kind==='status')statusPath=path;(n.children??[]).forEach((c,i)=>visit(c,`${path}.${i}`));};visit(app.presentation.ast,'$');
  assert.ok(statusPath);assert.equal(rendered.nodes[statusPath].delivery,'Delivery pending.');
  assert.equal(rendered.nodes[statusPath].pending,'Three pending at sign-in.');
});
