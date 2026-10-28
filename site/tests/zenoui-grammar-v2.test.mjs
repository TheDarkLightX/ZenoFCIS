// Component grammar version 2: relevance-exact forms, who is acting, state displays outside the
// collapsed technical record, command groups and the swap-card idiom.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import * as grammar from '../public/zenoui/ui-grammar.js';
import {searchUi} from '../public/zenoui/ui-search.js';
import {validateRequirements,deriveRequirements} from '../public/zenoui/layout-search.js';
import {displayPlan} from '../public/zenoui/ui-idioms.js';
const f=(path,index,source,role,extra={})=>({path,index,source,role,type_name:'T',unit:'units',label:path,options:[],domain:{kind:'int',min:'0',max:'3'},initial:'0',...extra});
// A pool: two reserves, a stage, two swap directions sharing amount and minimum, an add with its own amount.
const app={action:3,commands:[{id:'1',label:'A for B'},{id:'2',label:'B for A'},{id:'3',label:'Add'}],lanes:[],
  fields:[f('pre.a',0,'state','amount'),f('pre.b',1,'state','amount'),f('pre.stage',2,'state','stage',{options:[{value:'5'},{value:'6'}],domain:{kind:'sum'},stages:[['5'],['6']]}),
    f('command.action',3,'command','status',{domain:{kind:'sum'}}),f('command.pay',4,'command','amount-in'),f('command.min',5,'command','min-out'),f('command.add',6,'command','amount'),f('context.party',7,'context','status',{domain:{kind:'sum'}})],
  relevant:{'1':[4,5,7],'2':[4,5,7],'3':[6,7]}};
const requirements=()=>({...deriveRequirements(app),groups:[{id:'swap',commands:['1','2']}],groupCommands:true,
  projections:[{id:'receive',label:'You receive',kind:'accepted-decrease',fieldByCommand:{'1':'pre.b','2':'pre.a'}}],quoteOverrides:{'command.min':'0'}});
