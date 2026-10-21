import assert from 'node:assert/strict';
import {test} from 'node:test';
import {pressKey,waitFor,failureDetails,snapshotExpression} from './browser-checks.mjs';

test('keyboard journey sends Enter character data for native activation; Tab stays navigation',async()=>{
 const events=[];
 const command=async(method,params,session)=>{assert.equal(method,'Input.dispatchKeyEvent');assert.equal(session,'page');events.push(params);};
 await pressKey(command,'page','Tab');await pressKey(command,'page','Enter');
 assert.deepEqual(events.map(e=>[e.type,e.key,e.text??null]),[
  ['keyDown','Tab',null],['keyUp','Tab',null],['keyDown','Enter','\r'],['keyUp','Enter',null]]);
 // The prior sequence cannot deliver an Enter character, even with correct key codes.
 const activation=sequence=>sequence.some(e=>e.type==='keyDown'&&e.key==='Enter'&&e.text==='\r');
 assert.equal(activation(events),true);
 assert.equal(activation(events.map(({text,...event})=>event)),false,'planted omission must prevent character activation');
});

test('failed keyboard predicate retains expected and actual values through report JSON',async()=>{
 const expression='!!document.querySelector("[data-region=confirmation] button")';
 let failure;
 try{await waitFor(async seen=>{assert.equal(seen,expression);return false;},expression,'sample: keyboard review',{timeoutMs:0});}
 catch(error){failure=error;}
 assert.ok(failure);
 const report=JSON.parse(JSON.stringify({failure:failureDetails(failure)}));
 assert.match(report.failure.message,/^sample: keyboard review/);
 assert.equal(report.failure.expected,true);assert.equal(report.failure.actual,false);
 assert.equal(report.failure.operator,'strictEqual');assert.equal(report.failure.context.expression,expression);
});

test('browser failure snapshot includes focus, validation and confirmation evidence',()=>{
 const element=(tag,text,valid=true)=>({tagName:tag,textContent:text,type:'text',id:tag,disabled:false,value:'bad',getAttribute:()=>null,validity:{valid},validationMessage:'Invalid value'});
 const button=element('BUTTON','Review');const input=element('INPUT','',false);
 const confirmation=element('SECTION','No command awaiting confirmation.');
 const document={activeElement:button,querySelector:selector=>selector==='#app'?{dataset:{ready:'true',revision:'0'}}:confirmation,
  querySelectorAll:selector=>selector==='input,select'?[input]:[button]};
 const snapshot=Function('document',`return ${snapshotExpression}`)(document);
 assert.equal(snapshot.activeElement.text,'Review');assert.equal(snapshot.invalidControls[0].validationMessage,'Invalid value');
 assert.equal(snapshot.confirmation,'No command awaiting confirmation.');assert.equal(snapshot.app.revision,'0');
});
