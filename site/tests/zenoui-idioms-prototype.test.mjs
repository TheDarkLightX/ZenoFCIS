import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';
import {describe,checkPresentation,verifyPresentation,layoutCheck,REGIONS,display} from '../public/zenoui/app-contract.js';
import {validateAst} from '../public/zenoui/ui-grammar.js';
import {initial,step,view,enabled,validReply} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {mount,sha256} from '../public/zenoui/app.js';
import {documentFrom} from './browser-fixture.mjs';
import {editFixture,approvalFixture} from './idiom-fixtures.mjs';

const clone=structuredClone;
const walk=(node,fn)=>{fn(node);for(const child of node.children??[])walk(child,fn);};
const find=(node,pred)=>{let result=null;walk(node,n=>{if(!result&&pred(n))result=n;});return result;};
const idiom=app=>find(app.presentation.ast,n=>n.kind==='idiom');
const fixtureApp=make=>{const {raw,meta}=make();return describe(raw,meta);};
const edit=fixtureApp(editFixture),approval=fixtureApp(approvalFixture);
const contract='a'.repeat(64),principal='b'.repeat(64);

test('declared scalar edit and approval synthesize checked, complete trees',()=>{
  for(const [app,name,firstControl] of [[edit,'scalar-edit','input'],[approval,'approval-decision','selector']]){
    const chosen=idiom(app);
    assert.equal(chosen?.idiom,name);
    assert.deepEqual(chosen.children.map(n=>n.kind),['display',firstControl,'review']);
    assert.equal(checkPresentation(app),true);
    assert.equal(verifyPresentation(app),true);
    const state=[];walk(app.presentation.ast,n=>{if(n.kind==='display')state.push(...n.fields);});
    assert.deepEqual(state.sort(),app.fields.filter(f=>f.source==='state').map(f=>f.path).sort(),'state appears exactly once');
    assert.equal(app.possible,null);assert.equal(app.situation,null);
  }
});

test('role, unit, source, consequence and state bindings are required',()=>{
  for(const mutate of [
    m=>m.fields['command.101.121'].stateField='context.102.131',
    m=>m.fields['command.101.121'].unit='seconds',
    m=>m.fields['command.101.121'].decimals=2,
    m=>delete m.fields['command.101.121'].stateField,
    m=>m.commands['120'].consequential=false,
    m=>m.commands['120'].uiRole='unknown',
    m=>{m.fields['context.102.131'].role='requested-value';m.fields['context.102.131'].stateField='pre.100.110';},
  ]){
    const {raw,meta}=editFixture();mutate(meta);assert.throws(()=>describe(raw,meta),/UiInvalid/);
  }
  const {raw,meta}=approvalFixture();meta.commands['221'].uiRole='approve';
  const generic=describe(raw,meta);
  assert.notEqual(idiom(generic)?.idiom,'approval-decision','duplicate role cannot become a decision pair');
});

test('independent whole-tree checker rejects forged signatures and misplaced observations',()=>{
  for(const [app,changes] of [
    [edit,[a=>a.commands[0].uiRole='approve',a=>a.fields[2].unit='seconds',
      a=>a.fields[2].stateField='context.102.131',
      a=>idiom(a).children[0].fields=['context.102.131'],
      a=>idiom(a).children.reverse()]],
    [approval,[a=>a.commands[1].uiRole='approve',a=>a.commands[0].consequential=false,
      a=>a.fields[0].role='status',a=>idiom(a).children[0].fields=['context.202.230'],
      a=>idiom(a).children.pop()]],
  ]){
    for(const mutate of changes){const forged=clone(app);mutate(forged);
      assert.equal(validateAst(forged,forged.presentation.requirements,forged.presentation.ast).valid,false);}
  }
  const duplicated=clone(edit);duplicated.presentation.ast.children.unshift(clone(idiom(duplicated).children[0]));
  assert.equal(validateAst(duplicated,duplicated.presentation.requirements,duplicated.presentation.ast).valid,false);
});

test('signature guards reject mutants the rest of the grammar would accept',async()=>{
  const source=await readFile(new URL('../public/zenoui/ui-grammar.js',import.meta.url),'utf8');
  const idioms=new URL('../public/zenoui/ui-idioms.js',import.meta.url).href;
  for(const [app,change,guard] of [
    [edit,a=>a.commands[0].uiRole='approve',"if(record.idiom==='scalar-edit'&&(!edit||!idiomChildren(record.idiomNode,stateField,'input')))fail('idiom-signature',scope);"],
    [approval,a=>a.commands[1].uiRole='approve',"if(record.idiom==='approval-decision'&&(!approval||!idiomChildren(record.idiomNode,approvalField,'selector')))fail('idiom-signature',scope);"],
  ]){
    assert.ok(source.includes(guard));const forged=clone(app);change(forged);
    assert.equal(validateAst(forged,forged.presentation.requirements,forged.presentation.ast).valid,false);
    const code=source.replace(guard,'').replace("'./ui-idioms.js'",JSON.stringify(idioms));
    const mutant=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
    assert.equal(mutant.validateAst(forged,forged.presentation.requirements,forged.presentation.ast).valid,true,'guard removal exposes the forged tree');
  }
});

