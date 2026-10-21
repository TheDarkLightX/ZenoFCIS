import assert from 'node:assert/strict';
import {setTimeout as delay} from 'node:timers/promises';

// Enter needs text so Chrome produces the character event for native activation.
// Keep keyboard activation separate from pointer clicks and DOM evaluation.
export async function pressKey(command,sessionId,key) {
 const code={Tab:9,Enter:13}[key];
 if(!code)throw Error(`Unsupported journey key: ${key}`);
 await command('Input.dispatchKeyEvent',{type:'keyDown',key,code:key,
  windowsVirtualKeyCode:code,nativeVirtualKeyCode:code,...(key==='Enter'?{text:'\r'}:{})},sessionId);
 await command('Input.dispatchKeyEvent',{type:'keyUp',key,code:key,
  windowsVirtualKeyCode:code,nativeVirtualKeyCode:code},sessionId);
}

export async function waitFor(evaluate,expression,label,{timeoutMs=20000,intervalMs=30}={}) {
 const deadline=Date.now()+timeoutMs;
 for(;;){
  const actual=await evaluate(expression);
  if(actual)return;
  if(Date.now()>=deadline){
   const error=new assert.AssertionError({message:label,expected:true,actual,operator:'strictEqual'});
   error.browser={expression};throw error;
  }
  await delay(intervalMs);
 }
}

// A bounded snapshot makes native validation and focus failures inspectable.
export const snapshotExpression=`(()=>{
 const describe=e=>e?{tag:e.tagName,id:e.id,type:e.type,name:e.getAttribute('aria-label'),text:e.textContent.slice(0,300),disabled:!!e.disabled}:null;
 return {activeElement:describe(document.activeElement),app:{...document.querySelector('#app')?.dataset},
  buttons:[...document.querySelectorAll('form button,[data-region=confirmation] button')].map(describe),
  invalidControls:[...document.querySelectorAll('input,select')].filter(e=>!e.validity.valid).map(e=>({...describe(e),value:e.value,validationMessage:e.validationMessage})),
  confirmation:document.querySelector('[data-region=confirmation]')?.textContent.slice(0,1000),
  verdict:document.querySelector('[data-region=verdict]')?.textContent.slice(0,1000)};
})()`;

export function failureDetails(error) {
 return {name:error.name,message:error.message,operator:error.operator??null,
  expected:error.expected??null,actual:error.actual??null,context:error.browser??null,
  stack:error.stack??null};
}
