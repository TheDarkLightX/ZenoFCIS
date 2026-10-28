// Synthetic reducer and DOM controls. The native server's masking is a separate boundary.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe} from '../public/zenoui/app-contract.js';
import {initial,step,view,validReply} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {buildTree} from '../public/zenoui/ui-renderer.js';
import {documentFrom} from './browser-fixture.mjs';
import {editFixture,approvalFixture} from './idiom-fixtures.mjs';

const contract='a'.repeat(64),principal='b'.repeat(64);
const fixtures=[['scalar-edit',editFixture],['approval-decision',approvalFixture]].map(([name,make])=>{
  const {raw,meta}=make();raw.reasons.push({id:'9'});meta.reasons['9']='Recorded failure';
  const extra={path:'pre.extra',index:raw.fields.length,source:'state',domain:{kind:'int',min:'0',max:'9'},initial:'3',options:[],type_name:'Extra'};
  raw.fields.push(extra);raw.domain=(BigInt(raw.domain)*10n).toString();meta.fields[extra.path]={label:'Other counter',unit:'ticks'};
  return {name,app:describe(raw,meta)};
});
const accept=(binding,post)=>({input:binding.input,class:'Accept',reason:null,post,checked:'1',total:'1'});
function setup(app,values){
  let s=initial(app,contract);s=step(s,{type:'Principal',value:principal}).state;
  return step(s,{type:'Snapshot',values,revision:'0',observation:{contract,principal,pending:'0'}});
}
function previewAll(start,post){
  let s=start.state;
  for(const {binding} of start.intents)s=step(s,{type:'Preview',binding,reply:accept(binding,post)}).state;
  return s;
}
function consent(s,id){return step(step(s,{type:'Review',command:id}).state,{type:'Confirm'}).state;}

test('masked previews describe visible changes without claiming hidden state is unchanged',()=>{
  const previous=Object.getOwnPropertyDescriptor(globalThis,'document');
  Object.defineProperty(globalThis,'document',{value:documentFrom('<div id="app"></div>'),writable:true,configurable:true});
  try{
    for(const {name,app} of fixtures){
      const id=app.commands[0].id,start=setup(app,[null,'3']);
      let s=previewAll(start,[null,'3']);s=step(s,{type:'Review',command:id}).state;
      const data=view(s),rendered=presentationView(app,data,{server:true});
      assert.deepEqual(data.state.map(f=>f.label),['Other counter'],name);
      assert.deepEqual(data.confirmation.changes,[],name);
      const confirmation=Object.values(rendered.nodes).find(n=>n.open);
      assert.match(confirmation.consequence,/no state changes are visible in this preview/i);
      const tree=buildTree(app.presentation.ast,rendered,{review(){},select(){},refresh(){},edit(){},context(){},principal(){},confirm(){}});
      tree.update(rendered);
      assert.match(tree.element.textContent,/Visible changes/);
      assert.match(tree.element.textContent,/No state changes are visible in this preview/);
      assert.doesNotMatch(tree.element.textContent,/current state does not change/i);
      assert.ok(!data.confirmation.fields.some(f=>f.part==='state'&&f.index===0),'masked field is absent from the complete request');
    }
  }finally{if(previous)Object.defineProperty(globalThis,'document',previous);else delete globalThis.document;}
});

function checkMaskValidation(validate){
  for(const {name,app} of fixtures)for(const values of [[null,'3'],['0',null],[null,null]]){
    const start=setup(app,values),binding=start.intents[0].binding;
    for(const resultClass of ['Accept','CommittedFailure']){
      const good={...accept(binding,values),class:resultClass,reason:resultClass==='Accept'?null:'9'};
      assert.equal(validate(app,binding.input,good),true,`${name}: correctly masked ${resultClass}`);
      for(let index=0;index<values.length;index++)if(values[index]===null){
        const post=[...values];post[index]='1';
        assert.equal(validate(app,binding.input,{...good,post}),false,`${name}: ${resultClass} must preserve mask at ${index}`);
      }
    }
  }
}

test('accepted and committed-failure replies preserve every masked state position',()=>{
  checkMaskValidation(validReply);
  for(const {app} of fixtures){
    const {binding}=setup(app,['0','3']).intents[0];
    assert.equal(validReply(app,binding.input,accept(binding,['1','4'])),true,'visible results remain accepted');
    assert.equal(validReply(app,binding.input,accept(binding,[null,'4'])),false,'visible fields cannot become masked in an ordinary reply');
  }
});

test('malformed unmasking previews and results refuse without changing the frozen state',()=>{
  for(const {name,app} of fixtures){
    const start=setup(app,[null,'3']),id=app.commands[0].id,binding=start.intents[0].binding;
    assert.throws(()=>step(start.state,{type:'Preview',binding,reply:accept(binding,['1','3'])}),/UiInvalidCoreReply/,name);
    assert.deepEqual(view(start.state).state.map(f=>f.label),['Other counter']);
    const ready=consent(previewAll(start,[null,'3']),id),sent=step(ready,{type:'Dispatch',command:id});
    assert.equal(sent.intents.length,1);assert.equal(sent.state.delivery,'unknown');
    assert.throws(()=>step(sent.state,{type:'Result',binding:sent.state.active,reply:accept(sent.state.active,['1','3']),delivery:'pending'}),/UiInvalidCoreReply/,name);
    assert.equal(sent.state.delivery,'unknown');assert.deepEqual(sent.state.values,[null,'3']);
    const result=step(sent.state,{type:'Result',binding:sent.state.active,reply:accept(sent.state.active,[null,'4']),delivery:'pending'}).state;
    assert.deepEqual(result.values,[null,'4']);assert.equal(result.delivery,'pending');
    assert.deepEqual(view(result).state.map(f=>f.label),['Other counter']);
  }
});

test('mask-preservation regression kills the permissive validator',async()=>{
  const file=new URL('../public/zenoui/app-state.js',import.meta.url),source=await readFile(file,'utf8');
  const strict='hidden[i]?reply.post[i]===null:inDomain(f,reply.post[i])';assert.ok(source.includes(strict));
  const permissive='inDomain(f,reply.post[i])||(hidden[i]&&reply.post[i]===null)';
  const code=source.replace(strict,permissive).replace(/(from\s+)(['"])(\.\/[^'"]+)\2/g,(_,prefix,quote,specifier)=>prefix+JSON.stringify(new URL(specifier,file).href));
  const mutant=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
  assert.throws(()=>checkMaskValidation(mutant.validReply),assert.AssertionError);
});
