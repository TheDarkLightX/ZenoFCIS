import assert from 'node:assert/strict';
import {test} from 'node:test';
import {validateAst,scoreAst,EXPERT_REGIONS} from '../public/zenoui/ui-grammar.js';
const app={action:1,commands:[{id:'1'},{id:'2'}],fields:[{path:'stock',index:0,source:'state'},{path:'action',index:1,source:'command'},{path:'amount',index:2,source:'command'},{path:'minimum',index:3,source:'command'}]};
const req={primaryFields:['amount'],detailFields:['minimum'],fixedFields:[],projections:[{id:'receive',kind:'accepted-decrease'}],groupCommands:true,collapseExpert:true,compact:true,maxVisibleForms:2,maxExpandedExpertPanels:5,maxColumns:2};
const input=(field,scope='selected')=>({kind:'input',field,scope});
function form(scope='selected') {return {kind:'scope',scope,children:[...(scope==='selected'?[{kind:'selector',scope}]:[]),{kind:'card',children:[input('amount',scope),{kind:'projection',projection:'receive',scope}]},{kind:'disclosure',label:'Details',children:[input('minimum',scope)]},{kind:'review',scope}]};}
function tree(){return {kind:'page',children:[{kind:'commands',children:[form()]},{kind:'confirmation'},{kind:'status'},{kind:'disclosure',label:'Evidence',children:EXPERT_REGIONS.map(region=>({kind:'evidence',region}))}]};}
const accepted=ast=>validateAst(app,req,ast);
test('recursive typed trees accept shared or explicit scopes and independently derive summaries',()=>{
 const a=tree(),r=accepted(a);assert.equal(r.valid,true);assert.equal(r.summary.visibleForms,1);assert.equal(r.summary.expandedExpertPanels,0);assert.equal(r.summary.primaryCardGaps,0);assert.equal(r.summary.disclosureInputs,1);
 const b=tree();b.children[0].children=[{kind:'grid',children:[form('1'),form('2')]}];const alternate=accepted(b);assert.equal(alternate.valid,true);assert.equal(alternate.summary.visibleForms,2);assert.equal(alternate.summary.columns,2);assert.notDeepEqual(scoreAst(app,req,a,r.summary),scoreAst(app,req,b,alternate.summary));
 assert.equal(validateAst(app,{...req,maxVisibleForms:1},b).valid,false);assert.equal(validateAst(app,{...req,maxColumns:1},b).valid,false);
});
test('whole-tree checker rejects invalid local/global composition rather than trusting claimed summaries',()=>{
 const mutations=[a=>a.summary={safe:true},a=>a.children[0].children[0].children.push(input('amount')),a=>a.children[0].children[0].children[1].children[0].scope='2',a=>a.children[0].children[0].children[1].children[0].field='stock',a=>a.children[0].children[0].children[1].children[1].projection='invented',a=>a.children[0].children[0].children[1]={kind:'disclosure',label:'Details',children:a.children[0].children[0].children[1].children},a=>a.children[0].children[0].children.reverse(),a=>a.children.push({kind:'confirmation'}),a=>a.children[1]={kind:'disclosure',label:'Details',children:[{kind:'confirmation'}]},a=>a.children[3].children.pop(),a=>a.children[3].children.push({kind:'evidence',region:'state'}),a=>a.children[0].children.push(form('1')),a=>a.children[0].children[0].children.push({kind:'dispatch',scope:'selected'}),a=>a.children[0].children[0].children.push(form('2')),a=>a.children[2]={kind:'__proto__'}];
 for(const mutate of mutations){const a=tree();mutate(a);assert.equal(accepted(a).valid,false);}
});
test('depth/node exhaustion is refusal, not partial acceptance',()=>{
 assert.equal(validateAst(app,req,tree(),{maxNodes:3,maxDepth:10}).valid,false);
 assert.equal(validateAst(app,req,tree(),{maxNodes:128,maxDepth:2}).valid,false);
 const a=tree();for(let i=0;i<100;i++)a.children[0]={kind:'card',children:[a.children[0]]};assert.equal(accepted(a).valid,false);
});