test('hidden party state stays absent from both specialized views',()=>{
  for(const app of [edit,approval]){
    const state=clone(initial(app,contract));state.values=[null];
    const rendered=presentationView(app,view(state),{server:true});
    let displayPath=null;
    const visit=(n,path)=>{if(n.kind==='idiom')displayPath=path+'.0';(n.children??[]).forEach((x,i)=>visit(x,path+'.'+i));};
    visit(app.presentation.ast,'$');
    assert.deepEqual(rendered.nodes[displayPath].rows,[]);
    assert.ok(!JSON.stringify(rendered.nodes[displayPath]).includes('Pending'));
  }
});

test('both idioms retain the eight UI laws on bounded simulated decisions',()=>{
  for(const app of [edit,approval]){
    let state=clone(initial(app,contract));state.principal=principal;
    state=step(state,{type:'Snapshot',values:[app===edit?'2':'0'],revision:'0',
      observation:{contract,principal,pending:'0'}}).state;
    assert.deepEqual(view(state).layout.map(x=>x.id).sort(),[...REGIONS].sort()); // L1
    layoutCheck(app.layout);const bad=clone(app.layout);bad[1].slot=bad[0].slot;assert.throws(()=>layoutCheck(bad)); // L4
    assert.equal(display({options:[],unit:'items'},'9007199254740993'),'9007199254740993 items'); // L5
    const refresh=step(state,{type:'Refresh'});state=refresh.state;
    for(const intent of refresh.intents){
      const binding=intent.binding,reply={input:binding.input,class:'Accept',reason:null,post:[app===edit?'4':'1'],checked:'1',total:'1'};
      for(const key of ['epoch','run','request','revision','principal','contract'])
        assert.equal(step(state,{type:'Preview',binding:{...binding,[key]:'stale'},reply}).state,state); // L2
      assert.equal(validReply(app,binding.input,{...reply,checked:'0'}),false); // L6
      state=step(state,{type:'Preview',binding,reply}).state;
      assert.equal(enabled(state,binding.command),true); // L7 positive witness
    }
    const id=app.commands[0].id;
    assert.equal(step(state,{type:'Dispatch',command:id}).intents.length,0); // L8 no consent
    function dispatchWith(width,theme,motion){
      let s=step(state,{type:'Preferences',value:{width,theme,motion}}).state;
      s=step(s,{type:'Review',command:id}).state;s=step(s,{type:'Confirm'}).state;
      return step(s,{type:'Dispatch',command:id});
    }
    const sent=dispatchWith(320,'dark','reduce'),other=dispatchWith(1440,'light','normal');
    assert.deepEqual(sent.intents,other.intents); // L3
    assert.equal(sent.intents.length,1);
    const stopped=step(sent.state,{type:'Fail',binding:sent.state.active}).state;
    assert.equal(stopped.evidence.checked,'0');
    const late={input:sent.state.active.input,class:'Accept',reason:null,post:[app===edit?'4':'1'],checked:'1',total:'1'};
    assert.equal(step(stopped,{type:'Result',binding:sent.state.active,reply:late}).state,stopped); // L6
    let consent=step(state,{type:'Review',command:id}).state;consent=step(consent,{type:'Confirm'}).state;
    for(const changed of [{...consent,principal:'other'}, {...consent,contract:'other'}, {...consent,revision:'1'}])
      assert.equal(step(changed,{type:'Dispatch',command:id}).intents.length,0); // L8 exact subject
  }
});

function globalValue(key,value){const saved=Object.getOwnPropertyDescriptor(globalThis,key);Object.defineProperty(globalThis,key,{value,writable:true,configurable:true});return()=>saved?Object.defineProperty(globalThis,key,saved):delete globalThis[key];}
async function settle(predicate){for(let i=0;i<160;i++){if(predicate())return;await delay(5);}throw Error('timed out waiting for mounted UI');}
async function mounted(app,session,fn){
  const doc=documentFrom('<div id="app"></div>'),root=doc.getElementById('app'),submitted=[];
  const restore=[globalValue('document',doc),globalValue('innerWidth',390),globalValue('addEventListener',()=>{}),globalValue('matchMedia',()=>({matches:false,addEventListener:()=>{}}))];
  let lastView;
  root.addEventListener('zenoui-view',event=>{lastView=event.detail;});
  const transport={terminate(){},postMessage(data){
    if(data.kind==='init')queueMicrotask(()=>this.onmessage({data:{kind:'ready'}}));
    else if(data.kind==='preview')queueMicrotask(()=>this.onmessage({data:{kind:'preview',binding:data.binding,reply:{
      input:data.binding.input,class:'Accept',reason:null,post:[app===edit?'4':data.binding.command==='220'?'1':'2'],checked:'1',total:'1'}}}));
    else submitted.push(data.binding);
  }};
  try{
    const description=clone(app),source='private-idiom-fixture',id=await sha256({description,source});
    await mount(root,{description,source,contract:id},{session,transport});
    await settle(()=>root.dataset.ready==='true');
    await fn({root,transport,submitted,view:()=>lastView});
  }finally{restore.reverse().forEach(reset=>reset());}
}