const search=req=>searchUi(app,req,{maxCandidates:512});
const find=(node,pred)=>pred(node)?node:(node.children??[]).map(c=>find(c,pred)).find(Boolean);
const inputsOf=(ast,scope)=>{const out=[];(function walk(n){if(n.kind==='input'&&n.scope===scope)out.push(n.field);(n.children??[]).forEach(walk);})(ast);return out;};
function relevanceChecks(g){
  const req=requirements();validateRequirements(app,req);
  const {ast}=search(req);assert.ok(ast,'a tree is found');
  assert.equal(g.validateAst(app,req,ast).valid,true);
  assert.deepEqual(inputsOf(ast,'group-swap').sort(),['command.min','command.pay'],'R2_scope_inputs_exactly_relevant');
  assert.deepEqual(inputsOf(ast,'3'),['command.add'],'R2_scope_inputs_exactly_relevant');
  // An irrelevant input in a form, or a missing relevant one, is refused.
  const extra=structuredClone(ast),add=find(extra,n=>n.kind==='scope'&&n.scope==='3');
  add.children=[{kind:'stack',children:[{kind:'input',scope:'3',field:'command.pay'},...add.children]}];
  assert.ok(g.validateAst(app,req,extra).failures.some(x=>x.code==='input-coverage'),'R2_irrelevant_input_refused');
  const missing=structuredClone(ast);(function strip(n){if(n.children)n.children=n.children.filter(c=>!(c.kind==='input'&&c.field==='command.add')).map(c=>(strip(c),c));})(missing);
  assert.equal(g.validateAst(app,req,missing).valid,false,'R2_missing_relevant_input_refused');
}
test('forms hold exactly the inputs that can change their commands',()=>relevanceChecks(grammar));
test('control: a checker that skips input coverage fails the relevance check',async()=>{
  const source=await readFile(new URL('../public/zenoui/ui-grammar.js',import.meta.url),'utf8'),site="if(required.some(p=>record.input.get(p)!==1)||record.input.size!==required.length)fail('input-coverage',scope);";
  assert.ok(source.includes(site));
  const idioms=new URL('../public/zenoui/ui-idioms.js',import.meta.url).href;
  const mutant=await import('data:text/javascript;base64,'+Buffer.from(source.replace(site,'').replace("'./ui-idioms.js'",JSON.stringify(idioms))).toString('base64'));
  assert.throws(()=>relevanceChecks(mutant),assert.AssertionError);
});
test('who is acting and every state input are shown once, outside the collapsed technical record',()=>{
  const req=requirements(),{ast}=search(req);
  assert.equal(find(ast,n=>n.kind==='acting')?.kind,'acting');
  assert.deepEqual(displayPlan(app).map(d=>d.idiom),['stepper','metrics']);
  for(const mutate of [a=>{a.children=a.children.filter(c=>c.kind!=='acting');},
    a=>{const d=find(a,n=>n.kind==='display'&&n.idiom==='metrics');d.fields=d.fields.slice(1);},
    a=>{const d=find(a,n=>n.kind==='display'&&n.idiom==='metrics');d.fields=[...d.fields,'pre.stage'];},
    a=>{const p=find(a,n=>n.kind==='disclosure'&&n.label==='Evidence');p.kind='stack';delete p.label;}]){
    const changed=structuredClone(ast);mutate(changed);assert.equal(grammar.validateAst(app,req,changed).valid,false);
  }
});
test('groups need identical relevant inputs; the swap card must match its signature',()=>{
  const req=requirements(),{ast}=search(req);
  const card=find(ast,n=>n.kind==='idiom');assert.equal(card?.idiom,'swap-card','the fitting group is drawn as a swap card');
  assert.deepEqual(card.children.map(c=>c.kind),['input','selector','projection','disclosure','review'],'pay, switch, receive, options, button');
  const mixed={...req,groups:[{id:'swap',commands:['1','3']}]};
  assert.ok(grammar.validateAst(app,mixed,{...ast}).failures.some(x=>['group-relevance','unknown-scope','command-coverage'].includes(x.code)));
  const unroled={...app,fields:app.fields.map(x=>x.path==='command.min'?{...x,role:'amount'}:x)};
  assert.ok(grammar.validateAst(unroled,req,ast).failures.some(x=>x.code==='idiom-signature'),'a swap card needs a minimum-out input');
  const generic=structuredClone(ast),scope=find(generic,n=>n.kind==='scope'&&n.scope==='group-swap');scope.children=scope.children[0].children;
  const a=grammar.validateAst(app,req,ast),b=grammar.validateAst(app,req,generic);
  assert.equal(b.valid,true);assert.ok(grammar.scoreAst(app,req,ast,a.summary)[0]<grammar.scoreAst(app,req,generic,b.summary)[0],'idiom fit ranks first');
});
test('the state summary before the actions scores better than after them',()=>{
  const req=requirements(),{ast}=search(req);
  const late=structuredClone(ast),i=late.children.findIndex(c=>c.kind==='commands'),summary=late.children.splice(1,i-1);late.children.splice(late.children.findIndex(c=>c.kind==='status')+1,0,...summary);
  const a=grammar.validateAst(app,req,ast),b=grammar.validateAst(app,req,late);
  assert.equal(b.valid,true);assert.ok(grammar.scoreAst(app,req,ast,a.summary)[1]<grammar.scoreAst(app,req,late,b.summary)[1]);
});
test('requirements version 2 refuse context in forms, open technical panels and malformed groups',()=>{
  for(const change of [r=>{r.primaryFields.push('context.party');r.contextFields=[];},r=>{r.maxExpandedExpertPanels=1;},r=>{r.groups=[{id:'swap',commands:['1']}];},r=>{r.groups=[{id:'Bad id',commands:['1','2']}];},r=>{r.groups=[{id:'a',commands:['1','2']},{id:'b',commands:['2','3']}];}]){
    const r=requirements();change(r);assert.throws(()=>validateRequirements(app,r),/^Error: Ui/);
  }
});
