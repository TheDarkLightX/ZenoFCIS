'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {documentFrom}=require('./library_dom.cjs');
const html=fs.readFileSync(path.join(__dirname,'../lens_assets/index.html'),'utf8'),script=fs.readFileSync(path.join(__dirname,'../lens_assets/app.js'),'utf8');
const packet=JSON.parse(fs.readFileSync(process.env.ZAL_LIBRARY_PACKET,'utf8'));
function mount(mobile=false){
  const document=documentFrom(html),events={},states=[];let at=0;
  const window={ZAL_PACKET:packet,location:{hash:''},matchMedia:()=>({matches:mobile}),addEventListener:(name,fn)=>{events[name]=fn;}};
  window.history={state:null,replaceState(state,_,hash){states[at]={state,hash};this.state=state;window.location.hash=hash;},pushState(state,_,hash){states.splice(++at);states[at]={state,hash};this.state=state;window.location.hash=hash;},go(delta){at+=delta;const entry=states[at];this.state=entry.state;window.location.hash=entry.hash;events.popstate({state:entry.state});}};
  const context={window,document,Event,URLSearchParams,console,module:{exports:{}}};vm.runInNewContext(script,context,{filename:'actual-library-browser.js'});return{document,window,states};
}
test('desktop controller preserves source/meaning and selected catalog focus',()=>{
  const {document:doc}=mount();assert.equal(doc.getElementById('app').hidden,false);
  const picker=doc.getElementById('entry-select'),subject=packet.subjects.find(s=>s.object.id==='custom-term');picker.value=subject.object.key;picker.dispatchEvent(new Event('change'));
  assert.equal(doc.getElementById('view-title').textContent,'Retained authored source');assert.equal(doc.activeElement.id,'view-title');
  doc.getElementById('choices-details').open=true;let current=doc.querySelector('[data-lens="retained-source"]');current.focus();
  const event=new Event('keydown',{cancelable:true});Object.defineProperty(event,'key',{value:'ArrowDown'});doc.getElementById('lens-list').dispatchEvent(event);
  assert.equal(doc.activeElement.dataset.lens,'inductive-syntax');assert.equal(doc.getElementById('view-title').textContent,'Retained authored source','arrow navigation must not select');
  doc.activeElement.click();assert.equal(doc.getElementById('view-status').textContent,'Structure checked');
  assert.equal(doc.activeElement.dataset.lens,'inductive-syntax','selected replacement button must retain focus');
  assert.match(doc.getElementById('meaning-status').textContent,/unresolved/);
  doc.getElementById('restore-inline').click();assert.equal(doc.getElementById('view-title').textContent,'Retained authored source');assert.equal(doc.activeElement.id,'view-title');
});
test('mobile committed activation reveals the selected view while arrows only move catalog focus',()=>{
  const {document:doc,window,states}=mount(true),picker=doc.getElementById('entry-select');
  const subject=packet.subjects.find(s=>s.object.id==='custom-term');picker.value=subject.object.key;picker.dispatchEvent(new Event('change'));
  const title=doc.getElementById('view-title'),choices=doc.getElementById('choices-details');choices.open=true;
  doc.querySelector('[data-lens="retained-source"]').focus();const before=window.location.hash,count=states.length;
  const arrow=new Event('keydown',{cancelable:true});Object.defineProperty(arrow,'key',{value:'ArrowDown'});doc.getElementById('lens-list').dispatchEvent(arrow);
  assert.equal(doc.activeElement.dataset.lens,'inductive-syntax');assert.equal(title.textContent,'Retained authored source');
  assert.equal(window.location.hash,before);assert.equal(states.length,count);assert.equal(title.scrollCalls??0,0);
  doc.activeElement.click();assert.equal(doc.activeElement.id,'view-title');assert.equal(doc.getElementById('view-status').textContent,'Structure checked');
  assert.equal(title.scrollCalls,1);assert.equal(title.lastFocusOptions.preventScroll,true);assert.equal(title.lastScrollOptions.block,'start');assert.equal(title.lastScrollOptions.behavior,'instant');
  assert.equal(choices.open,true);assert.equal(doc.getElementById('object-id').textContent,subject.object.entryDigest);
  const selectedHash=window.location.hash,selectedCount=states.length;
  doc.querySelector('[data-lens="inductive-syntax"]').focus();doc.activeElement.click();
  assert.equal(doc.activeElement.id,'view-title');assert.equal(title.scrollCalls,2);assert.equal(window.location.hash,selectedHash);assert.equal(states.length,selectedCount);
});
test('catalog activation uses the current viewport policy after resize',()=>{
  const {document:doc,window}=mount(false),title=doc.getElementById('view-title');
  const target=()=>doc.querySelector('[data-lens="sequence-chunks"]');target().focus();target().click();
  assert.equal(doc.activeElement.dataset.lens,'sequence-chunks');assert.equal(title.scrollCalls??0,0);
  window.matchMedia=()=>({matches:true});target().focus();target().click();
  assert.equal(doc.activeElement.id,'view-title');assert.equal(title.scrollCalls,1);
  window.matchMedia=()=>({matches:false});target().focus();target().click();
  assert.equal(doc.activeElement.dataset.lens,'sequence-chunks');assert.equal(title.scrollCalls,1);
});
test('actual controller changes exact subjects and restores history without source substitution',()=>{
  const {document:doc}=mount(),picker=doc.getElementById('entry-select');
  for(const id of ['custom-graph','counter','phase']){const s=packet.subjects.find(s=>s.object.id===id);picker.value=s.object.key;picker.dispatchEvent(new Event('change'));assert.equal(doc.getElementById('object-id').textContent,s.object.entryDigest);assert.equal(doc.getElementById('source-text').textContent,s.object._sourceText);}
  doc.getElementById('history-back').click();assert.equal(doc.getElementById('object-id').textContent,packet.subjects.find(s=>s.object.id==='counter').object.entryDigest);
  doc.querySelector('[data-lens="unit-group"]').click();assert.equal(doc.getElementById('view-status').textContent,'Finite claim checked');assert.match(doc.getElementById('operations-controls').textContent,/Same final effect/);
});