test('mounted edit shows the current value and uses preview plus explicit confirmation',async()=>{
  await mounted(edit,{contract,principal,revision:'0',values:['2'],context:['1','0'],pending:'1'},async({root,transport,submitted,view:last})=>{
    const card=root.querySelector('[data-idiom="scalar-edit"]');
    assert.ok(card);assert.match(card.textContent,/Current stock/);assert.match(card.textContent,/2 items/);
    assert.equal(root.querySelectorAll('[data-region]').length,8);
    assert.equal(root.querySelector('#context-field-3'),null);assert.equal(root.querySelector('#context-field-4'),null);
    const input=root.querySelector('#command-120-field-2');input.value='7';input.dispatchEvent(new Event('input'));
    await settle(()=>!root.querySelector('button[data-command="120"]').disabled);
    assert.equal(submitted.length,0);
    root.querySelector('form[data-command="120"]').dispatchEvent(new Event('submit',{cancelable:true}));
    const confirmation=root.querySelector('[data-region="confirmation"]');
    assert.match(confirmation.textContent,/Requested stock/);assert.match(confirmation.textContent,/7 items/);
    assert.match(confirmation.textContent,/4 items/,'only the actual preview supplies the change');
    assert.doesNotMatch(confirmation.textContent,/Clock/,'server clock remains private');
    input.value='6';input.dispatchEvent(new Event('input'));
    assert.equal(confirmation.textContent,'','editing cancels consent');
    assert.equal(submitted.length,0);
    await settle(()=>!root.querySelector('button[data-command="120"]').disabled);
    root.querySelector('form[data-command="120"]').dispatchEvent(new Event('submit',{cancelable:true}));
    confirmation.querySelector('button').click();await settle(()=>submitted.length===1);
    const binding=submitted[0];assert.deepEqual(binding.input,['2','120','6','1','0']);
    assert.equal(binding.principal,principal);assert.equal(binding.contract,contract);assert.equal(binding.revision,'0');
    assert.equal(last().delivery,'unknown');
    transport.onmessage({data:{kind:'dispatch',binding:{...binding,principal:'c'.repeat(64)},reply:{input:binding.input,class:'Accept',reason:null,post:['4'],checked:'1',total:'1'},delivery:'pending'}});
    assert.equal(last().delivery,'unknown','wrong-principal reply is ignored');
    transport.onmessage({data:{kind:'dispatch',binding,reply:{input:binding.input,class:'Accept',reason:null,post:['4'],checked:'1',total:'1'},delivery:'pending'}});
    await settle(()=>root.dataset.phase==='done');assert.equal(last().delivery,'pending');
  });
});

test('approval choice selects a command but does not submit before review and confirmation',async()=>{
  await mounted(approval,{contract,principal,revision:'0',values:['0'],context:['1'],pending:'0'},async({root,transport,submitted})=>{
    const card=root.querySelector('[data-idiom="approval-decision"]');assert.ok(card);assert.match(card.textContent,/Pending/);
    assert.equal(root.querySelectorAll('[data-region]').length,8);assert.equal(root.querySelector('#context-field-2'),null);
    const choices=card.querySelectorAll('button[data-decision]');assert.equal(choices.length,2);
    assert.equal(choices[0].getAttribute('aria-pressed'),'true');assert.equal(choices[1].getAttribute('aria-pressed'),'false');
    choices[1].click();await settle(()=>choices[1].getAttribute('aria-pressed')==='true');
    await settle(()=>!root.querySelector('button[data-command="221"]').disabled);
    assert.equal(submitted.length,0,'selection is not an action');
    root.querySelector('form[data-command="221"]').dispatchEvent(new Event('submit',{cancelable:true}));
    const confirmation=root.querySelector('[data-region="confirmation"]');assert.match(confirmation.textContent,/Decline/);
    assert.equal(submitted.length,0,'review is not consent');
    confirmation.querySelector('button').click();await settle(()=>submitted.length===1);
    assert.deepEqual(submitted[0].input,['0','221','1']);assert.equal(submitted[0].principal,principal);
    transport.onmessage({data:{kind:'dispatch',binding:submitted[0],reply:{input:submitted[0].input,class:'Accept',reason:null,post:['2'],checked:'1',total:'1'},delivery:'pending'}});
    await settle(()=>root.dataset.phase==='done');
  });
});
